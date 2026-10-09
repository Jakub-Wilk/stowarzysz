from typing import Any

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from ledger.models import LedgerEntry, Obligation
from pacts.models import OutcomeProposal, Pact, PactParticipant

PACTS = "/api/pacts/"

pytestmark = pytest.mark.usefixtures("run_on_commit")


def url(pact: Pact | int, suffix: str = "") -> str:
    pk = pact.pk if isinstance(pact, Pact) else pact
    return f"{PACTS}{pk}/{suffix}"


@pytest.fixture
def events(monkeypatch: pytest.MonkeyPatch) -> list[tuple[int, str, Any]]:
    sent: list[tuple[int, str, Any]] = []
    monkeypatch.setattr("core.events.notify_user", lambda uid, t, d: sent.append((uid, t, d)))
    return sent


@pytest.fixture(autouse=True)
def pushes(monkeypatch: pytest.MonkeyPatch) -> list[dict[str, Any]]:
    sent: list[dict[str, Any]] = []

    def fake_send_push(user_ids, **kwargs):
        sent.append({"user_ids": sorted(user_ids), **kwargs})

    monkeypatch.setattr("pacts.services.send_push", fake_send_push)
    monkeypatch.setattr("ledger.services.send_push", lambda *a, **kw: None)
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


def make_bet(host: User, *opponents: tuple[User, int | None], **extra: Any) -> Pact:
    payload = {
        "kind": "bet",
        "title": "Witcher 4 w 2027",
        "condition": "Wyjdzie przed 2028",
        "opponents": [{"user_id": u.pk, "stake_amount": amount} for u, amount in opponents],
        **extra,
    }
    resp = client_for(host).post(PACTS, payload, format="json")
    assert resp.status_code == 201, resp.json()
    return Pact.objects.get(pk=resp.json()["id"])


def accept(user: User, pact: Pact) -> Any:
    return client_for(user).post(url(pact, "respond/"), {"accept": True}, format="json")


def wager(pact: Pact, user: User) -> PactParticipant:
    return PactParticipant.objects.get(pact=pact, user=user)


def propose(user: User, pact: Pact, opponent: User, winner: str) -> Any:
    return client_for(user).post(
        url(pact, "outcome/"),
        {"wager_id": wager(pact, opponent).pk, "result": {"winner": winner}},
        format="json",
    )


def owed() -> set[tuple[str, str, int]]:
    return {(o.debtor.username, o.creditor.username, o.amount) for o in Obligation.objects.all()}


# --- auth -------------------------------------------------------------------


