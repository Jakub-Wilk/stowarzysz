from datetime import timedelta
from typing import Any

import pytest
from django.utils import timezone
from rest_framework.test import APIClient

from accounts.models import User
from secretsanta import services
from secretsanta.models import SantaAssignment, SantaEvent, SantaGift, SantaHelpRequest

URL = "/api/secret-santa/"
HISTORY = f"{URL}history/"
HELP = f"{URL}help/"


@pytest.fixture(autouse=True)
def _run_on_commit_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("django.db.transaction.on_commit", lambda fn, **kw: fn())


@pytest.fixture
def events(monkeypatch: pytest.MonkeyPatch) -> list[tuple[int, str, Any]]:
    sent: list[tuple[int, str, Any]] = []

    def record(uid: int, t: str, d: Any) -> None:
        sent.append((uid, t, d))

    monkeypatch.setattr("core.events.notify_user", record)
    monkeypatch.setattr("secretsanta.services.notify_user", record)
    return sent


@pytest.fixture
def pushes(monkeypatch: pytest.MonkeyPatch) -> list[dict[str, Any]]:
    sent: list[dict[str, Any]] = []

    def fake_send_push(user_ids, **kwargs):
        sent.append({"user_ids": sorted(user_ids), **kwargs})

    monkeypatch.setattr("secretsanta.services.send_push", fake_send_push)
    return sent


def client_for(user: User) -> APIClient:
    client = APIClient()
    client.force_authenticate(user)
    return client


@pytest.fixture
def boss(db) -> User:
    return User.objects.create_superuser("boss", "admin-pass-123")


@pytest.fixture
def people(db) -> list[User]:
    return [User.objects.create(username=n) for n in ("alice", "bob", "carol", "dave", "erin")]


def deadline(days: int = 10) -> str:
    return (timezone.now() + timedelta(days=days)).isoformat()


def start(boss: User, people: list[User], tiers=(100, 30), **extra: Any) -> Any:
    return client_for(boss).post(
        URL,
        {
            "participant_ids": [p.pk for p in people],
            "deadline": deadline(),
            "gift_tiers": list(tiers),
            **extra,
        },
        format="json",
    )


def pairing(people: list[User]) -> dict[int, int]:
    return {p.pk: client_for(p).get(URL).data["my_victim"]["id"] for p in people}


# --- auth ---


@pytest.mark.parametrize(
    ("method", "path"),
    [
        ("get", URL),
        ("post", URL),
        ("patch", URL),
        ("delete", URL),
        ("get", HISTORY),
        ("patch", f"{URL}gifts/1/"),
        ("post", HELP),
        ("put", f"{HELP}1/"),
    ],
)
def test_requires_authentication(api_client: APIClient, method: str, path: str) -> None:
    assert getattr(api_client, method)(path).status_code == 401


@pytest.mark.parametrize("method", ["post", "patch", "delete"])
def test_changes_are_superuser_only(people: list[User], method: str) -> None:
    assert getattr(client_for(people[0]), method)(URL, {}, format="json").status_code == 403


# --- inactive ---


def test_inactive_state(people: list[User]) -> None:
    data = client_for(people[0]).get(URL).data
    assert data == {
        "active": False,
        "event": None,
        "my_victim": None,
        "my_tier_victims": [],
        "my_help_requests": [],
        "help_requests_for_me": [],
    }


# --- start ---


def test_start_draws_a_valid_cycle(boss: User, people: list[User]) -> None:
    assert start(boss, people).status_code == 201
    drawn = pairing(people)
    assert set(drawn) == set(drawn.values()) == {p.pk for p in people}  # everyone gives & receives
    assert all(giver != receiver for giver, receiver in drawn.items())
    assert all(drawn[receiver] != giver for giver, receiver in drawn.items())  # no swaps


def test_draw_is_always_valid() -> None:
    for size in range(3, 12):
        ids = list(range(size))
        for _ in range(50):
            drawn = services._draw(ids)
            assert sorted(drawn.values()) == ids
            assert all(g != r and drawn[r] != g for g, r in drawn.items())


def test_start_sends_push_with_victim_and_amounts(
    boss: User, people: list[User], pushes: list[dict[str, Any]], events: list
) -> None:
    start(boss, people)
    assert len(pushes) == len(people)
    drawn = pairing(people)
    names = {p.pk: p.username for p in people}
    for push in pushes:
        (giver,) = push["user_ids"]
        assert names[drawn[giver]] in push["body"]
        assert "100 zł, 30 zł" in push["body"]
        assert push["url"] == "/secret-santa"
    assert {uid for uid, t, _ in events if t == "santa.updated"} >= {p.pk for p in people}


