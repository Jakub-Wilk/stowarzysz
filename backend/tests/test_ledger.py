from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path
from typing import Any

import pytest
from django.utils import timezone
from rest_framework.test import APIClient

from accounts.models import User
from ledger import rates, services
from ledger.models import EntryAttachment, LedgerEntry, Obligation
from ledger.money import Debt
from tests.test_avatars import make_image

pytestmark = pytest.mark.usefixtures("run_on_commit")

ENTRIES = "/api/ledger/entries/"


@pytest.fixture(autouse=True)
def pushes(monkeypatch: pytest.MonkeyPatch) -> list[dict[str, Any]]:
    sent: list[dict[str, Any]] = []
    monkeypatch.setattr(
        "ledger.services.send_push", lambda ids, **kw: sent.append({"ids": sorted(ids), **kw})
    )
    return sent


@pytest.fixture(autouse=True)
def _media_root(settings, tmp_path: Path) -> None:
    settings.MEDIA_ROOT = tmp_path / "media"


@pytest.fixture
def eur(monkeypatch: pytest.MonkeyPatch) -> None:
    """1 EUR = 4.25 zł on any day (Frankfurter is never called for real)."""
    monkeypatch.setattr(rates, "_fetch", lambda currency, day: (Decimal("4.25"), day))


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


def item(name: str, amount: int, *users: User, split: str = "equal", weights=()) -> dict:
    shares = [
        {"user_id": u.pk, **({"weight": w} if weights else {})}
        for u, w in zip(users, weights or [1] * len(users), strict=True)
    ]
    return {"name": name, "amount": amount, "split": split, "shares": shares}


def add_expense(
    by: User,
    items: list[dict],
    payers: list[tuple[User, int]] | None = None,
    **extra: Any,
) -> Any:
    total = sum(i["amount"] for i in items)
    body = {
        "kind": "expense",
        "title": "Pizza",
        "items": items,
        "payers": [{"user_id": u.pk, "amount": a} for u, a in (payers or [(by, total)])],
        **extra,
    }
    return client_for(by).post(ENTRIES, body, format="json")


def pay(by: User, to: User, amount: int, **extra: Any) -> Any:
    body = {"kind": "payment", "to_user_id": to.pk, "amount": amount, **extra}
    return client_for(by).post(ENTRIES, body, format="json")


def act(user: User, entry_id: int, action: str) -> Any:
    return client_for(user).post(f"{ENTRIES}{entry_id}/{action}/")


def bilans(user: User) -> dict[str, Any]:
    resp = client_for(user).get("/api/ledger/balances/")
    assert resp.status_code == 200
    return resp.json()


def nets(body: dict[str, Any], item: str = "") -> dict[str, int]:
    return {m["user"]["username"]: m["net"] for m in body["members"] if m["item"] == item}


def transfers(body: dict[str, Any]) -> list[tuple[str, str, str, int]]:
    return [
        (s["debtor"]["username"], s["creditor"]["username"], s["item"], s["amount"])
        for s in body["settlements"]
    ]


def debt(debtor: User, creditor: User, amount: int, item: str = "") -> Debt:
    return Debt(debtor.pk, creditor.pk, amount, item)


def pact_debts(*debts: Debt, pact_id: int = 1) -> LedgerEntry:
    entry = services.record_debts(
        list(debts), title="Zakład", source_type="pact", source_id=pact_id
    )
    assert entry is not None
    return entry


# --- auth ---------------------------------------------------------------------------------


def test_every_endpoint_requires_auth(api_client: APIClient, alice: User, bob: User) -> None:
    entry = pact_debts(debt(bob, alice, 100))
    one = f"{ENTRIES}{entry.pk}/"
    calls = [
        api_client.get(ENTRIES),
        api_client.post(ENTRIES, {"kind": "payment"}, format="json"),
        api_client.get(one),
        api_client.put(one, {}, format="json"),
        api_client.post(f"{one}confirm/"),
        api_client.post(f"{one}reject/"),
        api_client.post(f"{one}cancel/"),
        api_client.post(f"{one}attachments/"),
        api_client.delete(f"{one}attachments/1/"),
        api_client.get("/api/ledger/balances/"),
        api_client.get("/api/ledger/meta/"),
    ]
    assert [c.status_code for c in calls] == [401] * len(calls)


