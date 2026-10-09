from typing import Any

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from ledger import services
from ledger.models import LedgerEntry

pytestmark = pytest.mark.usefixtures("run_on_commit")


@pytest.fixture(autouse=True)
def _no_push(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("ledger.services.send_push", lambda *a, **kw: None)


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


def balances(client: APIClient) -> dict[str, Any]:
    resp = client.get("/api/ledger/balances/")
    assert resp.status_code == 200
    return resp.json()


def pairs_of(body: dict[str, Any]) -> dict[tuple[str, str], int]:
    return {
        (p["debtor"]["username"], p["creditor"]["username"]): p["amount"] for p in body["pairs"]
    }


def pay(client: APIClient, to: User, amount: int, **extra: Any) -> Any:
    return client.post(
        "/api/ledger/payments/", {"to_user_id": to.pk, "amount": amount, **extra}, format="json"
    )


def act(client: APIClient, entry_id: int, action: str) -> Any:
    return client.post(f"/api/ledger/entries/{entry_id}/{action}/")


def test_endpoints_require_auth(api_client: APIClient, alice: User, bob: User) -> None:
    entry = services.record_debt(alice, bob, 1000, "x")
    calls = [
        api_client.get("/api/ledger/balances/"),
        api_client.get("/api/ledger/entries/"),
        api_client.post("/api/ledger/payments/", {"to_user_id": bob.pk, "amount": 1}),
        api_client.post(f"/api/ledger/entries/{entry.pk}/confirm/"),
        api_client.post(f"/api/ledger/entries/{entry.pk}/reject/"),
        api_client.post(f"/api/ledger/entries/{entry.pk}/cancel/"),
    ]
    assert [c.status_code for c in calls] == [401] * 6


def test_record_debt_rejects_bad_amounts_and_self_debt(alice: User, bob: User) -> None:
    from rest_framework.exceptions import ValidationError

    with pytest.raises(ValidationError):
        services.record_debt(alice, bob, 0, "x")
    with pytest.raises(ValidationError):
        services.record_debt(alice, alice, 100, "x")


def test_debts_count_at_once_and_balances_are_public(alice: User, bob: User, carol: User) -> None:
    services.record_debt(bob, alice, 1000, "a")  # bob owes alice 10
    services.record_debt(alice, bob, 400, "b")  # alice owes bob 4
    services.record_debt(alice, carol, 250, "c")  # alice owes carol 2.50
    body = balances(client_for(carol))  # carol isn't in the first two, and still sees everything
    assert {m["user"]["username"]: m["net"] for m in body["members"]} == {
        "alice": 350,
        "bob": -600,
        "carol": 250,
    }
    assert [m["user"]["username"] for m in body["members"]] == ["alice", "carol", "bob"]
    assert pairs_of(body) == {("bob", "alice"): 600, ("alice", "carol"): 250}  # netted per pair


def test_opposite_debts_cancel_out(alice: User, bob: User) -> None:
    services.record_debt(bob, alice, 500, "a")
    services.record_debt(alice, bob, 500, "b")
    body = balances(client_for(alice))
    assert body["pairs"] == [] and body["members"] == []


def test_a_payment_only_counts_once_the_receiver_confirms(alice: User, bob: User) -> None:
    services.record_debt(bob, alice, 1500, "bet")
    resp = pay(client_for(bob), alice, 1000, note="BLIK")
    assert resp.status_code == 201, resp.json()
    entry = resp.json()
    assert (entry["kind"], entry["status"], entry["description"]) == ("payment", "pending", "BLIK")
    assert (entry["debtor"]["username"], entry["creditor"]["username"]) == ("bob", "alice")
    assert pairs_of(balances(client_for(alice))) == {("bob", "alice"): 1500}  # not yet

    confirmed = act(client_for(alice), entry["id"], "confirm")
    assert confirmed.status_code == 200 and confirmed.json()["status"] == "confirmed"
    assert confirmed.json()["decided_at"]
    assert pairs_of(balances(client_for(alice))) == {("bob", "alice"): 500}
    assert act(client_for(alice), entry["id"], "confirm").status_code == 409  # only once


def test_paying_the_whole_debt_clears_it_and_the_entries_stay(alice: User, bob: User) -> None:
    debt = services.record_debt(bob, alice, 1500, "bet")
    payment = pay(client_for(bob), alice, 1500).json()
    act(client_for(alice), payment["id"], "confirm")
    body = balances(client_for(alice))
    assert body["pairs"] == [] and body["members"] == []
    rows = client_for(alice).get("/api/ledger/entries/").json()
    assert [(r["kind"], r["status"]) for r in rows] == [
        ("payment", "confirmed"),
        ("debt", "confirmed"),
    ]
    assert LedgerEntry.objects.get(pk=debt.pk).status == "confirmed"  # the debt itself is untouched


def test_an_overpayment_turns_the_balance_around(alice: User, bob: User) -> None:
    services.record_debt(bob, alice, 1000, "bet")
    act(client_for(alice), pay(client_for(bob), alice, 1500).json()["id"], "confirm")
    assert pairs_of(balances(client_for(alice))) == {("alice", "bob"): 500}


def test_the_receiver_can_reject_and_the_payer_can_cancel(alice: User, bob: User) -> None:
    services.record_debt(bob, alice, 1000, "bet")
    first = pay(client_for(bob), alice, 400).json()
    rejected = act(client_for(alice), first["id"], "reject")
    assert rejected.json()["status"] == "rejected"
    second = pay(client_for(bob), alice, 400).json()
    cancelled = act(client_for(bob), second["id"], "cancel")
    assert cancelled.json()["status"] == "cancelled"
    assert pairs_of(balances(client_for(alice))) == {("bob", "alice"): 1000}  # neither counted
    assert act(client_for(alice), second["id"], "confirm").status_code == 409  # already decided


def test_only_the_right_party_can_decide_a_payment(alice: User, bob: User, carol: User) -> None:
    services.record_debt(bob, alice, 1000, "bet")
    entry = pay(client_for(bob), alice, 400).json()["id"]
    assert act(client_for(bob), entry, "confirm").status_code == 403  # can't confirm your own
    assert act(client_for(bob), entry, "reject").status_code == 403
    assert act(client_for(alice), entry, "cancel").status_code == 403  # only the payer cancels
    assert act(client_for(carol), entry, "confirm").status_code == 403
    assert act(client_for(carol), 9999, "confirm").status_code == 404
    assert pairs_of(balances(client_for(alice))) == {("bob", "alice"): 1000}


def test_debts_cannot_be_confirmed_or_cancelled(alice: User, bob: User) -> None:
    debt = services.record_debt(bob, alice, 1000, "bet")
    assert act(client_for(alice), debt.pk, "confirm").status_code == 409
    assert act(client_for(bob), debt.pk, "cancel").status_code == 409


def test_payment_validation(alice: User, bob: User) -> None:
    assert pay(client_for(bob), bob, 100).status_code == 400  # yourself
    assert pay(client_for(bob), alice, 0).status_code == 400
    assert (
        client_for(bob)
        .post("/api/ledger/payments/", {"to_user_id": 9999, "amount": 5}, format="json")
        .status_code
        == 400
    )
    User.objects.filter(pk=alice.pk).update(is_active=False)
    assert pay(client_for(bob), alice, 100).status_code == 400  # inactive
    assert LedgerEntry.objects.count() == 0


def test_every_member_sees_every_entry_newest_first(alice: User, bob: User, carol: User) -> None:
    first = services.record_debt(bob, alice, 100, "first")
    second = services.record_debt(bob, alice, 200, "second")
    rows = client_for(carol).get("/api/ledger/entries/").json()
    assert [r["id"] for r in rows] == [second.pk, first.pk]


def test_payments_notify_the_other_side_and_everyone_refreshes(
    alice: User, bob: User, carol: User, monkeypatch: pytest.MonkeyPatch
) -> None:
    pushes: list[dict[str, Any]] = []
    broadcasts: list[str] = []
    monkeypatch.setattr(
        "ledger.services.send_push", lambda ids, **kw: pushes.append({"ids": list(ids), **kw})
    )
    monkeypatch.setattr("core.events.broadcast", lambda event, data: broadcasts.append(event))
    services.record_debt(bob, alice, 1000, "bet")
    entry = pay(client_for(bob), alice, 400).json()["id"]
    assert pushes[-1]["ids"] == [alice.pk] and pushes[-1]["title"] == "Wpłata do potwierdzenia"
    act(client_for(alice), entry, "confirm")
    assert pushes[-1]["ids"] == [bob.pk] and pushes[-1]["title"] == "Wpłata potwierdzona"
    assert broadcasts == ["ledger.updated"] * 3


def test_format_amount() -> None:
    assert services.format_amount(1000) == "10 zł"
    assert services.format_amount(750) == "7,50 zł"
    assert services.format_amount(5, "EUR") == "0,05 EUR"


@pytest.mark.django_db(transaction=True)
def test_the_migration_turns_closed_debts_into_confirmed_payments() -> None:
    """Debts used to be closed per entry; after migrating each closed one has a payment."""
    from django.db import connection
    from django.db.migrations.executor import MigrationExecutor
    from django.utils import timezone

    executor = MigrationExecutor(connection)
    executor.migrate([("ledger", "0001_initial")])
    old = executor.loader.project_state([("ledger", "0001_initial")]).apps
    entry_model = old.get_model("ledger", "LedgerEntry")
    ann = User.objects.create(username="ann")
    ben = User.objects.create(username="ben")
    now = timezone.now()
    entry_model.objects.create(debtor_id=ben.pk, creditor_id=ann.pk, amount=700, description="open")
    entry_model.objects.create(
        debtor_id=ben.pk, creditor_id=ann.pk, amount=300, description="closed", settled_at=now
    )

    executor = MigrationExecutor(connection)
    executor.migrate([("ledger", "0002_payments")])  # data copy, before the old columns go
    try:
        rows = list(LedgerEntry.objects.order_by("id").values_list("kind", "status", "amount"))
        assert rows == [
            ("debt", "confirmed", 700),
            ("debt", "confirmed", 300),
            ("payment", "confirmed", 300),
        ]
        _members, pairs = services.group_balances()
        assert [(p["debtor"].username, p["amount"]) for p in pairs] == [("ben", 700)]
    finally:
        LedgerEntry.objects.all().delete()
        User.objects.filter(username__in=["ann", "ben"]).delete()
        final = MigrationExecutor(connection)  # leave the test database fully migrated
        final.migrate(final.loader.graph.leaf_nodes())