def test_pairing_is_not_plain_in_the_db(boss: User, people: list[User]) -> None:
    start(boss, people)
    for row in SantaAssignment.objects.all():
        assert row.receiver_id is None
        assert row.payload
        for person in people:
            assert person.username not in row.payload
    drawn = pairing(people)
    row = SantaAssignment.objects.get(giver=people[0])
    assert services.crypto.open_seal(people[0].pk, row.payload) == drawn[people[0].pk]
    with pytest.raises(ValueError, match="does not belong"):
        services.crypto.open_seal(people[1].pk, row.payload)


def test_victim_hidden_from_others_including_superuser(boss: User, people: list[User]) -> None:
    start(boss, people[:3])
    assert client_for(boss).get(URL).data["my_victim"] is None
    outsider = client_for(people[4]).get(URL).data
    assert outsider["active"] is True
    assert outsider["my_victim"] is None
    assert outsider["event"]["is_participant"] is False
    assert len(outsider["event"]["participants"]) == 3


@pytest.mark.parametrize(
    "payload",
    [
        {"gift_tiers": []},
        {"gift_tiers": [0]},
        {"gift_tiers": [-5]},
        {"deadline": "2000-01-01T00:00:00Z"},
        {"participant_ids": [1, 2]},
        {"participant_ids": [1, 2, 99999]},
    ],
)
def test_start_validation(boss: User, people: list[User], payload: dict) -> None:
    body = {
        "participant_ids": [p.pk for p in people],
        "deadline": deadline(),
        "gift_tiers": [50],
    } | payload
    if payload.get("participant_ids") == [1, 2]:
        body["participant_ids"] = [p.pk for p in people[:2]]
    assert client_for(boss).post(URL, body, format="json").status_code == 400
    assert not SantaEvent.objects.exists()


def test_second_active_event_conflicts(boss: User, people: list[User]) -> None:
    assert start(boss, people).status_code == 201
    assert start(boss, people).status_code == 409


# --- per-tier mode ---


def tier_pairing(people: list[User]) -> dict[int, dict[int, int]]:
    """giver id -> {amount: receiver id}"""
    return {
        p.pk: {
            t["amount"]: t["victim"]["id"] for t in client_for(p).get(URL).data["my_tier_victims"]
        }
        for p in people
    }


def test_per_tier_gives_a_different_victim_per_tier(boss: User, people: list[User]) -> None:
    assert start(boss, people, tiers=(100, 50, 30), mode="per_tier").status_code == 201
    drawn = tier_pairing(people)
    for giver, victims in drawn.items():
        assert set(victims) == {100, 50, 30}
        assert len(set(victims.values())) == 3  # a different victim for every tier
        assert giver not in victims.values()
    for amount in (100, 50, 30):
        receivers = [victims[amount] for victims in drawn.values()]
        assert sorted(receivers) == sorted(p.pk for p in people)  # everyone receives each tier
        assert all(drawn[drawn[g][amount]][amount] != g for g in drawn)  # no swaps
    assert all(client_for(p).get(URL).data["my_victim"] is None for p in people)


def test_per_tier_state_for_the_event_and_outsiders(boss: User, people: list[User]) -> None:
    start(boss, people[:4], tiers=(100, 30), mode="per_tier")
    outsider = client_for(people[4]).get(URL).data
    assert outsider["event"]["mode"] == "per_tier"
    assert outsider["my_tier_victims"] == []
    assert len(outsider["event"]["participants"]) == 4  # listed once, not once per tier
    assert client_for(boss).get(URL).data["my_tier_victims"] == []


def test_single_mode_is_the_default(boss: User, people: list[User]) -> None:
    start(boss, people)
    data = client_for(people[0]).get(URL).data
    assert data["event"]["mode"] == "single"
    assert data["my_victim"] is not None
    assert data["my_tier_victims"] == []


