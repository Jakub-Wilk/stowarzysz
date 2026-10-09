import pytest
from rest_framework.exceptions import ValidationError

from pacts.kinds import (
    BetKind,
    GroupBetKind,
    Party,
    ResolutionKind,
    Terms,
    get_kind,
)

A, B, C = 1, 2, 3


def party(user_id: int, side: str = "", stake: int | None = None, host: bool = False) -> Party:
    return Party(user_id, host, side, stake)


def owed(settlement) -> set[tuple[int, int, int]]:
    return {(d.debtor_id, d.creditor_id, d.amount) for d in settlement.debts}


def test_registry_has_all_four_kinds() -> None:
    assert {get_kind(k).key for k in ("bet", "group_bet", "resolution")} == {
        "bet",
        "group_bet",
        "resolution",
    }
    with pytest.raises(ValidationError):
        get_kind("nope")


# --- bet --------------------------------------------------------------------


def test_bet_pairs_the_host_with_one_opponent() -> None:
    parties = [party(A, host=True), party(B, stake=1500)]
    host_wins = BetKind().settle(parties, {"winner": "host"}, {})
    assert owed(host_wins) == {(B, A, 1500)}
    assert host_wins.verdicts == {A: "won", B: "lost"}
    opponent_wins = BetKind().settle(parties, {"winner": "opponent"}, {})
    assert owed(opponent_wins) == {(A, B, 1500)}
    draw = BetKind().settle(parties, {"winner": "draw"}, {})
    assert draw.debts == [] and draw.verdicts == {A: "draw", B: "draw"}


def test_bet_terms() -> None:
    kind = BetKind()
    assert kind.validate_terms(Terms(B, 0, ""), {}, host=True, final=True) == Terms(B)
    with pytest.raises(ValidationError):
        kind.validate_terms(Terms(B), {}, host=False, final=False)
    assert (
        kind.validate_terms(Terms(B, None, " kolacja "), {}, host=False, final=False).stake_note
        == "kolacja"
    )


# --- group bet: the A 20 / B 30 / C 50 example --------------------------------


def group_parties() -> list[Party]:
    return [party(A, "tak", 2000, host=True), party(B, "nie", 3000), party(C, "nie", 5000)]


def test_group_bet_single_winner_takes_the_whole_losing_pool() -> None:
    settlement = GroupBetKind().settle(group_parties(), {"winner": "tak"}, {})
    assert owed(settlement) == {(B, A, 3000), (C, A, 5000)}
    assert settlement.verdicts == {A: "won", B: "lost", C: "lost"}


def test_group_bet_winners_split_the_losing_stake_by_their_stakes() -> None:
    settlement = GroupBetKind().settle(group_parties(), {"winner": "nie"}, {})
    assert owed(settlement) == {(A, B, 750), (A, C, 1250)}
    assert settlement.verdicts == {A: "lost", B: "won", C: "won"}


def test_group_bet_nobody_loses_more_than_their_stake_and_payouts_sum_to_the_pot() -> None:
    parties = [
        party(1, "tak", 333, host=True),
        party(2, "tak", 777),
        party(3, "tak", 101),
        party(4, "nie", 1000),
        party(5, "nie", 1),
    ]
    settlement = GroupBetKind().settle(parties, {"winner": "tak"}, {})
    paid_by = {}
    for d in settlement.debts:
        paid_by[d.debtor_id] = paid_by.get(d.debtor_id, 0) + d.amount
    assert paid_by == {4: 1000, 5: 1}


def test_group_bet_empty_side_and_draw_settle_no_money() -> None:
    kind = GroupBetKind()
    only_winners = [party(A, "tak", 100, host=True), party(B, "tak", 200)]
    assert kind.settle(only_winners, {"winner": "tak"}, {}).debts == []
    assert kind.settle(group_parties(), {"winner": "draw"}, {}).debts == []
    no_cash = [party(A, "tak", None, host=True), party(B, "nie", 500)]
    assert kind.settle(no_cash, {"winner": "tak"}, {}).debts == []