def test_meta_lists_currencies_with_their_digits_and_polish_categories(alice: User) -> None:
    body = client_for(alice).get("/api/ledger/meta/").json()
    assert body["base_currency"] == "PLN"
    assert [c["code"] for c in body["currencies"][:2]] == ["PLN", "EUR"]
    assert {"code": "JPY", "exponent": 0} in body["currencies"]
    assert {"key": "food", "label": "Jedzenie"} in body["categories"]


# --- expenses -----------------------------------------------------------------------------


def test_a_plain_expense_shares_the_cost_and_explains_itself(alice, bob, carol) -> None:
    resp = add_expense(alice, [item("Pizza", 9000, alice, bob, carol)], category="food")
    assert resp.status_code == 201, resp.json()
    body = resp.json()
    assert (body["kind"], body["status"], body["amount"], body["base_amount"]) == (
        "expense",
        "confirmed",
        9000,
        9000,
    )
    assert body["category"] == "food" and body["occurred_on"] == str(timezone.localdate())
    rows = {r["user"]["username"]: (r["paid"], r["owed"]) for r in body["breakdown"]}
    assert rows == {"alice": (9000, 3000), "bob": (0, 3000), "carol": (0, 3000)}
    assert nets(bilans(carol)) == {"alice": 6000, "bob": -3000, "carol": -3000}
    assert transfers(bilans(carol)) == [("bob", "alice", "", 3000), ("carol", "alice", "", 3000)]


def test_an_itemized_receipt_charges_each_item_to_its_own_people(alice, bob, carol) -> None:
    resp = add_expense(
        alice,
        [
            item("Piwo", 1200, bob),  # only bob drank
            item("Frytki", 1000, alice, bob, carol, split="shares", weights=(2, 1, 1)),
            item("Napiwek", 301, alice, bob, carol),  # an odd grosz
            item("Wino", 3000, alice, carol, split="exact", weights=(1000, 2000)),
        ],
    )
    assert resp.status_code == 201, resp.json()
    owed = {r["user"]["username"]: r["owed"] for r in resp.json()["breakdown"]}
    # Napiwek: 301 = 100 + 100 + 101; the odd grosz takes turns, and on the third item it is the
    # third person's (carol's) turn
    assert owed == {"alice": 500 + 100 + 1000, "bob": 1200 + 250 + 100, "carol": 250 + 101 + 2000}
    assert sum(owed.values()) == resp.json()["amount"] == 5501
    assert nets(bilans(alice)) == {"alice": 3901, "carol": -2351, "bob": -1550}


def test_several_people_can_pay_one_expense(alice, bob, carol) -> None:
    resp = add_expense(
        alice,
        [item("Nocleg", 30000, alice, bob, carol)],
        payers=[(alice, 20000), (bob, 10000)],
        category="lodging",
    )
    assert resp.status_code == 201, resp.json()
    assert nets(bilans(alice)) == {"alice": 10000, "carol": -10000}  # bob paid exactly his part
    assert transfers(bilans(alice)) == [("carol", "alice", "", 10000)]


def test_an_expense_in_another_currency_is_converted_once_and_adds_up(
    alice, bob, carol, eur
) -> None:
    day = timezone.localdate() - timedelta(days=3)
    resp = add_expense(
        alice,
        [item("Muzeum", 10000, alice, bob, carol)],
        currency="EUR",
        occurred_on=day.isoformat(),
    )
    assert resp.status_code == 201, resp.json()
    body = resp.json()
    assert (body["currency"], body["amount"], body["base_amount"]) == ("EUR", 10000, 42500)
    assert (Decimal(body["rate"]), body["rate_date"]) == (Decimal("4.25"), day.isoformat())
    owed = {r["user"]["username"]: (r["owed"], r["owed_base"]) for r in body["breakdown"]}
    # 33.34 + 33.33 + 33.33 EUR -> 141.695 + 141.6525 + 141.6525 zł, rounded to sum to 425 zł
    assert owed == {"alice": (3334, 14170), "bob": (3333, 14165), "carol": (3333, 14165)}
    # the Bilans agrees with the breakdown to the grosz
    assert nets(bilans(alice)) == {"alice": 42500 - 14170, "bob": -14165, "carol": -14165}


