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


def test_endpoints_require_auth(api_client: APIClient, alice: User, bob: User) -> None:
    entry = services.record_debt(alice, bob, 1000, "x")
    calls = [
        api_client.get("/api/ledger/balances/"),
        api_client.get("/api/ledger/entries/"),
        api_client.post(f"/api/ledger/entries/{entry.pk}/paid/"),
        api_client.post(f"/api/ledger/entries/{entry.pk}/confirm/"),
    ]
    assert [c.status_code for c in calls] == [401] * 4


def test_record_debt_rejects_bad_amounts_and_self_debt(alice: User, bob: User) -> None:
    from rest_framework.exceptions import ValidationError

    with pytest.raises(ValidationError):
        services.record_debt(alice, bob, 0, "x")
    with pytest.raises(ValidationError):
        services.record_debt(alice, alice, 100, "x")


def test_balances_net_per_counterpart(alice: User, bob: User, carol: User) -> None:
    services.record_debt(bob, alice, 1000, "a")  # bob owes alice 10
    services.record_debt(alice, bob, 400, "b")  # alice owes bob 4
    services.record_debt(alice, carol, 250, "c")  # alice owes carol 2.50
    resp = client_for(alice).get("/api/ledger/balances/")
    assert resp.status_code == 200
    rows = {r["user"]["username"]: r["amount"] for r in resp.json()}
    assert rows == {"bob": 600, "carol": -250}


def test_balances_drop_zero_and_settled(alice: User, bob: User) -> None:
    services.record_debt(bob, alice, 500, "a")
    services.record_debt(alice, bob, 500, "b")
    assert client_for(alice).get("/api/ledger/balances/").json() == []


def test_paid_then_confirmed_closes_the_entry(alice: User, bob: User) -> None:
    entry = services.record_debt(bob, alice, 1500, "bet")
    paid = client_for(bob).post(f"/api/ledger/entries/{entry.pk}/paid/")
    assert paid.status_code == 200
    assert paid.json()["paid_marked_at"] and not paid.json()["settled_at"]
    assert client_for(alice).get("/api/ledger/balances/").json()[0]["amount"] == 1500

    confirmed = client_for(alice).post(f"/api/ledger/entries/{entry.pk}/confirm/")
    assert confirmed.status_code == 200
    assert confirmed.json()["settled_at"]
    assert client_for(alice).get("/api/ledger/balances/").json() == []
    assert client_for(alice).get("/api/ledger/entries/?open=1").json() == []
    assert len(client_for(alice).get("/api/ledger/entries/").json()) == 1


def test_creditor_can_confirm_without_the_debtor_marking_paid(alice: User, bob: User) -> None:
    entry = services.record_debt(bob, alice, 100, "cash in hand")
    assert client_for(alice).post(f"/api/ledger/entries/{entry.pk}/confirm/").status_code == 200


def test_only_the_right_party_can_act(alice: User, bob: User, carol: User) -> None:
    entry = services.record_debt(bob, alice, 100, "x")
    assert client_for(alice).post(f"/api/ledger/entries/{entry.pk}/paid/").status_code == 403
    assert client_for(bob).post(f"/api/ledger/entries/{entry.pk}/confirm/").status_code == 403
    # strangers can't even see it
    assert client_for(carol).post(f"/api/ledger/entries/{entry.pk}/paid/").status_code == 404
    assert client_for(carol).get("/api/ledger/entries/").json() == []


def test_settled_entries_are_final(alice: User, bob: User) -> None:
    entry = services.record_debt(bob, alice, 100, "x")
    client_for(alice).post(f"/api/ledger/entries/{entry.pk}/confirm/")
    assert client_for(alice).post(f"/api/ledger/entries/{entry.pk}/confirm/").status_code == 400
    assert client_for(bob).post(f"/api/ledger/entries/{entry.pk}/paid/").status_code == 400
    assert LedgerEntry.objects.get(pk=entry.pk).amount == 100


def test_format_amount() -> None:
    assert services.format_amount(1000) == "10 zł"
    assert services.format_amount(750) == "7,50 zł"
    assert services.format_amount(5, "EUR") == "0,05 EUR"
