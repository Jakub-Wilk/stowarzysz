import json
from typing import Any

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from voting import kinds
from voting.kinds import Entry, PollKind, ScoreKind
from voting.models import Poll, PollParticipant

POLLS = "/api/polls/"


def url(poll: Poll | int, suffix: str = "") -> str:
    pk = poll.pk if isinstance(poll, Poll) else poll
    return f"{POLLS}{pk}/{suffix}"


@pytest.fixture(autouse=True)
def _run_on_commit_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    """Tests run in a transaction, so on_commit hooks would never fire."""
    monkeypatch.setattr("django.db.transaction.on_commit", lambda fn, **kw: fn())


@pytest.fixture
def events(monkeypatch: pytest.MonkeyPatch) -> list[tuple[int, str, Any]]:
    sent: list[tuple[int, str, Any]] = []
    monkeypatch.setattr("voting.events.notify_user", lambda uid, t, d: sent.append((uid, t, d)))
    return sent


@pytest.fixture
def pushes(monkeypatch: pytest.MonkeyPatch) -> list[dict[str, Any]]:
    sent: list[dict[str, Any]] = []

    def fake_send_push(user_ids, **kwargs):
        sent.append({"user_ids": sorted(user_ids), **kwargs})

    monkeypatch.setattr("voting.services.send_push", fake_send_push)
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


@pytest.fixture
def dave(db) -> User:
    """Never takes part in anything."""
    return User.objects.create(username="dave")


def client_for(user: User) -> APIClient:
    client = APIClient()
    client.force_authenticate(user)
    return client


@pytest.fixture
def poll(alice, bob, carol, events, pushes) -> Poll:
    """Open score vote called by alice with bob and carol."""
    resp = client_for(alice).post(POLLS, {"title": "Pizza?", "participant_ids": [bob.pk, carol.pk]})
    assert resp.status_code == 201, resp.json()
    events.clear()
    pushes.clear()
    return Poll.objects.get(pk=resp.json()["id"])


def vote(user: User, poll: Poll, value: int) -> Any:
    return client_for(user).put(url(poll, "ballot/"), {"ballot": {"value": value}})


# --- auth -------------------------------------------------------------------


def test_every_endpoint_requires_auth(api_client: APIClient, poll: Poll) -> None:
    calls = [
        api_client.get(POLLS),
        api_client.post(POLLS, {"title": "x"}),
        api_client.get(url(poll)),
        api_client.put(url(poll, "ballot/"), {"ballot": {"value": 1}}),
        api_client.post(url(poll, "veto/")),
        api_client.post(url(poll, "close/")),
        api_client.post(url(poll, "reactions/"), {"emoji": "❤️"}),
        api_client.get("/api/auth/people/"),
    ]
    assert [c.status_code for c in calls] == [401] * len(calls)


# --- create -----------------------------------------------------------------


def test_create_includes_creator_and_notifies_the_others(
    alice, bob, carol, dave, events, pushes
) -> None:
    resp = client_for(alice).post(POLLS, {"title": "Pizza?", "participant_ids": [bob.pk]})
    assert resp.status_code == 201
    body = resp.json()
    assert body["status"] == "open"
    assert body["kind"] == "score"
    assert body["creator"]["username"] == "alice"
    assert [p["user"]["username"] for p in body["participants"]] == ["alice", "bob"]
    assert body["can_vote"] is True and body["can_close"] is True

    # Push: participants except the creator; never non-participants.
    assert pushes == [
        {
            "user_ids": [bob.pk],
            "title": "Nowe głosowanie w Sejmiku",
            "body": "Pizza?",
            "url": f"/voting/{body['id']}",
        }
    ]
    # SSE: everyone is told (ids only), so lists update live for non-participants too.
    created = [(uid, data) for uid, kind, data in events if kind == "poll.created"]
    assert sorted(uid for uid, _ in created) == sorted([alice.pk, bob.pk, carol.pk, dave.pk])
    assert all(data == {"poll_id": body["id"]} for _, data in created)


def test_creator_alone_is_allowed(alice, pushes) -> None:
    resp = client_for(alice).post(POLLS, {"title": "Solo"})
    assert resp.status_code == 201
    assert len(resp.json()["participants"]) == 1
    assert pushes[0]["user_ids"] == []


@pytest.mark.parametrize(
    "payload",
    [
        {"title": ""},
        {"title": "x" * 201},
        {"title": "ok", "kind": "nope"},
        {"title": "ok", "config": {"surprise": 1}},
        {"title": "ok", "participant_ids": [999999]},
        {"title": "ok", "participant_ids": ["a"]},
    ],
)
def test_create_validation(alice, payload: dict[str, Any]) -> None:
    assert client_for(alice).post(POLLS, payload).status_code == 400
    assert Poll.objects.count() == 0


