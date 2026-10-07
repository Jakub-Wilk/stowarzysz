"""Nickname / profile-picture votes, and the 72h deadline that applies to every poll."""

from datetime import timedelta
from pathlib import Path
from typing import Any

import pytest
from django.utils import timezone
from rest_framework.test import APIClient

from accounts.models import User
from tests.test_avatars import make_image, stored_files
from voting import services
from voting.models import Poll, PollParticipant

POLLS = "/api/polls/"


@pytest.fixture(autouse=True)
def _run_on_commit_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("django.db.transaction.on_commit", lambda fn, **kw: fn())


@pytest.fixture(autouse=True)
def _media_root(settings, tmp_path: Path) -> None:
    settings.MEDIA_ROOT = tmp_path


@pytest.fixture(autouse=True)
def _no_events(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("core.events.notify_user", lambda uid, t, d: None)


@pytest.fixture
def pushes(monkeypatch: pytest.MonkeyPatch) -> list[dict[str, Any]]:
    sent: list[dict[str, Any]] = []
    monkeypatch.setattr(
        "voting.services.send_push",
        lambda user_ids, **kw: sent.append({"user_ids": sorted(user_ids), **kw}),
    )
    return sent


@pytest.fixture
def alice(db) -> User:
    return User.objects.create(username="alice")


@pytest.fixture
def bob(db) -> User:
    return User.objects.create(username="bob")


@pytest.fixture
def carol(db) -> User:
    return User.objects.create(username="carol")


def client_for(user: User) -> APIClient:
    client = APIClient()
    client.force_authenticate(user)
    return client


def propose_nickname(proposer: User, target: User, new_username: str = "bobby") -> Any:
    config = {"target_user_id": target.pk, "new_username": new_username}
    return client_for(proposer).post(POLLS, {"kind": "nickname", "config": config}, format="json")


def propose_avatar(proposer: User, target: User, *, remove: bool = False, image: Any = None) -> Any:
    data: dict[str, Any] = {
        "kind": "avatar",
        "config": f'{{"target_user_id": {target.pk}, "remove": {str(remove).lower()}}}',
    }
    if image is not None or not remove:
        data["image"] = image or make_image()
    return client_for(proposer).post(POLLS, data, format="multipart")


def vote_all(poll_id: int, scores: dict[User, int]) -> None:
    for user, value in scores.items():
        resp = client_for(user).put(f"{POLLS}{poll_id}/ballot/", {"ballot": {"value": value}})
        assert resp.status_code == 200, resp.content


# --- nickname ----------------------------------------------------------------


def test_nickname_requires_auth(api_client: APIClient, bob: User) -> None:
    config = {"target_user_id": bob.pk, "new_username": "x"}
    assert api_client.post(POLLS, {"kind": "nickname", "config": config}).status_code == 401


def test_nickname_poll_includes_everyone_and_has_a_generated_title(
    alice: User, bob: User, carol: User
) -> None:
    resp = propose_nickname(alice, bob)
    assert resp.status_code == 201
    body = resp.json()
    assert body["title"] == "Zmiana nicku: bob → bobby"
    assert {p["user"]["id"] for p in body["participants"]} == {alice.pk, bob.pk, carol.pk}
    assert body["can_close"] is False


def test_cannot_target_yourself(alice: User) -> None:
    resp = propose_nickname(alice, alice)
    assert resp.status_code == 400
    assert Poll.objects.count() == 0


@pytest.mark.parametrize("new_username", ["", "bob", "alice", "zły nick!", "dwie  spacje"])
def test_nickname_rejects_bad_names(alice: User, bob: User, new_username: str) -> None:
    assert propose_nickname(alice, bob, new_username).status_code == 400


def test_nickname_may_contain_single_spaces(alice: User, bob: User, carol: User) -> None:
    poll_id = propose_nickname(alice, bob, "Jan Kowalski").json()["id"]
    vote_all(poll_id, {alice: 5, bob: 5, carol: 5})
    bob.refresh_from_db()
    assert bob.username == "Jan Kowalski"


def test_nickname_rejects_unknown_and_inactive_target(alice: User, bob: User) -> None:
    User.objects.filter(pk=bob.pk).update(is_active=False)
    assert propose_nickname(alice, bob).status_code == 400
    config = {"target_user_id": 9999, "new_username": "x"}
    resp = client_for(alice).post(POLLS, {"kind": "nickname", "config": config}, format="json")
    assert resp.status_code == 400


def test_approved_nickname_vote_renames_the_target(alice: User, bob: User, carol: User) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    vote_all(poll_id, {alice: 2, bob: 0, carol: 1})  # average 1: passes
    poll = Poll.objects.get(pk=poll_id)
    assert poll.status == "closed" and poll.result["approved"] is True
    assert poll.result["applied"] is True
    bob.refresh_from_db()
    assert bob.username == "bobby"


@pytest.mark.parametrize("scores", [(1, 0, 1), (-5, 5, 2), (0, 0, 0)])
def test_nickname_vote_below_threshold_changes_nothing(
    alice: User, bob: User, carol: User, scores: tuple[int, int, int]
) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    vote_all(poll_id, dict(zip((alice, bob, carol), scores, strict=True)))
    poll = Poll.objects.get(pk=poll_id)
    assert poll.result["approved"] is False and poll.result["applied"] is False
    bob.refresh_from_db()
    assert bob.username == "bob"


def test_profile_votes_cannot_be_vetoed(alice: User, bob: User, carol: User) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    assert client_for(carol).get(f"{POLLS}{poll_id}/").json()["can_veto"] is False
    assert client_for(carol).post(f"{POLLS}{poll_id}/veto/").status_code == 400
    vote_all(poll_id, {alice: 5, bob: 5, carol: 5})
    bob.refresh_from_db()
    assert bob.username == "bobby"


def test_name_taken_while_voting_is_reported_not_applied(
    alice: User, bob: User, carol: User
) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    User.objects.create(username="bobby")
    vote_all(poll_id, {alice: 5, bob: 5, carol: 5})
    result = Poll.objects.get(pk=poll_id).result
    assert result["approved"] is True and result["applied"] is False and result["apply_error"]
    bob.refresh_from_db()
    assert bob.username == "bob"


def test_special_votes_cannot_be_closed_early(alice: User, bob: User) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    resp = client_for(alice).post(f"{POLLS}{poll_id}/close/")
    assert resp.status_code == 400
    assert Poll.objects.get(pk=poll_id).status == "open"


# --- avatar ------------------------------------------------------------------


def test_avatar_requires_auth(api_client: APIClient, bob: User) -> None:
    resp = api_client.post(POLLS, {"kind": "avatar", "image": make_image()}, format="multipart")
    assert resp.status_code == 401


def test_avatar_poll_stores_the_normalised_image_and_exposes_it(
    alice: User, bob: User, settings
) -> None:
    resp = propose_avatar(alice, bob)
    assert resp.status_code == 201
    body = resp.json()
    assert body["title"] == "Zmiana zdjęcia: bob"
    assert body["proposed_avatar_url"].endswith(".webp")
    assert len(stored_files(settings)) == 1


def test_avatar_poll_validation(alice: User, bob: User, settings) -> None:
    assert propose_avatar(alice, alice).status_code == 400  # not yourself
    assert propose_avatar(alice, bob, image=make_image(name="x.png")).status_code == 201
    no_image = client_for(alice).post(
        POLLS,
        {"kind": "avatar", "config": f'{{"target_user_id": {bob.pk}, "remove": false}}'},
        format="multipart",
    )
    assert no_image.status_code == 400
    both = propose_avatar(alice, bob, remove=True, image=make_image())
    assert both.status_code == 400
    assert Poll.objects.count() == 1
    assert len(stored_files(settings)) == 1  # nothing left behind by rejected proposals


def test_image_is_rejected_for_other_kinds(alice: User) -> None:
    resp = client_for(alice).post(
        POLLS, {"title": "x", "kind": "score", "image": make_image()}, format="multipart"
    )
    assert resp.status_code == 400


def test_approved_avatar_vote_replaces_the_picture_and_cleans_up(
    alice: User, bob: User, carol: User, settings
) -> None:
    poll_id = propose_avatar(alice, bob).json()["id"]
    vote_all(poll_id, {alice: 3, bob: 3, carol: 3})
    poll = Poll.objects.get(pk=poll_id)
    assert poll.result["applied"] is True
    bob.refresh_from_db()
    assert bob.avatar
    assert [p.name for p in stored_files(settings)] == [Path(str(bob.avatar_file.name)).name]
    assert poll.proposed_avatar.name == ""


def test_rejected_avatar_vote_discards_the_pending_image(
    alice: User, bob: User, carol: User, settings
) -> None:
    poll_id = propose_avatar(alice, bob).json()["id"]
    vote_all(poll_id, {alice: -1, bob: -1, carol: -1})
    bob.refresh_from_db()
    assert not bob.avatar
    assert stored_files(settings) == []


def test_avatar_removal_vote(alice: User, bob: User, carol: User, settings) -> None:
    from django.core.files.base import ContentFile

    bob.set_avatar(ContentFile(b"RIFF"))
    poll_id = propose_avatar(alice, bob, remove=True).json()["id"]
    assert propose_avatar(alice, bob, remove=True).json()["title"] == "Usunięcie zdjęcia: bob"
    vote_all(poll_id, {alice: 5, bob: 5, carol: 5})
    bob.refresh_from_db()
    assert not bob.avatar


# --- deadlines ---------------------------------------------------------------


@pytest.fixture
def old_poll(alice: User, bob: User, carol: User) -> Poll:
    poll = Poll.objects.create(creator=alice, title="stare", kind="score")
    PollParticipant.objects.bulk_create(
        PollParticipant(poll=poll, user=u) for u in (alice, bob, carol)
    )
    return poll


def age(poll: Poll, hours: float) -> None:
    Poll.objects.filter(pk=poll.pk).update(created_at=timezone.now() - timedelta(hours=hours))


def test_nothing_happens_before_24h(old_poll: Poll, pushes: list) -> None:
    age(old_poll, 23)
    assert services.process_deadlines() == (0, 0)
    assert pushes == []


def test_reminders_go_to_non_voters_once_per_stage(
    old_poll: Poll, alice: User, bob: User, carol: User, pushes: list
) -> None:
    client_for(alice).put(f"{POLLS}{old_poll.pk}/ballot/", {"ballot": {"value": 1}})
    for hours, left in ((25, 48), (49, 24), (70, 3)):
        age(old_poll, hours)
        assert services.process_deadlines() == (0, 1)
        assert services.process_deadlines() == (0, 0)  # idempotent
        assert pushes[-1]["user_ids"] == sorted([bob.pk, carol.pk])
        assert pushes[-1]["body"].endswith(f"za {left} godz.")
    assert len(pushes) == 3


def test_downtime_sends_only_the_latest_reminder(old_poll: Poll, pushes: list) -> None:
    age(old_poll, 50)
    assert services.process_deadlines() == (0, 1)
    assert len(pushes) == 1 and pushes[0]["body"].endswith("za 24 godz.")


def test_poll_expires_after_72h(
    old_poll: Poll, alice: User, bob: User, carol: User, pushes: list
) -> None:
    age(old_poll, 72.1)
    assert services.process_deadlines() == (1, 0)
    old_poll.refresh_from_db()
    assert old_poll.status == "closed" and old_poll.close_reason == "expired"
    assert len(pushes) == 1 and pushes[0]["title"] == "Głosowanie zakończone"
    assert pushes[0]["user_ids"] == sorted([alice.pk, bob.pk, carol.pk])
    assert services.process_deadlines() == (0, 0)  # closed polls are ignored


def test_everyone_is_notified_when_a_vote_ends(
    alice: User, bob: User, carol: User, pushes: list
) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    pushes.clear()  # the "new vote" push
    vote_all(poll_id, {alice: 5, bob: 5, carol: 5})
    assert len(pushes) == 1
    assert pushes[0]["user_ids"] == sorted([alice.pk, bob.pk, carol.pk])
    assert pushes[0]["body"] == "Zmiana nicku: bob → bobby: zmiana zastosowana"
    assert pushes[0]["url"] == f"/voting/{poll_id}"


def test_closing_early_notifies_everyone(alice: User, bob: User, carol: User, pushes: list) -> None:
    poll = Poll.objects.create(creator=alice, title="zwykłe", kind="score")
    PollParticipant.objects.bulk_create(PollParticipant(poll=poll, user=u) for u in (alice, bob))
    assert client_for(alice).post(f"{POLLS}{poll.pk}/close/").status_code == 200
    assert pushes[-1]["user_ids"] == sorted([alice.pk, bob.pk])
    assert pushes[-1]["body"] == "zwykłe"


def test_expiry_applies_an_approved_change(alice: User, bob: User, carol: User) -> None:
    poll_id = propose_nickname(alice, bob).json()["id"]
    vote_all_but_bob = {alice: 3, carol: 1}
    vote_all(poll_id, vote_all_but_bob)  # bob never votes; the average of those cast is 2
    age(Poll.objects.get(pk=poll_id), 73)
    assert services.process_deadlines() == (1, 0)
    bob.refresh_from_db()
    assert bob.username == "bobby"


def test_expiry_with_no_votes_is_not_approved(alice: User, bob: User, settings) -> None:
    poll_id = propose_avatar(alice, bob).json()["id"]
    age(Poll.objects.get(pk=poll_id), 73)
    services.process_deadlines()
    poll = Poll.objects.get(pk=poll_id)
    assert poll.result["approved"] is False
    assert stored_files(settings) == []


def test_command_runs(old_poll: Poll, capsys) -> None:
    from django.core.management import call_command

    age(old_poll, 80)
    call_command("process_poll_deadlines")
    assert "Closed 1 poll" in capsys.readouterr().out