def test_per_tier_push_lists_each_victim(
    boss: User, people: list[User], pushes: list[dict[str, Any]]
) -> None:
    start(boss, people, tiers=(100, 30), mode="per_tier")
    assert len(pushes) == len(people)
    drawn = tier_pairing(people)
    names = {p.pk: p.username for p in people}
    for push in pushes:
        (giver,) = push["user_ids"]
        assert (
            f"{names[drawn[giver][100]]} (100 zł), {names[drawn[giver][30]]} (30 zł)"
            in push["body"]
        )


def test_per_tier_needs_enough_participants(boss: User, people: list[User]) -> None:
    # Three people: two valid offsets, so at most two tiers.
    assert start(boss, people[:3], tiers=(100, 50, 30), mode="per_tier").status_code == 400
    assert not SantaEvent.objects.exists()
    assert start(boss, people[:3], tiers=(100, 50), mode="per_tier").status_code == 201


def test_unknown_mode_is_rejected(boss: User, people: list[User]) -> None:
    assert start(boss, people, mode="nope").status_code == 400


def test_draw_per_tier_is_always_valid() -> None:
    for size in range(3, 12):
        ids = list(range(size))
        for tiers in range(1, len(services._per_tier_shifts(size)) + 1):
            for _ in range(20):
                drawn = services._draw_per_tier(ids, tiers)
                for pairing in drawn:
                    assert sorted(pairing.values()) == ids
                    assert all(g != r and pairing[r] != g for g, r in pairing.items())
                for giver in ids:
                    assert len({pairing[giver] for pairing in drawn}) == tiers


def test_per_tier_update_changes_amounts_not_the_draw(boss: User, people: list[User]) -> None:
    start(boss, people, tiers=(100, 30), mode="per_tier")
    resp = client_for(boss).patch(URL, {"gift_tiers": [80, 20]}, format="json")
    assert resp.status_code == 200
    drawn = tier_pairing(people)
    assert all(set(victims) == {80, 20} for victims in drawn.values())
    assert client_for(boss).patch(URL, {"gift_tiers": [80]}, format="json").status_code == 400
    assert (
        client_for(boss).patch(URL, {"gift_tiers": [80, 20, 5]}, format="json").status_code == 400
    )


def test_per_tier_end_gives_one_gift_per_pairing(boss: User, people: list[User]) -> None:
    start(boss, people, tiers=(100, 30), mode="per_tier")
    drawn = tier_pairing(people)
    assert client_for(boss).delete(URL).status_code == 200
    (event,) = client_for(people[0]).get(HISTORY).data["results"]
    assert event["mode"] == "per_tier"
    assert len(event["pairings"]) == len(people) * 2
    for pairing in event["pairings"]:
        (gift,) = pairing["gifts"]
        assert drawn[pairing["giver"]["id"]][gift["amount"]] == pairing["receiver"]["id"]
    assert SantaGift.objects.count() == len(people) * 2


# --- update ---


def test_update_keeps_the_draw(boss: User, people: list[User], pushes: list) -> None:
    start(boss, people)
    before = pairing(people)
    pushes.clear()
    new_deadline = deadline(30)
    resp = client_for(boss).patch(
        URL, {"deadline": new_deadline, "gift_tiers": [200, 50, 20]}, format="json"
    )
    assert resp.status_code == 200
    assert resp.data["event"]["gift_tiers"] == [200, 50, 20]
    assert pairing(people) == before
    assert len(pushes) == 1
    assert "200 zł, 50 zł, 20 zł" in pushes[0]["body"]


def test_update_without_active_event_is_404(boss: User) -> None:
    assert client_for(boss).patch(URL, {"gift_tiers": [10]}, format="json").status_code == 404


# --- end + history ---


def test_end_reveals_pairings_in_history(boss: User, people: list[User], events: list) -> None:
    start(boss, people)
    drawn = pairing(people)
    assert client_for(people[0]).get(HISTORY).data["results"] == []

    resp = client_for(boss).delete(URL)
    assert resp.status_code == 200
    assert resp.data["active"] is False

    assert not SantaAssignment.objects.exclude(payload="").exists()
    (event,) = client_for(people[0]).get(HISTORY).data["results"]
    assert event["gift_tiers"] == [100, 30]
    shown = {p["giver"]["id"]: p["receiver"]["id"] for p in event["pairings"]}
    assert shown == drawn
    assert all([g["amount"] for g in p["gifts"]] == [100, 30] for p in event["pairings"])


def test_end_without_active_event_is_404(boss: User) -> None:
    assert client_for(boss).delete(URL).status_code == 404