def test_cannot_add_inactive_participant(alice) -> None:
    gone = User.objects.create(username="gone", is_active=False)
    resp = client_for(alice).post(POLLS, {"title": "x", "participant_ids": [gone.pk]})
    assert resp.status_code == 400


# --- non-participants -------------------------------------------------------


def test_non_participant_can_read_but_not_vote(poll: Poll, dave) -> None:
    client = client_for(dave)
    listed = client.get(POLLS, {"status": "open"}).json()
    assert [p["id"] for p in listed] == [poll.pk]
    assert listed[0]["my"] == {"participating": False, "has_voted": False, "vetoed": False}

    detail = client.get(url(poll)).json()
    assert detail["can_vote"] is False and detail["can_close"] is False
    assert detail["my_ballot"] is None

    assert vote(dave, poll, 3).status_code == 403
    assert client.post(url(poll, "veto/")).status_code == 403
    assert client.post(url(poll, "close/")).status_code == 403
    assert PollParticipant.objects.filter(poll=poll, voted_at__isnull=False).count() == 0


def test_unknown_poll_is_404(alice) -> None:
    client = client_for(alice)
    assert client.get(url(9999)).status_code == 404
    assert client.put(url(9999, "ballot/"), {"ballot": {"value": 1}}).status_code == 404


# --- ballots ----------------------------------------------------------------


@pytest.mark.parametrize("value", [-6, 6, "3", 2.5, True, None])
def test_invalid_ballots_rejected(poll: Poll, bob, value: Any) -> None:
    assert vote(bob, poll, value).status_code == 400
    assert not PollParticipant.objects.get(poll=poll, user=bob).has_ballot


@pytest.mark.parametrize("ballot", [{}, 3, "x", [1]])
def test_malformed_ballot_shapes(poll: Poll, bob, ballot: Any) -> None:
    resp = client_for(bob).put(url(poll, "ballot/"), {"ballot": ballot})
    assert resp.status_code == 400


def test_extra_ballot_keys_are_dropped(poll: Poll, bob) -> None:
    resp = client_for(bob).put(url(poll, "ballot/"), {"ballot": {"value": 1, "extra": 2}})
    assert resp.json()["my_ballot"] == {"value": 1}


@pytest.mark.parametrize("value", [-5, 0, 5])
def test_valid_boundaries(poll: Poll, bob, value: int) -> None:
    resp = vote(bob, poll, value)
    assert resp.status_code == 200
    assert resp.json()["my_ballot"] == {"value": value}


def test_can_change_vote_until_the_end(poll: Poll, bob) -> None:
    vote(bob, poll, 5)
    assert vote(bob, poll, -2).json()["my_ballot"] == {"value": -2}


# --- hidden until the end ---------------------------------------------------


def test_votes_stay_hidden_while_open(poll: Poll, alice, bob, carol, dave, events) -> None:
    vote(bob, poll, 4)
    client_for(carol).post(url(poll, "veto/"))

    for viewer in (alice, dave):  # a participant who hasn't voted, and an outsider
        detail = client_for(viewer).get(url(poll))
        text = json.dumps(detail.json())
        row = {p["user"]["username"]: p for p in detail.json()["participants"]}
        assert row["bob"] == {"user": row["bob"]["user"], "has_voted": True}
        assert row["carol"]["has_voted"] is True
        assert all("ballot" not in r and "vetoed" not in r for r in row.values())
        assert detail.json()["result"] is None
        assert '"value"' not in text and 'vetoed": true' not in text
        listing = json.dumps(client_for(viewer).get(POLLS, {"status": "open"}).json())
        assert '"value"' not in listing

    # you can see your own vote and veto, and nobody else's
    mine = client_for(bob).get(url(poll)).json()
    assert mine["my_ballot"] == {"value": 4}
    assert client_for(carol).get(url(poll)).json()["my"]["vetoed"] is True

    # live events carry ids only
    assert events and all(data == {"poll_id": poll.pk} for _, _, data in events)


# --- finishing --------------------------------------------------------------


def test_auto_closes_when_everyone_voted(poll: Poll, alice, bob, carol, events) -> None:
    vote(alice, poll, 3)
    vote(bob, poll, -1)
    assert Poll.objects.get(pk=poll.pk).status == "open"
    events.clear()
    resp = vote(carol, poll, 1)

    body = resp.json()
    assert body["status"] == "closed" and body["close_reason"] == "auto"
    assert body["result"] == {
        "votes_cast": 3,
        "veto_count": 0,
        "vetoed": False,
        "score": 1.0,
        "tone": "positive",
    }
    revealed = {p["user"]["username"]: p for p in body["participants"]}
    assert revealed["alice"]["ballot"] == {"value": 3} and revealed["alice"]["vetoed"] is False
    assert revealed["bob"]["ballot"] == {"value": -1}
    assert body["can_vote"] is False and body["can_close"] is False
    assert {kind for _, kind, _ in events} == {"poll.closed"}


