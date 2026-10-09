"""The ledger's arithmetic (`ledger.money`): pure functions, so no database."""

import random
from collections import defaultdict
from decimal import Decimal

import pytest

from ledger.money import Breakdown, Debt, allocate, format_amount, settle_up


@pytest.mark.parametrize(
    ("total", "weights"),
    [(2000, [3000, 5000]), (100, [1, 1, 1]), (1, [1, 1]), (999, [7, 11, 13]), (5, [1]), (0, [1])],
)
def test_allocate_always_sums_to_the_total(total: int, weights: list[int]) -> None:
    shares = allocate(total, weights)
    assert sum(shares) == total
    assert all(s >= 0 for s in shares)


def test_allocate_matches_the_documented_example_and_gives_ties_to_the_first() -> None:
    assert allocate(2000, [3000, 5000]) == [750, 1250]
    assert allocate(100, [1, 1, 1]) == [34, 33, 33]


def nets_after(debts: list[Debt]) -> dict[int, int]:
    net: dict[int, int] = defaultdict(int)
    for d in debts:
        net[d.debtor_id] -= d.amount
        net[d.creditor_id] += d.amount
    return {u: n for u, n in net.items() if n}


def test_settle_up_pays_the_biggest_creditor_first() -> None:
    debts = settle_up({1: 6000, 2: -3000, 3: -2000, 4: -1000})
    assert debts == [Debt(2, 1, 3000), Debt(3, 1, 2000), Debt(4, 1, 1000)]


@pytest.mark.parametrize("seed", range(25))
def test_settle_up_conserves_every_net_in_at_most_n_minus_1_transfers(seed: int) -> None:
    rng = random.Random(seed)
    people = list(range(1, rng.randint(2, 12)))
    nets = {u: rng.randint(-50_000, 50_000) for u in people[:-1]}
    nets[people[-1]] = -sum(nets.values())
    debts = settle_up(nets)
    assert nets_after(debts) == {u: n for u, n in nets.items() if n}
    assert len(debts) <= max(len(people) - 1, 0)
    assert all(d.amount > 0 and d.debtor_id != d.creditor_id for d in debts)
    assert settle_up(dict(reversed(list(nets.items())))) == debts  # order never matters


def test_settle_up_refuses_nets_that_do_not_balance() -> None:
    with pytest.raises(ValueError):
        settle_up({1: 100, 2: -99})


def test_a_breakdown_nets_paid_against_owed() -> None:
    breakdown = Breakdown(paid={1: 9000}, owed={1: 3000, 2: 3000, 3: 3000})
    assert breakdown.total == 9000
    assert breakdown.nets() == {1: 6000, 2: -3000, 3: -3000}


def test_converting_a_breakdown_converts_the_total_once_and_still_balances() -> None:
    # 100.00 EUR at 4.2537: 425.37 zł, split three ways can't be even, but it adds up
    breakdown = Breakdown(paid={1: 10000}, owed={1: 3334, 2: 3333, 3: 3333})
    base = breakdown.to_base(Decimal("4.2537"), 2)
    assert base.total == 42537
    assert sum(base.owed.values()) == 42537
    assert base.owed == {1: 14182, 2: 14178, 3: 14177}  # 14181.84 / 14177.58 / 14177.58
    assert sum(base.nets().values()) == 0


def test_converting_from_a_currency_without_minor_units() -> None:
    breakdown = Breakdown(paid={1: 1500}, owed={2: 1500})  # 1500 JPY
    assert breakdown.to_base(Decimal("0.0262"), 0).total == 3930  # 39.30 zł


def test_converting_rounds_half_up() -> None:
    assert Breakdown({1: 1}, {2: 1}).to_base(Decimal("0.5"), 2).total == 1


def test_format_amount() -> None:
    assert format_amount(1000) == "10 zł"
    assert format_amount(750) == "7,50 zł"
    assert format_amount(5, "EUR") == "0,05 EUR"
    assert format_amount(1200, "JPY") == "1200 JPY"
    assert format_amount(1, "", "piwo") == "piwo"
    assert format_amount(3, "", "piwo") == "3 x piwo"