def test_can_start_again_after_ending(boss: User, people: list[User]) -> None:
    start(boss, people)
    client_for(boss).delete(URL)
    assert start(boss, people).status_code == 201
    assert SantaEvent.objects.count() == 2


# --- notes ---


def ended_gift(boss: User, people: list[User], giver: User) -> SantaGift:
    start(boss, people)
    client_for(boss).delete(URL)
    return SantaGift.objects.filter(assignment__giver=giver).first()


def test_giver_and_superuser_can_note(boss: User, people: list[User]) -> None:
    gift = ended_gift(boss, people, people[0])
    path = f"{URL}gifts/{gift.pk}/"
    resp = client_for(people[0]).patch(path, {"note": "Książka"}, format="json")
    assert resp.status_code == 200
    assert resp.data["note"] == "Książka"
    assert client_for(boss).patch(path, {"note": "Kubek"}, format="json").status_code == 200
    gift.refresh_from_db()
    assert gift.note == "Kubek"


def test_others_cannot_note(boss: User, people: list[User]) -> None:
    gift = ended_gift(boss, people, people[0])
    resp = client_for(people[1]).patch(f"{URL}gifts/{gift.pk}/", {"note": "x"}, format="json")
    assert resp.status_code == 403


def test_note_rejected_while_active(boss: User, people: list[User]) -> None:
    start(boss, people)
    gift = SantaGift.objects.create(assignment=SantaAssignment.objects.first(), amount=5)
    resp = client_for(boss).patch(f"{URL}gifts/{gift.pk}/", {"note": "x"}, format="json")
    assert resp.status_code == 400


def test_note_length_and_missing_gift(boss: User, people: list[User]) -> None:
    gift = ended_gift(boss, people, people[0])
    path = f"{URL}gifts/{gift.pk}/"
    assert client_for(people[0]).patch(path, {"note": "x" * 301}, format="json").status_code == 400
    assert (
        client_for(boss).patch(f"{URL}gifts/99999/", {"note": "x"}, format="json").status_code
        == 404
    )


# --- help requests ---


def by_id(people: list[User]) -> dict[int, User]:
    return {p.pk: p for p in people}


def ask(giver: User, victim_id: int) -> Any:
    return client_for(giver).post(HELP, {"victim_id": victim_id}, format="json")


def answer(victim: User, request_id: int, ideas: list[str]) -> Any:
    return client_for(victim).put(f"{HELP}{request_id}/", {"ideas": ideas}, format="json")


def test_help_reaches_the_victim_anonymously(
    boss: User, people: list[User], pushes: list[dict[str, Any]], events: list
) -> None:
    start(boss, people)
    giver = people[0]
    victim = by_id(people)[pairing(people)[giver.pk]]
    pushes.clear()
    events.clear()

    resp = ask(giver, victim.pk)
    assert resp.status_code == 201
    assert resp.data["my_help_requests"] == [
        {"victim_id": victim.pk, "amount": None, "pending": True, "ideas": []}
    ]
    assert [p["user_ids"] for p in pushes] == [[victim.pk]]
    assert giver.username not in pushes[0]["body"]
    assert [uid for uid, _, _ in events] == [victim.pk]

    mine = client_for(victim).get(URL).data["help_requests_for_me"]
    assert len(mine) == 1
    assert set(mine[0]) == {"id", "amount", "pending", "ideas"}  # nothing about the giver
    assert mine[0]["pending"] is True
    # Nobody else sees it, superuser included.
    for other in [boss, *(p for p in people if p not in (giver, victim))]:
        data = client_for(other).get(URL).data
        assert data["help_requests_for_me"] == []
        assert all(r["victim_id"] != victim.pk for r in data["my_help_requests"])


def test_help_row_does_not_name_the_giver(boss: User, people: list[User]) -> None:
    start(boss, people)
    giver = people[0]
    ask(giver, pairing(people)[giver.pk])
    row = SantaHelpRequest.objects.values().get()
    assert "giver_id" not in row and "assignment_id" not in row
    assert row["receiver_id"] != giver.pk