def test_a_currency_with_no_minor_units(alice, bob, monkeypatch) -> None:
    monkeypatch.setattr(rates, "_fetch", lambda currency, day: (Decimal("0.0262"), day))
    resp = add_expense(alice, [item("Ramen", 3000, bob)], currency="JPY")  # 3000 yen
    assert resp.status_code == 201, resp.json()
    assert resp.json()["base_amount"] == 7860  # 78.60 zł
    assert nets(bilans(alice)) == {"alice": 7860, "bob": -7860}


def _stranger(body: dict) -> None:
    ghost = User.objects.create(username="ghost", is_active=False)
    body["items"][0]["shares"].append({"user_id": ghost.pk})


@pytest.mark.parametrize(
    "spoil",
    [
        lambda b: b.update(payers=[]),
        lambda b: b.update(items=[]),
        lambda b: b.update(title="   "),
        lambda b: b.update(currency="XXX"),
        lambda b: b.update(occurred_on=(date.today() + timedelta(days=2)).isoformat()),
        lambda b: b["payers"][0].update(amount=999),  # the payers don't cover the total
        lambda b: b["items"][0].update(split="exact"),  # weights 1 + 1 aren't 10 zł
        lambda b: b["items"][0]["shares"].append(dict(b["items"][0]["shares"][0])),  # twice
        lambda b: b["payers"].append(dict(b["payers"][0])),  # the same payer twice
        _stranger,
    ],
)
def test_expense_validation(alice, bob, spoil) -> None:
    body = {
        "kind": "expense",
        "title": "Pizza",
        "items": [item("Pizza", 1000, alice, bob)],
        "payers": [{"user_id": alice.pk, "amount": 1000}],
    }
    spoil(body)
    resp = client_for(alice).post(ENTRIES, body, format="json")
    assert resp.status_code == 400, resp.json()
    assert not LedgerEntry.objects.exists()


def test_anyone_can_edit_an_expense_which_replaces_what_it_means(alice, bob, carol) -> None:
    entry = add_expense(alice, [item("Pizza", 9000, alice, bob, carol)]).json()
    body = {
        "title": "Pizza bez Carol",
        "items": [item("Pizza", 6000, alice, bob)],
        "payers": [{"user_id": alice.pk, "amount": 6000}],
        "version": entry["version"],
    }
    resp = client_for(carol).put(f"{ENTRIES}{entry['id']}/", body, format="json")
    assert resp.status_code == 200, resp.json()
    assert (resp.json()["title"], resp.json()["version"]) == ("Pizza bez Carol", 2)
    assert nets(bilans(alice)) == {"alice": 3000, "bob": -3000}
    assert Obligation.objects.count() == 1


def test_an_outdated_edit_is_refused_instead_of_overwriting(alice, bob) -> None:
    entry = add_expense(alice, [item("Pizza", 1000, alice, bob)]).json()
    body = {
        "title": "Pizza",
        "items": [item("Pizza", 2000, alice, bob)],
        "payers": [{"user_id": alice.pk, "amount": 2000}],
        "version": entry["version"],
    }
    url = f"{ENTRIES}{entry['id']}/"
    assert client_for(bob).put(url, body, format="json").status_code == 200
    assert client_for(alice).put(url, body, format="json").status_code == 409  # stale version


def test_only_expenses_can_be_edited(alice, bob) -> None:
    entry = pact_debts(debt(bob, alice, 100))
    resp = client_for(alice).put(f"{ENTRIES}{entry.pk}/", {"version": 1}, format="json")
    assert resp.status_code == 409


def test_deleting_an_expense_cancels_it_and_it_stops_counting(alice, bob) -> None:
    entry = add_expense(alice, [item("Pizza", 1000, alice, bob)]).json()
    resp = act(bob, entry["id"], "cancel")
    assert resp.status_code == 200 and resp.json()["status"] == "cancelled"
    assert bilans(alice)["members"] == []
    assert act(bob, entry["id"], "cancel").status_code == 409
    body = {"title": "x", "items": [], "payers": [], "version": resp.json()["version"]}
    assert client_for(alice).put(f"{ENTRIES}{entry['id']}/", body, format="json").status_code in (
        400,
        409,
    )
    assert LedgerEntry.objects.filter(pk=entry["id"]).exists()  # never deleted


# --- debts --------------------------------------------------------------------------------


