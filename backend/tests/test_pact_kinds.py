import pytest
from rest_framework.exceptions import ValidationError

from pacts.kinds import (
    BetKind,
    GroupBetKind,
    Party,
    PredictionKind,
    ResolutionKind,
    Terms,
    _allocate,
    get_kind,
)

A, B, C = 1, 2, 3


def party(user_id: int, side: str = "", stake: int | None = None, host: bool = False) -> Party:
    return Party(user_id, host, side, stake)


def owed(settlement) -> set[tuple[int, int, int]]:
    return {(d.debtor_id, d.creditor_id, d.amount) for d in settlement.debts}


def test_registry_has_all_four_kinds() -> None:
    assert {get_kind(k).key for k in ("bet", "group_bet", "prediction", "resolution")} == {
        "bet",
        "group_bet",
        "prediction",
        "resolution",
    }
    with pytest.raises(ValidationError):
        get_kind("nope")


@pytest.mark.parametrize(
    ("total", "weights"),
    [(2000, [3000, 5000]), (100, [1, 1, 1]), (1, [1, 1]), (999, [7, 11, 13]), (5, [1])],
)
def test_allocate_always_sums_to_the_total(total: int, weights: list[int]) -> None:
    shares = _allocate(total, weights)
    assert sum(shares) == total
    assert all(s >= 0 for s in shares)


def test_allocate_matches_the_documented_example() -> None:
    assert _allocate(2000, [3000, 5000]) == [750, 1250]


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


def test_group_bet_empty_side_draw_and_note_only_stakes_settle_no_money() -> None:
    kind = GroupBetKind()
    only_winners = [party(A, "tak", 100, host=True), party(B, "tak", 200)]
    assert kind.settle(only_winners, {"winner": "tak"}, {}).debts == []
    assert kind.settle(group_parties(), {"winner": "draw"}, {}).debts == []
    no_cash = [party(A, "tak", None, host=True), party(B, "nie", 500)]
    assert kind.settle(no_cash, {"winner": "tak"}, {}).debts == []


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
        kind.validate_terms(Terms(B), config, host=False, final=True)
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
    assert PredictionKind().join_approvers(A, [A, B, C]) == {A}


# --- prediction and resolution ----------------------------------------------


def test_prediction_scores_the_right_side_and_has_no_money() -> None:
    kind = PredictionKind()
    settlement = kind.settle([party(A, "tak", host=True), party(B, "nie")], {"winner": "nie"}, {})
    assert settlement.debts == []
    assert settlement.verdicts == {A: "lost", B: "won"}
    with pytest.raises(ValidationError):
        kind.validate_terms(
            Terms(B, 100, "", "tak"), {"sides": ["tak", "nie"]}, host=False, final=True
        )
    assert kind.needs_due_date


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