def test_victim_answers_and_giver_is_told(
    boss: User, people: list[User], pushes: list[dict[str, Any]], events: list
) -> None:
    start(boss, people)
    giver = people[0]
    victim = by_id(people)[pairing(people)[giver.pk]]
    ask(giver, victim.pk)
    request_id = client_for(victim).get(URL).data["help_requests_for_me"][0]["id"]
    pushes.clear()
    events.clear()

    resp = answer(victim, request_id, [" Książka ", "", "Kubek"])
    assert resp.status_code == 200
    assert resp.data["help_requests_for_me"][0]["ideas"] == ["Książka", "Kubek"]
    assert [p["user_ids"] for p in pushes] == [[giver.pk]]
    assert victim.username in pushes[0]["body"]
    assert [uid for uid, _, _ in events] == [giver.pk]
    assert client_for(giver).get(URL).data["my_help_requests"] == [
        {"victim_id": victim.pk, "amount": None, "pending": False, "ideas": ["Książka", "Kubek"]}
    ]


def test_ask_again_only_after_an_answer(boss: User, people: list[User]) -> None:
    start(boss, people)
    giver = people[0]
    victim = by_id(people)[pairing(people)[giver.pk]]
    assert ask(giver, victim.pk).status_code == 201
    assert ask(giver, victim.pk).status_code == 400  # still waiting
    request_id = SantaHelpRequest.objects.get().pk
    answer(victim, request_id, ["Skarpetki"])
    resp = ask(giver, victim.pk)
    assert resp.status_code == 201
    assert resp.data["my_help_requests"][0]["pending"] is True
    assert resp.data["my_help_requests"][0]["ideas"] == ["Skarpetki"]  # earlier ideas stay
    assert SantaHelpRequest.objects.count() == 1


def test_cannot_ask_about_someone_else(boss: User, people: list[User]) -> None:
    start(boss, people)
    giver = people[0]
    drawn = pairing(people)
    not_mine = next(p for p in people if p.pk not in (giver.pk, drawn[giver.pk]))
    assert ask(giver, not_mine.pk).status_code == 400
    assert ask(boss, drawn[giver.pk]).status_code == 400  # not taking part
    assert ask(giver, 99999).status_code == 400


def test_only_the_victim_can_answer(boss: User, people: list[User]) -> None:
    start(boss, people)
    giver = people[0]
    ask(giver, pairing(people)[giver.pk])
    request_id = SantaHelpRequest.objects.get().pk
    assert answer(giver, request_id, ["x"]).status_code == 404
    assert answer(boss, request_id, ["x"]).status_code == 404
    assert answer(giver, 99999, ["x"]).status_code == 404


@pytest.mark.parametrize("ideas", [[], ["", "  "], ["x"] * 6, ["x" * 201]])
def test_answer_validation(boss: User, people: list[User], ideas: list[str]) -> None:
    start(boss, people)
    giver = people[0]
    victim = by_id(people)[pairing(people)[giver.pk]]
    ask(giver, victim.pk)
    assert answer(victim, SantaHelpRequest.objects.get().pk, ideas).status_code == 400


def test_help_needs_an_active_event(boss: User, people: list[User]) -> None:
    assert ask(people[0], people[1].pk).status_code == 404
    start(boss, people)
    giver = people[0]
    victim = by_id(people)[pairing(people)[giver.pk]]
    ask(giver, victim.pk)
    request_id = SantaHelpRequest.objects.get().pk
    client_for(boss).delete(URL)
    assert answer(victim, request_id, ["x"]).status_code == 404


def test_per_tier_help_is_per_pairing(
    boss: User, people: list[User], pushes: list[dict[str, Any]]
) -> None:
    start(boss, people, tiers=(100, 30), mode="per_tier")
    giver = people[0]
    victims = client_for(giver).get(URL).data["my_tier_victims"]
    for v in victims:
        assert ask(giver, v["victim"]["id"]).status_code == 201
    state = client_for(giver).get(URL).data
    assert sorted(r["amount"] for r in state["my_help_requests"]) == [30, 100]
    assert any("100 zł" in p["body"] for p in pushes)

    # The 100 zł victim answers; only their giver for that tier hears about it.
    top = next(v for v in victims if v["amount"] == 100)
    victim = by_id(people)[top["victim"]["id"]]
    request = next(
        r for r in client_for(victim).get(URL).data["help_requests_for_me"] if r["amount"] == 100
    )
    pushes.clear()
    answer(victim, request["id"], ["Gra"])
    assert [p["user_ids"] for p in pushes] == [[giver.pk]]
    mine = {r["amount"]: r for r in client_for(giver).get(URL).data["my_help_requests"]}
    assert mine[100]["ideas"] == ["Gra"]
    assert mine[30]["pending"] is True
