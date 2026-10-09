import csv
from datetime import UTC, datetime, timedelta
from io import StringIO
from pathlib import Path
from typing import Any

import pytest
from django.core.management import call_command
from django.utils import timezone

from accounts.models import User
from ledger.models import LedgerEntry
from pacts import importing, services
from pacts.models import OutcomeProposal, Pact, PactParticipant
from tests.test_pacts import (
    PACTS,
    accept,
    alice,
    bob,
    carol,
    client_for,
    dave,
    events,
    make_bet,
    owed,
    propose,
    pushes,
    url,
    wager,
)
from voting.models import Poll

pytestmark = pytest.mark.usefixtures("run_on_commit")


@pytest.fixture
def erin(db) -> User:
    return User.objects.create(username="erin")


def create(user: User, **body: Any) -> Pact:
    resp = client_for(user).post(PACTS, body, format="json")
    assert resp.status_code == 201, resp.json()
    return Pact.objects.get(pk=resp.json()["id"])


def respond(user: User, pact: Pact, **body: Any) -> Any:
    return client_for(user).post(url(pact, "respond/"), {"accept": True, **body}, format="json")


def claim(user: User, pact: Pact, result: dict[str, Any], wager_id: int | None = None) -> Any:
    return client_for(user).post(
        url(pact, "outcome/"), {"wager_id": wager_id, "result": result}, format="json"
    )


def confirm(user: User, pact: Pact, proposal_id: int) -> Any:
    return client_for(user).post(url(pact, f"outcome/{proposal_id}/confirm/"))


def state(pact: Pact) -> str:
    pact.refresh_from_db()
    return str(pact.status)


# --- group bet -----------------------------------------------------------------


@pytest.fixture
def group(alice, bob, carol) -> Pact:
    """alice: tak 20 zł. bob: nie 30 zł, carol: nie 50 zł, both still to answer."""
    return create(
        alice,
        kind="group_bet",
        title="Witcher 4 w 2027",
        condition="Wyjdzie przed 2028",
        host={"side": "tak", "stake_amount": 2000},
        opponents=[{"user_id": bob.pk}, {"user_id": carol.pk}],
    )


def test_group_bet_starts_only_once_everyone_has_answered(group, bob, carol) -> None:
    assert respond(bob, group, side="nie", stake_amount=3000).status_code == 200
    assert state(group) == Pact.Status.PROPOSED  # carol hasn't answered yet
    assert respond(carol, group, side="nie", stake_amount=5000).status_code == 200
    assert state(group) == Pact.Status.ACTIVE


def test_a_group_bet_decline_lets_the_rest_start(group, bob, carol) -> None:
    respond(bob, group, side="nie", stake_amount=3000)
    client_for(carol).post(url(group, "respond/"), {"accept": False}, format="json")
    assert state(group) == Pact.Status.ACTIVE


def test_group_bet_acceptance_needs_a_side_and_a_stake(group, bob) -> None:
    assert respond(bob, group).status_code == 400
    assert respond(bob, group, side="nie").status_code == 400
    assert respond(bob, group, side="maybe", stake_amount=100).status_code == 400
    assert wager(group, bob).state == "invited"


def test_group_bet_needs_the_hosts_side_and_stake(alice, bob) -> None:
    resp = client_for(alice).post(
        PACTS,
        {
            "kind": "group_bet",
            "title": "t",
            "condition": "c",
            "opponents": [{"user_id": bob.pk}],
        },
        format="json",
    )
    assert resp.status_code == 400


def test_group_bet_pays_the_documented_example_after_everyone_confirms(
    group, alice, bob, carol
) -> None:
    respond(bob, group, side="nie", stake_amount=3000)
    respond(carol, group, side="nie", stake_amount=5000)
    proposal = claim(alice, group, {"winner": "nie"}).json()
    assert proposal["wager_id"] is None
    assert confirm(bob, group, proposal["id"]).status_code == 200
    assert state(group) == Pact.Status.ACTIVE  # carol still has to agree
    assert not LedgerEntry.objects.exists()
    assert confirm(bob, group, proposal["id"]).status_code == 409  # already counted
    assert confirm(carol, group, proposal["id"]).status_code == 200
    assert owed() == {("alice", "bob", 750), ("alice", "carol", 1250)}
    assert state(group) == Pact.Status.RESOLVED
    assert group.outcome == {"result": {"winner": "nie"}}