def test_a_pact_settlement_is_one_debt_entry_that_counts_at_once(alice, bob, carol) -> None:
    entry = pact_debts(debt(bob, alice, 1000), debt(carol, alice, 500))
    assert (entry.kind, entry.status, entry.amount, entry.base_amount) == (
        "debt",
        "confirmed",
        1500,
        1500,
    )
    assert nets(bilans(alice)) == {"alice": 1500, "bob": -1000, "carol": -500}


def test_a_settlement_mixing_money_and_goods_has_no_headline_amount(alice, bob, carol) -> None:
    entry = pact_debts(debt(bob, alice, 1000), debt(carol, alice, 1, "kolacja"))
    assert (entry.amount, entry.currency, entry.item) == (None, "", "")
    assert nets(bilans(alice), "kolacja") == {"alice": 1, "carol": -1}


def test_nothing_owed_records_nothing(db) -> None:
    assert services.record_debts([], title="x", source_type="pact", source_id=1) is None
    assert not LedgerEntry.objects.exists()


def test_recorded_debts_are_validated(alice, bob) -> None:
    from rest_framework.exceptions import ValidationError

    with pytest.raises(ValidationError):
        pact_debts(debt(bob, alice, 0))
    with pytest.raises(ValidationError):
        pact_debts(debt(alice, alice, 100))


def test_a_pact_debt_can_only_change_through_the_pact(alice, bob) -> None:
    entry = pact_debts(debt(bob, alice, 1000))
    assert act(alice, entry.pk, "confirm").status_code == 409
    assert act(alice, entry.pk, "cancel").status_code == 409
    assert client_for(alice).get(f"{ENTRIES}{entry.pk}/").json()["actions"] == {
        "confirm": False,
        "reject": False,
        "cancel": False,
        "edit": False,
        "attach": True,
    }


def test_a_member_records_goods_owed_to_them_and_can_drop_them(alice, bob) -> None:
    body = {"kind": "debt", "debtor_id": bob.pk, "item": " kawa ", "amount": 2}
    resp = client_for(alice).post(ENTRIES, body, format="json")
    assert resp.status_code == 201, resp.json()
    entry = resp.json()
    assert (entry["item"], entry["amount"], entry["source_type"]) == ("kawa", 2, "manual")
    assert nets(bilans(bob), "kawa") == {"alice": 2, "bob": -2}
    assert act(bob, entry["id"], "cancel").status_code == 403  # only the person owed
    assert act(alice, entry["id"], "cancel").status_code == 200
    assert bilans(bob)["members"] == []


@pytest.mark.parametrize(
    "bad",
    [
        {"item": "kawa"},  # owed to yourself
        {"debtor_id": "bob", "item": "   "},
        {"debtor_id": "bob", "item": "kawa", "amount": 0},
        {"debtor_id": 9999, "item": "kawa"},
    ],
)
def test_goods_debt_validation(alice, bob, bad: dict) -> None:
    body = {"kind": "debt", "debtor_id": alice.pk, **bad}
    if body["debtor_id"] == "bob":
        body["debtor_id"] = bob.pk
    assert client_for(alice).post(ENTRIES, body, format="json").status_code == 400


def test_an_unknown_kind_is_refused(alice) -> None:
    assert client_for(alice).post(ENTRIES, {"kind": "loan"}, format="json").status_code == 400


# --- payments -----------------------------------------------------------------------------


def test_a_payment_only_counts_once_the_receiver_confirms(alice, bob) -> None:
    pact_debts(debt(bob, alice, 1500))
    resp = pay(bob, alice, 1000, note="BLIK")
    assert resp.status_code == 201, resp.json()
    entry = resp.json()
    assert (entry["kind"], entry["status"], entry["title"], entry["note"]) == (
        "payment",
        "pending",
        "Spłata",
        "BLIK",
    )
    assert entry["details"] == {"from": bob.pk, "to": alice.pk}
    assert nets(bilans(alice)) == {"alice": 1500, "bob": -1500}  # not yet

    confirmed = act(alice, entry["id"], "confirm")
    assert confirmed.status_code == 200 and confirmed.json()["status"] == "confirmed"
    assert confirmed.json()["decided_at"]
    assert nets(bilans(alice)) == {"alice": 500, "bob": -500}
    assert act(alice, entry["id"], "confirm").status_code == 409  # only once


