"""Exchange rates (`ledger.rates`): Frankfurter is never called for real here."""

import io
import json
import urllib.error
from datetime import date, timedelta
from decimal import Decimal
from typing import Any

import pytest
from django.utils import timezone
from rest_framework.test import APIClient

from accounts.models import User
from ledger import rates
from ledger.models import ExchangeRate

SATURDAY = date(2026, 3, 14)
FRIDAY = date(2026, 3, 13)


@pytest.fixture
def fetches(monkeypatch: pytest.MonkeyPatch) -> list[tuple[str, date]]:
    """Frankfurter answers 4.25 for any currency, dated the Friday before a weekend."""
    calls: list[tuple[str, date]] = []

    def fake_fetch(currency: str, day: date) -> tuple[Decimal, date]:
        calls.append((currency, day))
        published = day - timedelta(days=max(0, day.weekday() - 4))
        return Decimal("4.2537"), published

    monkeypatch.setattr(rates, "_fetch", fake_fetch)
    return calls


def test_the_base_currency_needs_no_rate(fetches, db) -> None:
    assert rates.rate_for("PLN", SATURDAY) == rates.Rate(Decimal(1), SATURDAY)
    assert fetches == []


def test_a_rate_is_fetched_once_and_kept(fetches, db) -> None:
    first = rates.rate_for("EUR", SATURDAY)
    second = rates.rate_for("EUR", SATURDAY)
    assert first == second == rates.Rate(Decimal("4.2537"), FRIDAY)  # a weekend uses Friday's
    assert fetches == [("EUR", SATURDAY)]
    assert ExchangeRate.objects.get().rate_date == FRIDAY


def test_today_is_not_kept_before_the_ecb_publishes(fetches, db, monkeypatch) -> None:
    today = timezone.localdate()
    monkeypatch.setattr(
        rates, "_fetch", lambda c, d: (Decimal("4.3"), today - timedelta(days=1))
    )  # yesterday's rate: today's isn't out yet
    rates.rate_for("EUR", today)
    assert not ExchangeRate.objects.exists()


def test_fetch_reads_frankfurter(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: list[str] = []

    def urlopen(request: Any, timeout: float) -> io.BytesIO:
        seen.append(request.full_url)
        body = {"amount": 1.0, "base": "EUR", "date": "2026-03-13", "rates": {"PLN": 4.2537}}
        return io.BytesIO(json.dumps(body).encode())

    monkeypatch.setattr("urllib.request.urlopen", urlopen)
    assert rates._fetch("EUR", SATURDAY) == (Decimal("4.2537"), FRIDAY)
    assert seen == ["https://api.frankfurter.dev/v1/2026-03-14?base=EUR&symbols=PLN"]


@pytest.mark.parametrize(
    "failure",
    [urllib.error.URLError("down"), TimeoutError(), ValueError("not json")],
)
def test_fetch_failures_become_a_polish_503(monkeypatch: pytest.MonkeyPatch, failure) -> None:
    def urlopen(request: Any, timeout: float) -> io.BytesIO:
        raise failure

    monkeypatch.setattr("urllib.request.urlopen", urlopen)
    with pytest.raises(rates.RateUnavailable) as caught:
        rates._fetch("EUR", SATURDAY)
    assert caught.value.status_code == 503


def test_the_rate_endpoint(fetches, db) -> None:
    url = "/api/ledger/rates/"
    assert APIClient().get(url, {"currency": "EUR"}).status_code == 401
    client = APIClient()
    client.force_authenticate(User.objects.create(username="ann"))
    resp = client.get(url, {"currency": "EUR", "date": SATURDAY.isoformat()})
    assert resp.status_code == 200
    assert resp.json() == {"currency": "EUR", "rate": "4.25370000", "rate_date": "2026-03-13"}
    assert client.get(url, {"currency": "XXX"}).status_code == 400