def test_group_bet_one_dispute_stops_the_whole_claim(group, alice, bob, carol) -> None:
    respond(bob, group, side="nie", stake_amount=3000)
    respond(carol, group, side="nie", stake_amount=5000)
    proposal = claim(alice, group, {"winner": "tak"}).json()
    confirm(bob, group, proposal["id"])
    assert (
        client_for(carol).post(url(group, f"outcome/{proposal['id']}/dispute/")).status_code == 200
    )
    assert state(group) == Pact.Status.ACTIVE
    assert not LedgerEntry.objects.exists()


def test_group_bet_outcome_must_name_a_side(group, alice, bob, carol) -> None:
    respond(bob, group, side="nie", stake_amount=3000)
    respond(carol, group, side="nie", stake_amount=5000)
    assert claim(alice, group, {"winner": "host"}).status_code == 400


def test_group_bet_is_decided_by_participants_only(group, alice, bob, carol, dave) -> None:
    respond(bob, group, side="nie", stake_amount=3000)
    respond(carol, group, side="nie", stake_amount=5000)
    assert claim(dave, group, {"winner": "tak"}).status_code == 403  # can read it, not act


# --- joining -------------------------------------------------------------------


def request(user: User, pact: Pact, **terms: Any) -> Any:
    return client_for(user).post(url(pact, "join/"), terms, format="json")


def decide(user: User, pact: Pact, requester: User, approve: bool) -> Any:
    pid = wager(pact, requester).pk
    return client_for(user).post(
        url(pact, f"participants/{pid}/decide/"), {"approve": approve}, format="json"
    )


def test_bet_join_request_is_approved_by_the_host_alone(alice, bob, carol, dave, pushes) -> None:
    pact = make_bet(alice, (bob, 1000), is_open=True)
    accept(bob, pact)
    pushes.clear()
    assert request(carol, pact, stake_amount=1500).status_code == 200
    assert wager(pact, carol).state == "requested"
    assert pushes[-1]["user_ids"] == [alice.pk]
    assert decide(bob, pact, carol, True).status_code == 403  # not bob's call
    assert decide(dave, pact, carol, True).status_code == 403  # a stranger, but it's open
    assert decide(alice, pact, carol, True).status_code == 200
    assert wager(pact, carol).state == "active"
    assert decide(alice, pact, carol, True).status_code == 409  # already decided/active


def test_a_joined_wager_settles_like_any_other(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), is_open=True)
    accept(bob, pact)
    request(carol, pact, stake_amount=1500)
    decide(alice, pact, carol, True)
    proposal = propose(alice, pact, carol, "opponent").json()
    confirm(carol, pact, proposal["id"])
    assert owed() == {("alice", "carol", 1500)}


def test_bet_join_needs_a_stake_and_an_open_pact(alice, bob, carol) -> None:
    closed = make_bet(alice, (bob, 1000))
    accept(bob, closed)
    assert request(carol, closed, stake_amount=100).status_code == 409  # not open
    open_pact = make_bet(alice, (bob, 1000), is_open=True)
    assert request(carol, open_pact).status_code == 400
    assert request(carol, open_pact, stake_amount=100).status_code == 200
    assert request(carol, open_pact, stake_amount=100).status_code == 409  # already asked


def test_rejected_requests_can_be_made_again(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), is_open=True)
    accept(bob, pact)
    request(carol, pact, stake_amount=100)
    decide(alice, pact, carol, False)
    assert wager(pact, carol).state == "rejected"
    assert request(carol, pact, stake_amount=200).status_code == 200
    assert wager(pact, carol).stake_amount == 200 and wager(pact, carol).state == "requested"


def test_withdrawing_a_request(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), is_open=True)
    request(carol, pact, stake_amount=100)
    assert client_for(carol).post(url(pact, "withdraw/")).status_code == 200
    assert wager(pact, carol).state == "withdrawn"
    assert client_for(carol).post(url(pact, "withdraw/")).status_code == 409
    assert decide(alice, pact, carol, True).status_code == 409