def test_paying_everything_clears_the_balance_and_the_entries_stay(alice, bob) -> None:
    pact_debts(debt(bob, alice, 1500))
    act(alice, pay(bob, alice, 1500).json()["id"], "confirm")
    body = bilans(alice)
    assert body["members"] == [] and body["settlements"] == []
    rows = client_for(alice).get(ENTRIES).json()["results"]
    assert [(r["kind"], r["status"]) for r in rows] == [
        ("payment", "confirmed"),
        ("debt", "confirmed"),
    ]


def test_an_overpayment_turns_the_balance_around(alice, bob) -> None:
    pact_debts(debt(bob, alice, 1000))
    act(alice, pay(bob, alice, 1500).json()["id"], "confirm")
    assert transfers(bilans(alice)) == [("alice", "bob", "", 500)]


def test_the_receiver_can_reject_and_the_payer_can_cancel(alice, bob) -> None:
    pact_debts(debt(bob, alice, 1000))
    first = pay(bob, alice, 400).json()
    assert act(alice, first["id"], "reject").json()["status"] == "rejected"
    second = pay(bob, alice, 400).json()
    assert act(bob, second["id"], "cancel").json()["status"] == "cancelled"
    assert nets(bilans(alice)) == {"alice": 1000, "bob": -1000}  # neither counted
    assert act(alice, second["id"], "confirm").status_code == 409  # already decided


def test_only_the_right_party_can_decide_a_payment(alice, bob, carol) -> None:
    pact_debts(debt(bob, alice, 1000))
    entry = pay(bob, alice, 400).json()
    assert entry["actions"] == {
        "confirm": False,
        "reject": False,
        "cancel": True,
        "edit": False,
        "attach": True,
    }
    assert client_for(alice).get(f"{ENTRIES}{entry['id']}/").json()["actions"]["confirm"] is True
    assert act(bob, entry["id"], "confirm").status_code == 403  # not your own
    assert act(bob, entry["id"], "reject").status_code == 403
    assert act(alice, entry["id"], "cancel").status_code == 403  # only the payer cancels
    assert act(carol, entry["id"], "confirm").status_code == 403
    assert act(carol, 9999, "confirm").status_code == 404


def test_payment_validation(alice, bob) -> None:
    assert pay(bob, bob, 100).status_code == 400  # yourself
    assert pay(bob, alice, 0).status_code == 400
    body = {"kind": "payment", "to_user_id": 9999, "amount": 5}
    assert client_for(bob).post(ENTRIES, body, format="json").status_code == 400
    User.objects.filter(pk=alice.pk).update(is_active=False)
    assert pay(bob, alice, 100).status_code == 400  # inactive
    assert not LedgerEntry.objects.exists()


def test_goods_are_returned_the_same_way(alice, bob) -> None:
    pact_debts(debt(bob, alice, 2, "piwo"))
    resp = pay(bob, alice, 2, item="piwo")
    assert resp.status_code == 201
    assert (resp.json()["title"], resp.json()["currency"]) == ("Zwrot", "")
    act(alice, resp.json()["id"], "confirm")
    assert bilans(alice)["members"] == []


# --- the Bilans ---------------------------------------------------------------------------


def test_balances_combine_expenses_pact_debts_and_payments(alice, bob, carol) -> None:
    add_expense(alice, [item("Zakupy", 9000, alice, bob, carol)])  # alice +6000
    pact_debts(debt(alice, carol, 2000))  # alice lost a bet to carol
    act(bob, pay(bob, alice, 1000).json()["id"], "cancel")  # never mind
    act(alice, pay(bob, alice, 1000).json()["id"], "confirm")  # bob paid 10 zł
    assert nets(bilans(alice)) == {"alice": 3000, "bob": -2000, "carol": -1000}


def test_following_every_suggestion_leaves_everyone_even(alice, bob, carol) -> None:
    dave = User.objects.create(username="dave")
    add_expense(alice, [item("Nocleg", 40000, alice, bob, carol, dave)])
    add_expense(bob, [item("Paliwo", 12000, alice, bob, carol)], category="transport")
    add_expense(carol, [item("Kolacja", 3001, bob, dave)])
    pact_debts(debt(dave, bob, 777))
    users = {u.username: u for u in (alice, bob, carol, dave)}
    for debtor, creditor, _item, amount in transfers(bilans(alice)):
        entry = pay(users[debtor], users[creditor], amount).json()
        act(users[creditor], entry["id"], "confirm")
    body = bilans(alice)
    assert body["members"] == [] and body["settlements"] == []


