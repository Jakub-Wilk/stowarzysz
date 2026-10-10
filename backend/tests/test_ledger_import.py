from decimal import Decimal
from typing import Any

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from ledger import rates
from ledger.balances import balances
from ledger.models import LedgerEntry

pytestmark = pytest.mark.usefixtures("run_on_commit")

PREVIEW = "/api/ledger/import/preview/"
IMPORT = "/api/ledger/import/"


def member(name: str) -> dict[str, Any]:
    return {"RegistryMembershipNonUser": {"alias": {"display_name": name}}}


def money(value: str, currency: str = "PLN") -> dict[str, str]:
    return {"value": value, "currency": currency}


def entry(
    id_: int,
    payer: str,
    total: str,
    shares: dict[str, str],
    *,
    kind: str = "NORMAL",
    currency: str = "PLN",
    description: str = "Piwo",
    status: str = "ACTIVE",
) -> dict[str, Any]:
    return {
        "RegistryEntry": {
            "id": id_,
            "status": status,
            "type_transaction": kind,
            "description": description,
            "date": "2024-05-04 12:00:00.000000",
            "category": "UNCATEGORIZED",
            "amount": money(f"-{total}", currency),
            "membership_owned": member(payer),
            "allocations": [
                {"membership": member(name), "amount": money(value, currency), "type": "AMOUNT"}
                for name, value in shares.items()
            ],
        }
    }


def dump(*entries: dict[str, Any]) -> dict[str, Any]:
    return {
        "Response": [
            {
                "Registry": {
                    "title": "Wyjazd",
                    "memberships": [member("Alice"), member("Bob"), member("Carol")],
                    "all_registry_entry": list(entries),
                }
            }
        ]
    }


@pytest.fixture
def people(db) -> dict[str, User]:
    return {n: User.objects.create(username=n.lower()) for n in ("Alice", "Bob", "Carol")}


@pytest.fixture
def mapping(people: dict[str, User]) -> dict[str, int]:
    return {name: user.pk for name, user in people.items()}


@pytest.fixture(autouse=True)
def _no_push(monkeypatch: pytest.MonkeyPatch) -> list[Any]:
    sent: list[Any] = []
    monkeypatch.setattr("ledger.services.send_push", lambda *a, **kw: sent.append(kw))
    return sent


def admin() -> APIClient:
    client = APIClient()
    client.force_authenticate(User.objects.create_superuser("boss", "pass-12345-x"))
    return client


SAMPLE = dump(
    entry(1, "Alice", "30.00", {"Alice": "10.00", "Bob": "10.00", "Carol": "10.00"}),
    entry(2, "Bob", "10.00", {"Alice": "10.00"}, kind="BALANCE", description="Spłata"),
)


@pytest.mark.parametrize("url", [PREVIEW, IMPORT])
def test_endpoints_need_login_and_superuser(url: str, people: dict[str, User]) -> None:
    assert APIClient().post(url, {"dump": SAMPLE}, format="json").status_code == 401
    client = APIClient()
    client.force_authenticate(people["Alice"])
    assert client.post(url, {"dump": SAMPLE}, format="json").status_code == 403


def test_preview_suggests_members(people: dict[str, User]) -> None:
    response = admin().post(PREVIEW, {"dump": SAMPLE}, format="json")
    assert response.status_code == 200
    body = response.json()
    assert (body["expenses"], body["payments"], body["already_imported"]) == (1, 1, 0)
    assert body["participants"][0] == {"name": "Alice", "user_id": people["Alice"].pk}
    assert body["first_date"] == "2024-05-04"


def test_preview_rejects_foreign_files(db) -> None:
    response = admin().post(PREVIEW, {"dump": {"hello": 1}}, format="json")
    assert response.status_code == 400


def test_import_creates_entries_quietly_and_balances_match(
    mapping: dict[str, int], people: dict[str, User], _no_push: list[Any]
) -> None:
    response = admin().post(IMPORT, {"dump": SAMPLE, "mapping": mapping}, format="json")
    assert response.status_code == 200, response.content
    assert response.json() == {"imported": 2, "skipped_existing": 0, "skipped_deleted": 0}
    assert _no_push == []
    payment = LedgerEntry.objects.get(kind="payment")
    assert payment.status == "confirmed"
    assert payment.created_by == people["Bob"]
    assert payment.details == {"from": people["Bob"].pk, "to": people["Alice"].pk}
    net = {m["user"].pk: m["net"] for m in balances().members if m["item"] == ""}
    assert net[people["Alice"].pk] == 1000
    assert net.get(people["Bob"].pk, 0) == 0
    assert net[people["Carol"].pk] == -1000


def test_import_twice_skips_everything(mapping: dict[str, int]) -> None:
    client = admin()
    client.post(IMPORT, {"dump": SAMPLE, "mapping": mapping}, format="json")
    again = client.post(IMPORT, {"dump": SAMPLE, "mapping": mapping}, format="json")
    assert again.json()["imported"] == 0
    assert again.json()["skipped_existing"] == 2
    assert LedgerEntry.objects.count() == 2


def test_incomplete_mapping_is_rejected(mapping: dict[str, int]) -> None:
    del mapping["Carol"]
    response = admin().post(IMPORT, {"dump": SAMPLE, "mapping": mapping}, format="json")
    assert response.status_code == 400
    assert LedgerEntry.objects.count() == 0


def test_deleted_entries_are_skipped(mapping: dict[str, int]) -> None:
    deleted = dump(entry(3, "Alice", "5.00", {"Bob": "5.00"}, status="DELETED"))
    response = admin().post(IMPORT, {"dump": deleted, "mapping": mapping}, format="json")
    assert response.json() == {"imported": 0, "skipped_existing": 0, "skipped_deleted": 1}


def test_foreign_currency_expense_uses_the_ledger_rate(
    mapping: dict[str, int], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(rates, "_fetch", lambda currency, day: (Decimal("4.25"), day))
    eur = dump(entry(4, "Alice", "10.00", {"Alice": "5.00", "Bob": "5.00"}, currency="EUR"))
    admin().post(IMPORT, {"dump": eur, "mapping": mapping}, format="json")
    row = LedgerEntry.objects.get()
    assert (row.currency, row.amount, row.base_amount) == ("EUR", 1000, 4250)


def test_rounding_is_evened_out(mapping: dict[str, int]) -> None:
    odd = dump(entry(5, "Alice", "10.00", {"Alice": "3.33", "Bob": "3.33", "Carol": "3.33"}))
    admin().post(IMPORT, {"dump": odd, "mapping": mapping}, format="json")
    shares = LedgerEntry.objects.get().details["items"][0]["shares"]
    assert sum(s["weight"] for s in shares) == 1000