def test_every_endpoint_requires_auth(api_client: APIClient, alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    calls = [
        api_client.get(PACTS),
        api_client.post(PACTS, {"kind": "bet"}, format="json"),
        api_client.get(url(pact)),
        api_client.post(url(pact, "respond/"), {"accept": True}, format="json"),
        api_client.post(url(pact, "cancel/")),
        api_client.post(url(pact, "outcome/"), {"wager_id": 1, "result": {}}, format="json"),
        api_client.post(url(pact, "outcome/1/confirm/")),
        api_client.post(url(pact, "outcome/1/dispute/")),
        api_client.post(url(pact, "outcome/1/escalate/")),
        api_client.post(url(pact, "join/"), {}, format="json"),
        api_client.post(url(pact, "withdraw/")),
        api_client.post(url(pact, "participants/1/decide/"), {"approve": True}, format="json"),
    ]
    assert [c.status_code for c in calls] == [401] * len(calls)


# --- create -----------------------------------------------------------------


def test_create_invites_opponents_and_notifies_them(alice, bob, carol, events, pushes) -> None:
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    assert pact.status == Pact.Status.PROPOSED
    assert wager(pact, alice).role == "host" and wager(pact, alice).state == "active"
    assert wager(pact, bob).state == "invited"
    assert wager(pact, carol).stake_amount == 1500
    assert {uid for uid, t, _ in events if t == "pact.created"} == {alice.pk, bob.pk, carol.pk}
    assert pushes[0]["user_ids"] == sorted([bob.pk, carol.pk])
    assert pushes[0]["url"] == f"/pacts/{pact.pk}"


def test_create_validation(alice, bob, dave) -> None:
    client = client_for(alice)

    def create(**over: Any) -> Any:
        body = {"kind": "bet", "title": "t", "condition": "c", **over}
        return client.post(PACTS, body, format="json")

    assert create(opponents=[]).status_code == 201  # nobody invited: anyone can still join
    assert create(opponents=[{"user_id": alice.pk, "stake_amount": 100}]).status_code == 400
    assert create(opponents=[{"user_id": 9999, "stake_amount": 100}]).status_code == 400
    assert create(opponents=[{"user_id": bob.pk}]).status_code == 400  # a bet needs a stake
    twice = [{"user_id": bob.pk, "stake_amount": 100}] * 2
    assert create(opponents=twice).status_code == 400
    assert (
        create(kind="nope", opponents=[{"user_id": bob.pk, "stake_amount": 1}]).status_code == 400
    )
    assert Pact.objects.count() == 1  # only the one with nobody invited


def test_a_non_cash_stake_is_enough(alice, bob) -> None:
    resp = client_for(alice).post(
        PACTS,
        {
            "kind": "bet",
            "title": "t",
            "condition": "c",
            "opponents": [{"user_id": bob.pk, "stake_note": "kolacja"}],
        },
        format="json",
    )
    assert resp.status_code == 201
    assert wager(Pact.objects.get(), bob).stake_note == "kolacja"


def test_a_pact_can_start_with_no_invitees_and_is_broadcast(alice, bob, dave, events) -> None:
    resp = client_for(alice).post(
        PACTS,
        {"kind": "bet", "title": "t", "condition": "c"},
        format="json",
    )
    assert resp.status_code == 201
    assert any(t == "pact.created" and uid == dave.pk for uid, t, _ in events)
    assert client_for(dave).get(url(resp.json()["id"])).status_code == 200


# --- visibility -------------------------------------------------------------


def test_every_member_can_read_every_pact_but_only_participants_act(alice, bob, dave) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert client_for(dave).get(url(pact)).status_code == 200
    assert [p["id"] for p in client_for(dave).get(PACTS).json()] == [pact.pk]
    assert client_for(dave).get(url(pact)).json()["my"]["role"] is None
    assert accept(dave, pact).status_code == 403
    assert client_for(dave).post(url(pact, "cancel/")).status_code == 403
    accept(bob, pact)
    assert propose(dave, pact, bob, "host").status_code == 403


def test_list_filters(alice, bob, dave) -> None:
    mine = make_bet(alice, (bob, 1000))
    other = make_bet(bob, (alice, 500))
    assert {p["id"] for p in client_for(dave).get(PACTS).json()} == {mine.pk, other.pk}
    assert {p["id"] for p in client_for(dave).get(PACTS + "?mine=1").json()} == set()
    assert {p["id"] for p in client_for(alice).get(PACTS + "?mine=1").json()} == {
        mine.pk,
        other.pk,
    }
    assert [p["id"] for p in client_for(alice).get(PACTS + "?status=active").json()] == []


def test_detail_shows_my_state(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    my = client_for(bob).get(url(pact)).json()["my"]
    assert my["role"] == "opponent" and my["state"] == "invited"


# --- invites ----------------------------------------------------------------


def test_each_opponent_accepts_separately_and_the_pact_starts_with_the_first(alice, bob, carol):
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    assert accept(bob, pact).status_code == 200
    pact.refresh_from_db()
    assert pact.status == Pact.Status.ACTIVE
    assert wager(pact, bob).state == "active" and wager(pact, carol).state == "invited"
    assert accept(bob, pact).status_code == 409  # already answered
    assert accept(carol, pact).status_code == 200


def test_declining_one_wager_leaves_the_others(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    client_for(bob).post(url(pact, "respond/"), {"accept": False}, format="json")
    accept(carol, pact)
    pact.refresh_from_db()
    assert pact.status == Pact.Status.ACTIVE
    assert wager(pact, bob).state == "declined"


def test_pact_stays_open_for_joiners_when_every_invitee_says_no(alice, bob, carol, dave) -> None:
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    for user in (bob, carol):
        client_for(user).post(url(pact, "respond/"), {"accept": False}, format="json")
    pact.refresh_from_db()
    assert pact.status == Pact.Status.PROPOSED  # anyone can still ask to join; the host can cancel
    assert client_for(dave).get(url(pact)).json()["actions"]["can_request_join"] is True


def test_strangers_cannot_respond(alice, bob, dave) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert accept(dave, pact).status_code == 403  # can see it, isn't in it
    assert wager(pact, bob).state == "invited"


def test_creator_can_cancel_before_it_starts_only(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert client_for(bob).post(url(pact, "cancel/")).status_code == 403
    assert client_for(alice).post(url(pact, "cancel/")).status_code == 200
    pact.refresh_from_db()
    assert pact.status == Pact.Status.CANCELLED
    assert accept(bob, pact).status_code == 409

    started = make_bet(alice, (bob, 1000))
    accept(bob, started)
    assert client_for(alice).post(url(started, "cancel/")).status_code == 409


# --- outcomes and settlement -------------------------------------------------


@pytest.fixture
def started(alice, bob, carol) -> Pact:
    """Alice bets 10 zł against bob and 15 zł against carol; both accepted."""
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    accept(bob, pact)
    accept(carol, pact)
    return pact


def settle(pact: Pact, proposer: User, confirmer: User, opponent: User, winner: str) -> None:
    proposal = propose(proposer, pact, opponent, winner)
    assert proposal.status_code == 201, proposal.json()
    resp = client_for(confirmer).post(url(pact, f"outcome/{proposal.json()['id']}/confirm/"))
    assert resp.status_code == 200, resp.json()


def test_host_wins_and_is_paid_by_each_opponent(started, alice, bob, carol) -> None:
    settle(started, alice, bob, bob, "host")
    assert owed() == {("bob", "alice", 1000)}
    started.refresh_from_db()
    assert started.status == Pact.Status.ACTIVE  # carol's wager is still open
    settle(started, carol, alice, carol, "host")
    assert owed() == {("bob", "alice", 1000), ("carol", "alice", 1500)}
    started.refresh_from_db()
    assert started.status == Pact.Status.RESOLVED
    assert started.resolved_at and len(started.outcome["wagers"]) == 2


def test_host_loses_and_pays_each_opponent_their_own_stake(started, alice, bob, carol) -> None:
    settle(started, bob, alice, bob, "opponent")
    settle(started, alice, carol, carol, "opponent")
    assert owed() == {("alice", "bob", 1000), ("alice", "carol", 1500)}


def test_wagers_settle_independently_with_different_results(started, alice, bob, carol) -> None:
    settle(started, alice, bob, bob, "host")
    settle(started, alice, carol, carol, "draw")
    assert owed() == {("bob", "alice", 1000)}
    started.refresh_from_db()
    assert started.status == Pact.Status.RESOLVED


def test_a_pact_with_an_unanswered_invite_is_not_resolved(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    accept(bob, pact)
    settle(pact, alice, bob, bob, "host")
    pact.refresh_from_db()
    assert pact.status == Pact.Status.ACTIVE


def test_the_proposer_cannot_confirm_their_own_claim(started, alice, bob) -> None:
    proposal = propose(alice, started, bob, "host").json()
    resp = client_for(alice).post(url(started, f"outcome/{proposal['id']}/confirm/"))
    assert resp.status_code == 403
    assert not LedgerEntry.objects.exists()


def test_only_the_wagers_two_sides_can_propose_or_confirm(started, alice, bob, carol) -> None:
    assert propose(carol, started, bob, "host").status_code == 403
    proposal = propose(alice, started, bob, "host").json()
    resp = client_for(carol).post(url(started, f"outcome/{proposal['id']}/confirm/"))
    assert resp.status_code == 403


def test_dispute_keeps_the_wager_open_and_a_new_claim_can_follow(started, alice, bob) -> None:
    proposal = propose(alice, started, bob, "host").json()
    resp = client_for(bob).post(url(started, f"outcome/{proposal['id']}/dispute/"))
    assert resp.status_code == 200
    assert OutcomeProposal.objects.get(pk=proposal["id"]).state == "disputed"
    assert wager(started, bob).state == "active"
    assert (
        client_for(bob).post(url(started, f"outcome/{proposal['id']}/confirm/")).status_code == 409
    )
    settle(started, bob, alice, bob, "opponent")
    assert owed() == {("alice", "bob", 1000)}


def test_a_new_proposal_supersedes_the_pending_one(started, alice, bob) -> None:
    first = propose(alice, started, bob, "host").json()
    propose(bob, started, bob, "opponent")
    assert OutcomeProposal.objects.get(pk=first["id"]).state == "superseded"
    stale = client_for(bob).post(url(started, f"outcome/{first['id']}/confirm/"))
    assert stale.status_code == 409


def test_outcome_validation_and_state_checks(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert propose(alice, pact, bob, "host").status_code == 409  # not started yet
    accept(bob, pact)
    assert propose(alice, pact, bob, "nobody").status_code == 400
    resp = client_for(alice).post(
        url(pact, "outcome/"),
        {"wager_id": wager(pact, alice).pk, "result": {"winner": "host"}},
        format="json",
    )
    assert resp.status_code == 400  # the host's own row isn't a wager


def test_non_cash_wager_settles_into_an_item_debt(alice, bob) -> None:
    resp = client_for(alice).post(
        PACTS,
        {
            "kind": "bet",
            "title": "t",
            "condition": "c",
            "opponents": [{"user_id": bob.pk, "stake_note": "kolacja"}],
        },
        format="json",
    )
    pact = Pact.objects.get(pk=resp.json()["id"])
    accept(bob, pact)
    settle(pact, alice, bob, bob, "host")
    entry = LedgerEntry.objects.get()
    assert (entry.kind, entry.source_type, entry.source_id) == ("debt", "pact", pact.pk)
    assert (entry.item, entry.amount, entry.currency) == ("kolacja", 1, "")
    debt = entry.obligations.get()
    assert (debt.debtor_id, debt.creditor_id, debt.item) == (bob.pk, alice.pk, "kolacja")
    pact.refresh_from_db()
    assert pact.status == Pact.Status.RESOLVED


def test_settling_notifies_the_pact_and_pushes_the_proposer(
    started, alice, bob, carol, events, pushes
):
    proposal = propose(alice, started, bob, "host").json()
    events.clear()
    pushes.clear()
    client_for(bob).post(url(started, f"outcome/{proposal['id']}/confirm/"))
    assert {uid for uid, t, _ in events if t == "pact.updated"} == {alice.pk, bob.pk, carol.pk}
    assert pushes[-1]["user_ids"] == [alice.pk]