def test_suggestions_assume_pending_payments_go_through(alice, bob) -> None:
    add_expense(alice, [item("Pizza", 2000, bob)])
    entry = pay(bob, alice, 1500).json()
    body = bilans(alice)
    assert nets(body) == {"alice": 2000, "bob": -2000}  # confirmed only
    assert transfers(body) == [("bob", "alice", "", 500)]  # what is left to pay
    assert [(p["entry_id"], p["debtor"]["username"], p["amount"]) for p in body["pending"]] == [
        (entry["id"], "bob", 1500)
    ]


def test_goods_net_per_pair_and_per_item_never_against_money(alice, bob, carol) -> None:
    pact_debts(debt(bob, alice, 1000))
    pact_debts(debt(bob, alice, 2, "Piwo"))
    pact_debts(debt(alice, bob, 1, "  piwo "))
    pact_debts(debt(carol, bob, 1, "piwo"))
    pact_debts(debt(bob, alice, 1, "kolacja"))
    body = bilans(alice)
    assert sorted(transfers(body), key=lambda t: (t[2].lower(), t[0])) == [
        ("bob", "alice", "", 1000),
        ("bob", "alice", "kolacja", 1),
        ("bob", "alice", "Piwo", 1),
        ("carol", "bob", "Piwo", 1),  # goods are never passed on to somebody else
    ]


# --- the feed -----------------------------------------------------------------------------


def test_everyone_reads_the_feed_by_date_newest_first(alice, bob, carol) -> None:
    old = add_expense(
        alice, [item("Stare", 100, bob)], occurred_on=(date.today() - timedelta(days=9)).isoformat()
    ).json()
    first = pact_debts(debt(bob, alice, 100))
    second = add_expense(alice, [item("Nowe", 100, bob)]).json()
    rows = client_for(carol).get(ENTRIES).json()["results"]
    assert [r["id"] for r in rows] == [second["id"], first.pk, old["id"]]


def test_the_feed_is_paged_and_filterable(alice, bob) -> None:
    for _ in range(31):
        pact_debts(debt(bob, alice, 100))
    add_expense(alice, [item("Pizza", 100, bob)])
    page = client_for(alice).get(ENTRIES).json()
    assert len(page["results"]) == 30 and page["next"]
    rest = client_for(alice).get(page["next"]).json()
    assert len(rest["results"]) == 2 and rest["next"] is None
    expenses = client_for(alice).get(ENTRIES, {"kind": "expense"}).json()["results"]
    assert [r["title"] for r in expenses] == ["Pizza"]


def test_a_pacts_entries_come_unpaged(alice, bob) -> None:
    entry = pact_debts(debt(bob, alice, 100), pact_id=7)
    pact_debts(debt(bob, alice, 100), pact_id=8)
    rows = client_for(alice).get(ENTRIES, {"source_type": "pact", "source_id": 7}).json()
    assert [r["id"] for r in rows] == [entry.pk]
    assert rows[0]["obligations"][0]["debtor"]["username"] == "bob"


# --- notifications ------------------------------------------------------------------------


def test_people_in_an_expense_are_pushed_and_everyone_refreshes(
    alice, bob, carol, pushes, monkeypatch
) -> None:
    broadcasts: list[str] = []
    monkeypatch.setattr("core.events.broadcast", lambda event, data: broadcasts.append(event))
    entry = add_expense(alice, [item("Pizza", 3000, alice, bob)]).json()
    assert pushes == [
        {
            "ids": [bob.pk],
            "title": "Nowy wydatek",
            "body": "alice: Pizza (30 zł)",
            "url": f"/ledger/{entry['id']}",
        }
    ]
    act(bob, entry["id"], "cancel")
    assert pushes[-1]["ids"] == [alice.pk] and pushes[-1]["title"] == "Wydatek usunięty"
    assert broadcasts == ["ledger.updated"] * 2