def test_no_voting_after_the_end(poll: Poll, alice, bob, carol) -> None:
    for user, value in ((alice, 1), (bob, 1), (carol, 1)):
        vote(user, poll, value)
    assert vote(bob, poll, 5).status_code == 409
    assert client_for(bob).post(url(poll, "veto/")).status_code == 409
    assert client_for(alice).post(url(poll, "close/")).status_code == 409


def test_result_is_frozen(poll: Poll, alice, bob, carol) -> None:
    for user, value in ((alice, 5), (bob, 5), (carol, 5)):
        vote(user, poll, value)
    frozen = Poll.objects.get(pk=poll.pk).result
    PollParticipant.objects.filter(poll=poll).update(ballot={"value": -5})  # tampering
    assert client_for(alice).get(url(poll)).json()["result"] == frozen


# --- veto -------------------------------------------------------------------


def test_veto_counts_as_minus_five_and_is_flagged(poll: Poll, alice, bob, carol) -> None:
    vote(alice, poll, 5)
    vote(bob, poll, 5)
    resp = client_for(carol).post(url(poll, "veto/"))
    body = resp.json()
    assert body["status"] == "closed"
    assert body["result"]["vetoed"] is True and body["result"]["veto_count"] == 1
    assert body["result"]["score"] == pytest.approx(5 / 3, abs=1e-3)
    row = {p["user"]["username"]: p for p in body["participants"]}
    assert row["carol"]["ballot"] == {"value": -5} and row["carol"]["vetoed"] is True


def test_veto_overrides_an_earlier_vote_and_cannot_be_undone(poll: Poll, bob) -> None:
    vote(bob, poll, 5)
    client_for(bob).post(url(poll, "veto/"))
    assert vote(bob, poll, 5).status_code == 400
    assert PollParticipant.objects.get(poll=poll, user=bob).ballot == {"value": -5}


# --- ending early -----------------------------------------------------------


def test_only_the_creator_can_end_early(poll: Poll, bob, dave) -> None:
    assert client_for(bob).post(url(poll, "close/")).status_code == 403
    assert client_for(dave).post(url(poll, "close/")).status_code == 403
    assert Poll.objects.get(pk=poll.pk).status == "open"


def test_early_end_averages_cast_ballots(poll: Poll, alice, bob) -> None:
    vote(bob, poll, 4)
    resp = client_for(alice).post(url(poll, "close/"))
    body = resp.json()
    assert body["close_reason"] == "creator"
    assert body["result"]["votes_cast"] == 1 and body["result"]["score"] == 4.0
    row = {p["user"]["username"]: p for p in body["participants"]}
    assert row["carol"]["has_voted"] is False and row["carol"]["ballot"] is None


def test_early_end_with_no_ballots(poll: Poll, alice) -> None:
    body = client_for(alice).post(url(poll, "close/")).json()
    assert body["result"]["score"] is None and body["result"]["tone"] is None
    assert body["result"]["votes_cast"] == 0


# --- tones ------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "tone"),
    [
        ([1, 0], "neutral"),  # exactly +0.5
        ([-1, 0], "neutral"),  # exactly -0.5
        ([0], "neutral"),
        ([2, 0, 0, 0], "neutral"),  # +0.5 again, with more voters
        ([1, 1, 0], "positive"),  # 0.667
        ([3, 0, 0, 0], "positive"),
        ([-1, -1, 0], "negative"),
        ([-3, 0, 0, 0], "negative"),
    ],
)
def test_tone_thresholds(values: list[int], tone: str) -> None:
    entries = [Entry({"value": v}, False) for v in values]
    assert ScoreKind().compute_result({}, entries)["tone"] == tone


# --- listing ----------------------------------------------------------------


def test_history_is_paginated_newest_first(alice, bob) -> None:
    for i in range(25):
        Poll.objects.create(
            creator=alice, title=f"p{i}", kind="score", status="closed", result={"score": 1}
        )
    client = client_for(bob)
    first = client.get(POLLS, {"status": "closed"}).json()
    assert len(first["results"]) == 20 and first["next"]
    second = client.get(first["next"]).json()
    assert len(second["results"]) == 5 and second["next"] is None
    titles = [p["title"] for p in first["results"] + second["results"]]
    assert titles == [f"p{i}" for i in reversed(range(25))]


