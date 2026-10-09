"""Exchange rates for the day something was paid, from Frankfurter (ECB reference rates).

A rate is fetched once per currency and day and kept in `ExchangeRate` for good, since a past
day's rate never changes. The one exception is today before the ECB publishes (~16:00 CET):
Frankfurter then answers with yesterday's rate, which is used but not stored.

Fetching blocks (stdlib `urllib`, short timeout), so callers do it before opening a transaction,
and the expense form warms the cache through `GET /api/ledger/rates/` while the user types.
"""

import json
import urllib.request
from dataclasses import dataclass
from datetime import date
from decimal import Decimal

from django.utils import timezone
from rest_framework.exceptions import APIException

from ledger.models import ExchangeRate
from ledger.money import BASE_CURRENCY

FRANKFURTER_URL = "https://api.frankfurter.dev/v1"
TIMEOUT_SECONDS = 3
PRECISION = Decimal("1e-8")  # what `ExchangeRate.rate` stores; fresh rates are rounded alike


class RateUnavailable(APIException):
    status_code = 503
    default_detail = "Nie udało się pobrać kursu waluty. Spróbuj ponownie za chwilę."
    default_code = "rate_unavailable"


@dataclass(frozen=True)
class Rate:
    """One unit of a currency is worth `value` base units, as published on `day`."""

    value: Decimal
    day: date


def rate_for(currency: str, day: date) -> Rate:
    if currency == BASE_CURRENCY:
        return Rate(Decimal(1), day)
    cached = ExchangeRate.objects.filter(currency=currency, requested_on=day).first()
    if cached is not None:
        return Rate(cached.rate, cached.rate_date)
    value, published = _fetch(currency, day)
    rate = Rate(value.quantize(PRECISION), published)
    if published == day or day < timezone.localdate():
        ExchangeRate.objects.get_or_create(
            currency=currency,
            requested_on=day,
            defaults={"rate": rate.value, "rate_date": rate.day},
        )
    return rate


def _fetch(currency: str, day: date) -> tuple[Decimal, date]:
    """Ask Frankfurter; it answers for the last working day up to `day`."""
    url = f"{FRANKFURTER_URL}/{day.isoformat()}?base={currency}&symbols={BASE_CURRENCY}"
    request = urllib.request.Request(url, headers={"User-Agent": "stowarzysz"})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            body = json.load(response, parse_float=Decimal)
        return Decimal(body["rates"][BASE_CURRENCY]), date.fromisoformat(body["date"])
    except (OSError, ValueError, KeyError, TypeError) as exc:  # network, JSON, unexpected shape
        raise RateUnavailable() from exc