def test_an_open_pact_with_nobody_invited_starts_with_the_first_approved_joiner(alice, bob) -> None:
    pact = create(alice, kind="bet", title="t", condition="c", is_open=True)
    assert pact.status == Pact.Status.PROPOSED
    request(bob, pact, stake_amount=500)
    decide(alice, pact, bob, True)
    assert state(pact) == Pact.Status.ACTIVE


def test_group_bet_joiners_need_every_active_participant_to_approve(
    alice, bob, carol, dave
) -> None:
    pact = create(
        alice,
        kind="group_bet",
        title="t",
        condition="c",
        is_open=True,
        host={"side": "tak", "stake_amount": 1000},
        opponents=[{"user_id": bob.pk}],
    )
    respond(bob, pact, side="nie", stake_amount=1000)
    assert request(dave, pact, side="nie", stake_amount=500).status_code == 200
    assert decide(alice, pact, dave, True).status_code == 200
    assert wager(pact, dave).state == "requested"  # bob hasn't approved
    assert decide(carol, pact, dave, True).status_code == 403
    assert decide(bob, pact, dave, True).status_code == 200
    assert wager(pact, dave).state == "active"
    assert wager(pact, dave).side == "nie"


def test_one_no_is_enough_to_reject_a_group_bet_joiner(alice, bob, dave) -> None:
    pact = create(
        alice,
        kind="group_bet",
        title="t",
        condition="c",
        is_open=True,
        host={"side": "tak", "stake_amount": 1000},
        opponents=[{"user_id": bob.pk}],
    )
    respond(bob, pact, side="nie", stake_amount=1000)
    request(dave, pact, side="tak", stake_amount=500)
    decide(bob, pact, dave, False)
    assert wager(pact, dave).state == "rejected"


def test_group_bet_membership_is_frozen_while_a_claim_is_open(alice, bob, carol, dave) -> None:
    pact = create(
        alice,
        kind="group_bet",
        title="t",
        condition="c",
        is_open=True,
        host={"side": "tak", "stake_amount": 1000},
        opponents=[{"user_id": bob.pk}],
    )
    respond(bob, pact, side="nie", stake_amount=1000)
    claim(alice, pact, {"winner": "tak"})
    assert request(dave, pact, side="tak", stake_amount=100).status_code == 409


def test_detail_tells_the_caller_what_they_can_do(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), is_open=True)
    assert client_for(bob).get(url(pact)).json()["actions"]["can_respond"] is True
    accept(bob, pact)
    assert client_for(carol).get(url(pact)).json()["actions"]["can_request_join"] is True
    request(carol, pact, stake_amount=100)
    assert client_for(alice).get(url(pact)).json()["actions"]["to_decide"] == [
        wager(pact, carol).pk
    ]
    assert client_for(carol).get(url(pact)).json()["actions"]["can_withdraw_request"] is True
    proposal = propose(alice, pact, bob, "host").json()
    assert client_for(bob).get(url(pact)).json()["actions"]["to_confirm"] == [proposal["id"]]
    assert client_for(alice).get(url(pact)).json()["actions"]["to_confirm"] == []


# --- void, predictions, resolutions -----------------------------------------------


