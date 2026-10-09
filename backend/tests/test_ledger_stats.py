from datetime import date, timedelta
from typing import Any

import pytest
from django.utils import timezone
from rest_framework.test import APIClient

# ruff: noqa: F401, F811
# (the ledger tests' fixtures are imported, so pytest finds them and ruff sees them "redefined")
from tests.test_ledger import (
    ENTRIES,
    _media_root,
    act,
    add_expense,
    add_income,
    alice,
    bob,
    carol,
    client_for,
    debt,
    eur,
    item,
    pact_debts,
    pay,
    pushes,
)

pytestmark = pytest.mark.usefixtures("run_on_commit")

STATS = "/api/ledger/stats/"


def stats(user: Any, **params: str) -> dict[str, Any]:
    resp = client_for(user).get(STATS, params)
    assert resp.status_code == 200, resp.json()
    return resp.json()


def by_key(body: dict[str, Any]) -> dict[str, int]:
    return {c["key"]: c["spent"] for c in body["categories"]}


def test_stats_require_auth(api_client: APIClient) -> None:
    assert api_client.get(STATS).status_code == 401


def test_an_empty_period_is_all_zeros_with_a_filled_series(alice) -> None:
    body = stats(alice, start="2026-03-01", end="2026-03-05")
    assert (body["spent"], body["count"], body["categories"], body["people"]) == (0, 0, [], [])
    assert body["unit"] == "day"
    assert [p["period"] for p in body["series"]] == [f"2026-03-0{d}" for d in range(1, 6)]
    assert {p["spent"] for p in body["series"]} == {0}
    assert stats(alice)["series"] != [] and stats(alice)["spent"] == 0  # all time, no entries


def test_a_period_includes_its_boundary_days_and_nothing_else(alice, bob) -> None:
    for day, amount in (
        (date(2026, 2, 28), 100),
        (date(2026, 3, 1), 200),
        (date(2026, 3, 31), 400),
        (date(2026, 4, 1), 800),
    ):
        add_expense(alice, [item("x", amount, alice, bob)], occurred_on=day.isoformat())
    body = stats(alice, start="2026-03-01", end="2026-03-31")
    assert (body["spent"], body["count"], body["unit"]) == (600, 2, "day")
    assert len(body["series"]) == 31
    assert {p["period"]: p["spent"] for p in body["series"] if p["spent"]} == {
        "2026-03-01": 200,
        "2026-03-31": 400,
    }


def test_categories_people_and_totals_add_up(alice, bob, carol) -> None:
    add_expense(alice, [item("Kolacja", 9000, alice, bob, carol)], category="food")
    add_expense(bob, [item("Taxi", 3000, alice, bob)], category="transport")
    body = stats(alice)
    assert (body["spent"], body["expenses"], body["income"], body["count"]) == (12000, 12000, 0, 2)
    assert by_key(body) == {"food": 9000, "transport": 3000}
    assert [c["key"] for c in body["categories"]] == ["food", "transport"]  # biggest first
    assert body["categories"][0]["emoji"] == "🍽️"
    people = {p["user"]["username"]: (p["share"], p["paid"]) for p in body["people"]}
    assert people == {"alice": (4500, 9000), "bob": (4500, 3000), "carol": (3000, 0)}
    assert sum(s for s, _ in people.values()) == sum(p for _, p in people.values()) == 12000


def test_an_income_nets_out_of_its_category_and_people(alice, bob) -> None:
    add_expense(alice, [item("Bilety", 10000, alice, bob)], category="fun")
    add_income(bob, [item("Zwrot", 4000, alice, bob)], category="fun")
    body = stats(alice)
    assert (body["spent"], body["expenses"], body["income"]) == (6000, 10000, 4000)
    assert by_key(body) == {"fun": 6000}
    people = {p["user"]["username"]: (p["share"], p["paid"]) for p in body["people"]}
    assert people == {"alice": (3000, 10000), "bob": (3000, -4000)}


def test_a_fully_refunded_expense_counts_as_nothing(alice, bob) -> None:
    add_expense(alice, [item("Bilety", 5000, alice, bob)], category="fun")
    add_income(alice, [item("Zwrot", 5000, alice, bob)], category="fun")
    body = stats(alice)
    assert body["spent"] == 0 and by_key(body) == {"fun": 0}


def test_money_debts_are_a_category_of_their_own(alice, bob, carol) -> None:
    add_expense(alice, [item("Kolacja", 6000, alice, bob)], category="food")
    pact_debts(debt(bob, alice, 1500))
    pact_debts(debt(carol, alice, 2, "piwo"))  # goods are not money
    manual = client_for(carol).post(
        ENTRIES,
        {"kind": "debt", "debtor_id": carol.pk, "creditor_id": bob.pk, "amount": 500},
        format="json",
    )
    assert manual.status_code == 201, manual.json()
    body = stats(alice)
    debts = next(c for c in body["categories"] if c["key"] == "debts")
    assert (debts["label"], debts["emoji"], debts["spent"], debts["count"]) == (
        "Długi",
        "🤝",
        2000,
        2,
    )
    assert (body["spent"], body["count"]) == (
        8000,
        4,
    )  # the goods debt counts as an entry, not money
    people = {p["user"]["username"]: (p["share"], p["paid"]) for p in body["people"]}
    assert people["alice"] == (3000, 7500)  # half the dinner, and the 15 zł she is owed
    assert people["bob"] == (
        4500,
        500,
    )  # half the dinner and 15 zł owed; paid out the 5 zł he is owed
    assert people["carol"] == (500, 0)


def test_cancelled_entries_and_payments_are_not_spending(alice, bob) -> None:
    gone = add_expense(alice, [item("x", 1000, alice, bob)]).json()
    act(bob, gone["id"], "cancel")
    pact_debts(debt(bob, alice, 700))
    paid = pay(bob, alice, 700)
    act(alice, paid.json()["id"], "confirm")
    body = stats(alice)
    assert (body["spent"], body["count"]) == (700, 1)  # only the debt, not the payback
    assert by_key(body) == {"debts": 700}


def test_a_foreign_expense_counts_in_zloty(alice, bob, eur) -> None:
    add_expense(alice, [item("Hotel", 2000, alice, bob)], currency="EUR", category="lodging")
    body = stats(alice)
    assert (body["spent"], by_key(body)) == (8500, {"lodging": 8500})  # 20 EUR at 4.25


def test_long_periods_are_charted_by_month_and_all_time_spans_the_entries(alice, bob) -> None:
    today = timezone.localdate()
    old = today.replace(day=1) - timedelta(days=200)
    add_expense(alice, [item("Stare", 100, alice, bob)], occurred_on=old.isoformat())
    add_expense(alice, [item("Nowe", 300, alice, bob)])
    body = stats(alice)
    assert body["unit"] == "month"
    assert (body["start"], body["end"]) == (old.isoformat(), today.isoformat())
    months = [p["period"] for p in body["series"]]
    assert months[0] == old.strftime("%Y-%m") and months[-1] == today.strftime("%Y-%m")
    assert len(months) == len(set(months)) >= 7  # no gaps
    assert sum(p["spent"] for p in body["series"]) == 400
    assert body["monthly_average"] == round(400 / len(months))
    year = stats(alice, start=f"{today.year}-01-01", end=f"{today.year}-12-31")
    assert len(year["series"]) == 12


def test_start_after_end_is_refused(alice) -> None:
    resp = client_for(alice).get(STATS, {"start": "2026-05-02", "end": "2026-05-01"})
    assert resp.status_code == 400