def test_group_bet_note_only_stakes_become_item_debts_spread_over_winners() -> None:
    kind = GroupBetKind()
    parties = [
        Party(A, True, "tak", None),
        Party(B, False, "tak", None),
        Party(4, False, "nie", None, "piwo"),
        Party(5, False, "nie", None, "kolacja"),
        Party(6, False, "nie", None, "kawa"),
    ]
    debts = kind.settle(parties, {"winner": "tak"}, {}).debts
    assert [(d.debtor_id, d.creditor_id, d.amount, d.item) for d in debts] == [
        (4, A, 1, "piwo"),
        (5, B, 1, "kolacja"),
        (6, A, 1, "kawa"),
    ]


def test_group_bet_terms_and_outcome_validation() -> None:
    kind = GroupBetKind()
    config = kind.validate_config({})
    assert config == {"sides": ["tak", "nie"]}
    with pytest.raises(ValidationError):
        kind.validate_config({"sides": ["tak"]})
    with pytest.raises(ValidationError):
        kind.validate_config({"sides": ["a", "a"]})
    # the creator pre-filling an invitee may leave things open; committing may not
    assert kind.validate_terms(Terms(B), config, host=False, final=False) == Terms(B)
    with pytest.raises(ValidationError):
        kind.validate_terms(Terms(B, 100), config, host=False, final=True)  # no side
    with pytest.raises(ValidationError):
        kind.validate_terms(Terms(B, 100, "", "maybe"), config, host=False, final=True)
    assert (
        kind.validate_terms(Terms(B, 100, "", "tak"), config, host=False, final=True).side == "tak"
    )
    assert kind.validate_outcome({"winner": "nie"}, config) == {"winner": "nie"}
    with pytest.raises(ValidationError):
        kind.validate_outcome({"winner": "chyba"}, config)


def test_group_bet_joins_need_everyone_but_other_kinds_only_the_host() -> None:
    assert GroupBetKind().join_approvers(A, [A, B, C]) == {A, B, C}
    assert BetKind().join_approvers(A, [A, B, C]) == {A}
    assert ResolutionKind().joinable is False and BetKind().joinable is True


# --- prediction and resolution ----------------------------------------------


def test_group_bet_without_stakes_is_a_prediction_that_only_scores() -> None:
    kind = GroupBetKind()
    config = kind.validate_config({})
    parties = [party(A, "tak", host=True), party(B, "nie"), party(C, "nie")]
    settlement = kind.settle(parties, {"winner": "nie"}, config)
    assert settlement.debts == []
    assert settlement.verdicts == {A: "lost", B: "won", C: "won"}
    assert (
        kind.validate_terms(Terms(B, None, "", "tak"), config, host=False, final=True).side == "tak"
    )
    with pytest.raises(ValidationError):
        kind.validate_terms(Terms(B), config, host=False, final=True)  # a side is still required


def test_stakes_are_optional_per_person_in_a_group_bet() -> None:
    parties = [party(A, "tak", 1000, host=True), party(B, "nie"), party(C, "nie", 500)]
    settlement = GroupBetKind().settle(parties, {"winner": "tak"}, {})
    assert owed(settlement) == {(C, A, 500)}  # B put nothing in, so owes nothing
    assert settlement.verdicts == {A: "won", B: "lost", C: "lost"}


def test_resolution_judges_the_host_only() -> None:
    kind = ResolutionKind()
    parties = [party(A, host=True), party(B)]
    assert kind.settle(parties, {"kept": True}, {}).verdicts == {A: "won"}
    assert kind.settle(parties, {"kept": False}, {}).verdicts == {A: "lost"}
    assert kind.validate_outcome({"kept": True}, {}) == {"kept": True}
    with pytest.raises(ValidationError):
        kind.validate_outcome({"kept": "yes"}, {})
    with pytest.raises(ValidationError):
        kind.validate_terms(Terms(A, 100), {}, host=True, final=True)