def test_a_wager_can_be_called_off_by_mutual_agreement(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    accept(bob, pact)
    accept(carol, pact)
    proposal = propose(bob, pact, bob, "x")  # not a valid claim
    assert proposal.status_code == 400
    void = (
        client_for(bob)
        .post(
            url(pact, "outcome/"),
            {"wager_id": wager(pact, bob).pk, "result": {"void": True}},
            format="json",
        )
        .json()
    )
    confirm(alice, pact, void["id"])
    assert wager(pact, bob).state == "void"
    assert not LedgerEntry.objects.exists()
    assert state(pact) == Pact.Status.ACTIVE  # carol's wager still stands


def test_calling_off_every_wager_voids_the_pact(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    accept(bob, pact)
    void = claim(alice, pact, {"void": True}, wager(pact, bob).pk).json()
    confirm(bob, pact, void["id"])
    assert state(pact) == Pact.Status.VOID


def test_prediction_scores_those_who_picked_the_winning_side(alice, bob, carol) -> None:
    pact = create(
        alice,
        kind="prediction",
        title="Polska na mundialu",
        condition="Wyjdzie z grupy",
        due_at=(timezone.now() + timedelta(days=30)).isoformat(),
        host={"side": "tak"},
        opponents=[{"user_id": bob.pk}, {"user_id": carol.pk}],
    )
    assert respond(bob, pact).status_code == 400  # must pick a side
    assert respond(bob, pact, side="nie", stake_amount=100).status_code == 400  # no money here
    respond(bob, pact, side="nie")
    respond(carol, pact, side="tak")
    proposal = claim(bob, pact, {"winner": "tak"}).json()
    confirm(alice, pact, proposal["id"])
    confirm(carol, pact, proposal["id"])
    claim_row = OutcomeProposal.objects.get(pk=proposal["id"])
    assert claim_row.verdicts == {str(alice.pk): "won", str(bob.pk): "lost", str(carol.pk): "won"}
    assert not LedgerEntry.objects.exists()
    assert state(pact) == Pact.Status.RESOLVED


def test_prediction_and_resolution_require_a_deadline(alice, bob) -> None:
    for kind in ("prediction", "resolution"):
        resp = client_for(alice).post(
            PACTS,
            {
                "kind": kind,
                "title": "t",
                "condition": "c",
                "host": {"side": "tak"} if kind == "prediction" else {},
                "opponents": [{"user_id": bob.pk}],
            },
            format="json",
        )
        assert resp.status_code == 400


def test_a_deadline_must_be_in_the_future(alice, bob) -> None:
    resp = client_for(alice).post(
        PACTS,
        {
            "kind": "bet",
            "title": "t",
            "condition": "c",
            "due_at": (timezone.now() - timedelta(days=1)).isoformat(),
            "opponents": [{"user_id": bob.pk, "stake_amount": 100}],
        },
        format="json",
    )
    assert resp.status_code == 400


def test_resolution_is_judged_by_the_others(alice, bob, carol) -> None:
    pact = create(
        alice,
        kind="resolution",
        title="Rzucę palenie",
        condition="Do końca roku bez papierosa",
        due_at=(timezone.now() + timedelta(days=90)).isoformat(),
        opponents=[{"user_id": bob.pk, "stake_note": "stawiam piwo"}, {"user_id": carol.pk}],
    )
    respond(bob, pact)
    respond(carol, pact)
    assert client_for(alice).post(url(pact), {}).status_code == 405
    proposal = claim(alice, pact, {"kept": True}).json()
    confirm(bob, pact, proposal["id"])
    confirm(carol, pact, proposal["id"])
    assert OutcomeProposal.objects.get(pk=proposal["id"]).verdicts == {str(alice.pk): "won"}
    assert state(pact) == Pact.Status.RESOLVED
    assert not LedgerEntry.objects.exists()


def test_resolutions_have_no_money_or_open_join(alice, bob) -> None:
    body = {
        "kind": "resolution",
        "title": "t",
        "condition": "c",
        "due_at": (timezone.now() + timedelta(days=9)).isoformat(),
    }
    paid = {**body, "opponents": [{"user_id": bob.pk, "stake_amount": 100}]}
    assert client_for(alice).post(PACTS, paid, format="json").status_code == 400
    opened = {**body, "is_open": True, "opponents": [{"user_id": bob.pk}]}
    assert client_for(alice).post(PACTS, opened, format="json").status_code == 400


# --- Sejmik rulings ------------------------------------------------------------------


@pytest.fixture
def disputed(alice, bob) -> tuple[Pact, OutcomeProposal]:
    pact = make_bet(alice, (bob, 1000))
    accept(bob, pact)
    proposal = propose(alice, pact, bob, "host").json()
    client_for(bob).post(url(pact, f"outcome/{proposal['id']}/dispute/"))
    return pact, OutcomeProposal.objects.get(pk=proposal["id"])


def escalate(user: User, pact: Pact, proposal: OutcomeProposal) -> Any:
    return client_for(user).post(url(pact, f"outcome/{proposal.pk}/escalate/"))


def test_escalating_creates_a_poll_for_members_outside_the_pact(
    disputed, alice, bob, carol, dave
) -> None:
    pact, proposal = disputed
    assert escalate(carol, pact, proposal).status_code == 403  # only the parties can
    assert client_for(alice).get(url(pact)).json()["actions"]["can_escalate"] == [proposal.pk]
    assert escalate(alice, pact, proposal).status_code == 200
    proposal.refresh_from_db()
    poll = proposal.ruling_poll
    assert poll.kind == "pact_ruling" and poll.title == f"Spór o zakład: {pact.title}"
    assert set(poll.participants.values_list("user__username", flat=True)) == {"carol", "dave"}
    assert escalate(alice, pact, proposal).status_code == 409  # only once


def test_escalation_needs_impartial_voters(disputed, alice) -> None:
    pact, proposal = disputed
    resp = escalate(alice, pact, proposal)
    assert resp.status_code == 400
    assert Poll.objects.count() == 0


def test_a_claim_must_be_disputed_before_it_goes_to_a_vote(alice, bob, carol) -> None:
    pact = make_bet(alice, (bob, 1000))
    accept(bob, pact)
    proposal = OutcomeProposal.objects.get(pk=propose(alice, pact, bob, "host").json()["id"])
    assert escalate(alice, pact, proposal).status_code == 409


def vote(user: User, poll: Poll, upheld: bool) -> Any:
    return client_for(user).put(
        f"/api/polls/{poll.pk}/ballot/", {"ballot": {"upheld": upheld}}, format="json"
    )


def test_upholding_a_disputed_claim_settles_it(disputed, alice, bob, carol, dave) -> None:
    pact, proposal = disputed
    escalate(alice, pact, proposal)
    proposal.refresh_from_db()
    poll = proposal.ruling_poll
    assert vote(alice, poll, True).status_code == 403  # parties don't vote
    assert vote(carol, poll, True).status_code == 200
    assert owed() == set()  # one juror still to go
    assert vote(dave, poll, False).status_code == 200  # 1-1 ... but see the next test
    # a tie overrules
    proposal.refresh_from_db()
    assert proposal.state == "overruled"


def test_a_majority_upholds_and_money_moves(disputed, alice, bob, carol, dave, erin) -> None:
    pact, proposal = disputed
    escalate(bob, pact, proposal)
    proposal.refresh_from_db()
    poll = proposal.ruling_poll
    for juror, upheld in ((carol, True), (dave, True), (erin, False)):
        vote(juror, poll, upheld)
    poll.refresh_from_db()
    assert poll.status == "closed" and poll.result["approved"] is True
    assert poll.result["applied"] is True
    assert owed() == {("bob", "alice", 1000)}
    proposal.refresh_from_db()
    assert proposal.state == "confirmed"
    assert state(pact) == Pact.Status.RESOLVED


def test_an_overruled_claim_leaves_the_wager_open_for_a_new_claim(
    disputed, alice, bob, carol, dave, erin
) -> None:
    pact, proposal = disputed
    escalate(alice, pact, proposal)
    proposal.refresh_from_db()
    poll = proposal.ruling_poll
    for juror in (carol, dave, erin):
        vote(juror, poll, False)
    proposal.refresh_from_db()
    assert proposal.state == "overruled"
    assert wager(pact, bob).state == "active"
    again = propose(bob, pact, bob, "opponent")
    assert again.status_code == 201


def test_no_new_claim_while_the_vote_is_running(disputed, alice, bob, carol) -> None:
    pact, proposal = disputed
    escalate(alice, pact, proposal)
    assert propose(alice, pact, bob, "host").status_code == 409


def test_an_expired_ruling_with_no_votes_overrules_the_claim(disputed, alice, bob, carol) -> None:
    from voting import services as voting_services

    pact, proposal = disputed
    escalate(alice, pact, proposal)
    proposal.refresh_from_db()
    voting_services.process_deadlines(now=timezone.now() + timedelta(days=4))
    proposal.refresh_from_db()
    assert proposal.state == "overruled"


# --- deadlines ---------------------------------------------------------------------------


def test_overdue_pacts_are_moved_and_nudged_weekly(alice, bob, pushes) -> None:
    pact = make_bet(alice, (bob, 1000))
    accept(bob, pact)
    Pact.objects.filter(pk=pact.pk).update(due_at=timezone.now() - timedelta(hours=1))
    pushes.clear()
    now = timezone.now()
    assert services.process_deadlines(now)["overdue"] == 1
    assert state(pact) == Pact.Status.AWAITING_RESULT
    assert pushes[-1]["user_ids"] == sorted([alice.pk, bob.pk])
    assert services.process_deadlines(now + timedelta(days=3)) == {
        "overdue": 0,
        "nudged": 0,
        "expired": 0,
        "lapsed": 0,
    }
    assert services.process_deadlines(now + timedelta(days=8))["nudged"] == 1
    # an overdue pact can still be settled
    proposal = propose(alice, pact, bob, "host").json()
    confirm(bob, pact, proposal["id"])
    assert state(pact) == Pact.Status.RESOLVED
    assert services.process_deadlines(now + timedelta(days=30))["nudged"] == 0


def test_unanswered_invites_expire_and_an_abandoned_pact_is_declined(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert services.process_deadlines()["expired"] == 0
    counts = services.process_deadlines(timezone.now() + timedelta(days=8))
    assert counts["expired"] == 1
    assert wager(pact, bob).state == "expired"
    assert state(pact) == Pact.Status.DECLINED


def test_expiring_a_straggler_starts_a_group_bet(group, bob) -> None:
    respond(bob, group, side="nie", stake_amount=3000)  # carol never answers
    services.process_deadlines(timezone.now() + timedelta(days=8))
    assert state(group) == Pact.Status.ACTIVE


def test_a_pact_that_never_started_lapses_at_its_deadline(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000), due_at=(timezone.now() + timedelta(days=1)).isoformat())
    services.process_deadlines(timezone.now() + timedelta(days=2))
    assert state(pact) == Pact.Status.CANCELLED


def test_the_cron_command_runs_every_job(db) -> None:
    out = StringIO()
    call_command("process_deadlines", stdout=out)
    assert "Closed" in out.getvalue() and "Pacts:" in out.getvalue()


def test_the_cron_command_keeps_going_when_one_job_fails(db, monkeypatch) -> None:
    from django.core.management.base import CommandError

    monkeypatch.setattr(
        "pacts.management.commands.process_pact_deadlines.process_deadlines",
        lambda: (_ for _ in ()).throw(RuntimeError("boom")),
    )
    out = StringIO()
    with pytest.raises(CommandError, match="process_pact_deadlines"):
        call_command("process_deadlines", stdout=out, stderr=StringIO())
    assert "Closed" in out.getvalue()  # the poll job still ran


# --- stats ---------------------------------------------------------------------------------


def test_stats_count_settled_claims_and_money(alice, bob, carol) -> None:
    first = make_bet(alice, (bob, 1000), (carol, 1500))
    accept(bob, first)
    accept(carol, first)
    for opponent, winner in ((bob, "host"), (carol, "opponent")):
        proposal = propose(alice, first, opponent, winner).json()
        confirm(opponent, first, proposal["id"])
    open_claim = make_bet(alice, (bob, 500))
    accept(bob, open_claim)
    propose(alice, open_claim, bob, "host")  # unconfirmed: must not count

    rows = {r["user"]["username"]: r for r in client_for(bob).get(PACTS + "stats/").json()}
    assert (rows["alice"]["won"], rows["alice"]["lost"]) == (1, 1)
    assert (rows["bob"]["won"], rows["bob"]["lost"]) == (0, 1)
    assert (rows["carol"]["won"], rows["carol"]["lost"]) == (1, 0)
    assert rows["alice"]["money_won"] == 1000 and rows["alice"]["money_lost"] == 1500
    assert rows["carol"]["money_won"] == 1500
    assert rows["alice"]["by_kind"] == {"bet": {"won": 1, "lost": 1, "draw": 0}}


def test_stats_leave_out_members_with_no_record(alice, bob, dave) -> None:
    pact = make_bet(alice, (bob, 1000))
    accept(bob, pact)
    names = [r["user"]["username"] for r in client_for(dave).get(PACTS + "stats/").json()]
    assert names == []


# --- importing the old sheet ----------------------------------------------------------------


def test_parse_due() -> None:
    assert importing.parse_due("23.01.2042") == datetime(2042, 1, 23, 23, 59, tzinfo=UTC)
    assert importing.parse_due("do 2025-06-01 włącznie") == datetime(2025, 6, 1, 23, 59, tzinfo=UTC)
    assert importing.parse_due("2026") == datetime(2026, 12, 31, 23, 59, tzinfo=UTC)
    assert importing.parse_due("kiedyś") is None
    assert importing.parse_due("31.02.2030") is None


HEADERS = [
    "Nazwa zakładu/postanowienia",
    "Warunek",
    "Czas",
    "Nagroda",
    "Dodatkowe info",
    "Rezultat",
    "Wypłacone",
]


def write_csv(path: Path, rows: list[list[str]]) -> Path:
    with path.open("w", encoding="utf-8-sig", newline="") as handle:
        csv.writer(handle).writerows([HEADERS, *rows])
    return path


def test_import_command_creates_pacts_bets_and_ledger_entries(tmp_path, alice, bob, carol) -> None:
    sheet = write_csv(
        tmp_path / "sheet.csv",
        [
            ["Zakład trumpowski", "Trump wygra", "2016", "50 zł", "", "✅", "✅"],
            [
                "Rozwód przed 40",
                "Roman się rozwiedzie",
                "23.01.2042",
                "10000 zł",
                "info",
                "❓",
                "➖",
            ],
            ["Psztrycze Podrywy", "5 randek", "2025", "", "", "❌", ""],
            ["Anulowany", "x", "", "", "", "➖", ""],
            ["", "", "", "", "", "", ""],
        ],
    )
    mapping = tmp_path / "map.json"
    mapping.write_text(
        '{"Zakład trumpowski": {"host": "alice", "opponents": '
        '[{"username": "bob", "stake_pln": 50}]},'
        ' "Psztrycze Podrywy": {"host": "alice", "opponents": '
        '[{"username": "carol", "stake_pln": 20.5}]}}'
    )
    out = StringIO()
    call_command("import_pacts", str(sheet), host="alice", map=mapping, stdout=out)
    assert "Imported 4 pact(s), skipped 1" in out.getvalue()

    trump = Pact.objects.get(title="Zakład trumpowski")
    assert (trump.kind, trump.status) == ("bet", Pact.Status.RESOLVED)
    assert trump.due_at.year == 2016
    entry = LedgerEntry.objects.get(source_id=trump.pk)
    assert (entry.debtor, entry.creditor, entry.amount) == (bob, alice, 5000)
    assert entry.settled_at is not None  # Wypłacone ✅

    podrywy = Pact.objects.get(title="Psztrycze Podrywy")
    unpaid = LedgerEntry.objects.get(source_id=podrywy.pk)
    assert (unpaid.debtor, unpaid.creditor, unpaid.amount) == (alice, carol, 2050)
    assert unpaid.settled_at is None  # host lost, nothing marked paid

    divorce = Pact.objects.get(title="Rozwód przed 40")
    assert (divorce.kind, divorce.status) == ("resolution", Pact.Status.ACTIVE)
    assert "Nagroda: 10000 zł" in divorce.notes
    assert Pact.objects.get(title="Anulowany").status == Pact.Status.VOID

    # imported results feed the stats
    rows = {r["user"]["username"]: r for r in client_for(alice).get(PACTS + "stats/").json()}
    assert (rows["alice"]["won"], rows["alice"]["lost"]) == (1, 1)

    # re-running adds nothing
    again = StringIO()
    call_command("import_pacts", str(sheet), host="alice", map=mapping, stdout=again)
    assert "Imported 0 pact(s), skipped 5" in again.getvalue()


def test_import_dry_run_changes_nothing(tmp_path, alice) -> None:
    sheet = write_csv(tmp_path / "s.csv", [["Coś", "c", "2030", "", "", "✅", ""]])
    out = StringIO()
    call_command("import_pacts", str(sheet), host="alice", dry_run=True, stdout=out)
    assert "dry run" in out.getvalue()
    assert Pact.objects.count() == 0


def test_import_reports_unknown_users(tmp_path, alice) -> None:
    from django.core.management.base import CommandError

    sheet = write_csv(tmp_path / "s.csv", [["Coś", "c", "2030", "", "", "", ""]])
    with pytest.raises(CommandError, match="nobody"):
        call_command("import_pacts", str(sheet), host="nobody")
    assert PactParticipant.objects.count() == 0