def test_filters(poll: Poll, alice, dave) -> None:
    closed = Poll.objects.create(creator=dave, title="old", kind="score", status="closed")
    client = client_for(alice)
    assert [p["id"] for p in client.get(POLLS, {"status": "open"}).json()] == [poll.pk]
    assert [p["id"] for p in client.get(POLLS, {"status": "closed"}).json()["results"]] == [
        closed.pk
    ]
    mine = client.get(POLLS, {"participating": "1"}).json()["results"]
    assert [p["id"] for p in mine] == [poll.pk]


def test_list_item_progress(poll: Poll, bob) -> None:
    vote(bob, poll, 1)
    item = client_for(bob).get(POLLS, {"status": "open"}).json()[0]
    assert item["participant_count"] == 3 and item["voted_count"] == 1
    assert item["my"] == {"participating": True, "has_voted": True, "vetoed": False}


# --- reactions --------------------------------------------------------------


def finish(poll: Poll, alice: User, bob: User, carol: User) -> None:
    for user in (alice, bob, carol):
        vote(user, poll, 1)


def test_reactions_only_on_finished_votes(poll: Poll, bob) -> None:
    assert client_for(bob).post(url(poll, "reactions/"), {"emoji": "❤️"}).status_code == 400


def test_reaction_reaches_every_active_user(poll: Poll, alice, bob, carol, dave, events) -> None:
    User.objects.create(username="gone", is_active=False)
    finish(poll, alice, bob, carol)
    events.clear()
    resp = client_for(dave).post(url(poll, "reactions/"), {"emoji": "🤣"})  # an outsider
    assert resp.status_code == 204
    got = sorted(uid for uid, kind, _ in events if kind == "poll.reaction")
    assert got == sorted([alice.pk, bob.pk, carol.pk, dave.pk])
    assert events[0][2] == {"poll_id": poll.pk, "emoji": "🤣", "user_id": dave.pk}


@pytest.mark.parametrize("emoji", ["❤️", "😭", "👎", "🤣", "🔥"])
def test_allowed_reactions(poll: Poll, alice, bob, carol, emoji: str) -> None:
    finish(poll, alice, bob, carol)
    assert client_for(bob).post(url(poll, "reactions/"), {"emoji": emoji}).status_code == 204


@pytest.mark.parametrize("emoji", ["👍", "", "x", None])
def test_other_emoji_rejected(poll: Poll, alice, bob, carol, emoji: Any) -> None:
    finish(poll, alice, bob, carol)
    assert client_for(bob).post(url(poll, "reactions/"), {"emoji": emoji}).status_code == 400


def test_reactions_can_be_spammed(poll: Poll, alice, bob, carol) -> None:
    finish(poll, alice, bob, carol)
    client = client_for(bob)
    codes = {client.post(url(poll, "reactions/"), {"emoji": "🔥"}).status_code for _ in range(100)}
    assert codes == {204}


# --- directory --------------------------------------------------------------


def test_people_lists_active_activated_users(alice, bob) -> None:
    User.objects.create(username="gone", is_active=False)
    pending = User(username="pending")
    pending.set_unusable_password()
    pending.save()
    rows = client_for(alice).get("/api/auth/people/").json()
    assert [r["username"] for r in rows] == ["alice", "bob"]
    assert set(rows[0]) == {"id", "username", "avatar_url"}


# --- extensibility ----------------------------------------------------------


class YesNoKind(PollKind):
    """Stand-in for a future kind: proves the registry is the only extension point."""

    key = "yesno"

    def validate_config(self, config):
        return {}

    def validate_ballot(self, config, ballot):
        if ballot not in ({"yes": True}, {"yes": False}):
            raise kinds.serializers.ValidationError("bad ballot")
        return ballot

    def veto_ballot(self, config):
        return {"yes": False}

    def compute_result(self, config, entries):
        yes = sum(1 for e in entries if e.ballot["yes"])
        return {"yes": yes, "no": len(entries) - yes, "tone": "positive" if yes else "negative"}


def test_new_kinds_plug_in_through_the_registry(
    alice, bob, monkeypatch: pytest.MonkeyPatch, events, pushes
) -> None:
    monkeypatch.setitem(kinds.KINDS, "yesno", YesNoKind())
    client = client_for(alice)
    created = client.post(
        POLLS, {"title": "Ship it?", "kind": "yesno", "participant_ids": [bob.pk]}
    )
    assert created.status_code == 201 and created.json()["kind"] == "yesno"
    pid = created.json()["id"]
    assert client.put(url(pid, "ballot/"), {"ballot": {"value": 3}}).status_code == 400
    client.put(url(pid, "ballot/"), {"ballot": {"yes": True}})
    body = client_for(bob).put(url(pid, "ballot/"), {"ballot": {"yes": False}}).json()
    assert body["status"] == "closed"
    assert body["result"] == {"yes": 1, "no": 1, "tone": "positive"}