def test_payments_notify_the_other_side(alice, bob, pushes) -> None:
    pact_debts(debt(bob, alice, 1000))
    assert pushes[-1]["ids"] == [alice.pk] and pushes[-1]["title"] == "Nowe rozliczenie"
    entry = pay(bob, alice, 400).json()
    assert pushes[-1]["ids"] == [alice.pk] and pushes[-1]["title"] == "Wpłata do potwierdzenia"
    act(alice, entry["id"], "confirm")
    assert pushes[-1]["ids"] == [bob.pk] and pushes[-1]["title"] == "Wpłata potwierdzona"
    assert pushes[-1]["body"] == "alice potwierdził(a) wpłatę 4 zł"


# --- attachments --------------------------------------------------------------------------


def attach(user: User, entry_id: int) -> Any:
    return client_for(user).post(
        f"{ENTRIES}{entry_id}/attachments/", {"image": make_image()}, format="multipart"
    )


def test_a_receipt_photo_on_an_expense(alice, bob, carol) -> None:
    entry = add_expense(alice, [item("Pizza", 1000, alice, bob)]).json()
    resp = attach(carol, entry["id"])  # anyone, like editing
    assert resp.status_code == 201, resp.json()
    photo = resp.json()["attachments"][0]
    assert photo["url"].endswith(".webp") and photo["uploaded_by"]["username"] == "carol"
    url = f"{ENTRIES}{entry['id']}/attachments/{photo['id']}/"
    assert client_for(bob).delete(url).status_code == 403  # neither uploader nor author
    assert client_for(alice).delete(url).status_code == 200  # the author
    assert not EntryAttachment.objects.exists()


def test_attachments_are_limited_and_only_for_the_people_involved(alice, bob, carol) -> None:
    entry = pay(bob, alice, 100).json()
    assert attach(carol, entry["id"]).status_code == 403
    for _ in range(10):
        assert attach(bob, entry["id"]).status_code == 201
    assert attach(alice, entry["id"]).status_code == 400


# --- migrating the old ledger -------------------------------------------------------------


@pytest.mark.django_db(transaction=True)
def test_old_entries_keep_their_balances_after_the_migration() -> None:
    """Before 0005 each entry was one two-party fact; afterwards every net and every goods pair
    must come out exactly the same."""
    from django.db import connection
    from django.db.migrations.executor import MigrationExecutor

    from ledger.balances import balances

    before = [("ledger", "0004_item_debts")]
    executor = MigrationExecutor(connection)
    executor.migrate(before)
    old = executor.loader.project_state(before).apps.get_model("ledger", "LedgerEntry")
    ann = User.objects.create(username="ann")
    ben = User.objects.create(username="ben")
    old.objects.create(
        debtor_id=ben.pk, creditor_id=ann.pk, amount=1000, description="zakład", source_type="pact"
    )
    old.objects.create(
        kind="payment", debtor_id=ben.pk, creditor_id=ann.pk, amount=400, description="BLIK"
    )
    old.objects.create(
        kind="payment",
        status="pending",
        debtor_id=ben.pk,
        creditor_id=ann.pk,
        amount=100,
        description="Spłata długu",
    )
    old.objects.create(
        debtor_id=ann.pk,
        creditor_id=ben.pk,
        amount=2,
        currency="",
        item="piwo",
        description="kawa",
        source_type="manual",
    )
    try:
        latest = MigrationExecutor(connection)
        latest.migrate(latest.loader.graph.leaf_nodes())
        result = balances()
        assert {(m["user"].username, m["item"], m["net"]) for m in result.members} == {
            ("ann", "", 600),
            ("ben", "", -600),
            ("ann", "piwo", -2),
            ("ben", "piwo", 2),
        }
        assert [(p["debtor"].username, p["amount"]) for p in result.pending] == [("ben", 100)]
        manual = LedgerEntry.objects.get(source_type="manual")
        assert (manual.kind, manual.created_by_id, manual.title) == ("debt", ben.pk, "kawa")
        payment = LedgerEntry.objects.get(kind="payment", status="confirmed")
        assert payment.details == {"from": ben.pk, "to": ann.pk}
        assert payment.occurred_on == timezone.localdate()
    finally:
        LedgerEntry.objects.all().delete()
        User.objects.filter(username__in=["ann", "ben"]).delete()
        final = MigrationExecutor(connection)  # leave the test database fully migrated
        final.migrate(final.loader.graph.leaf_nodes())
