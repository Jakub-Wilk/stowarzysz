"""Spending stats for the Statystyki view: what the group spent in a period, by category and by
person, and how it spread over time. All in the base currency, converted the same way as the
balances (`ExpenseKind.base_breakdown`), so the numbers agree with the entries to the grosz.

What counts, all confirmed only:
- expenses, and incomes (a negative expense: they reduce what was spent, so a refunded ticket
  counts as nothing);
- money debts, a category of their own ("Długi"): one is an expense the creditor paid for the
  debtor, so it adds to the total, the creditor's `paid` and the debtor's `share`. Goods
  debts aren't money, and payments only settle debts: neither is spending.
"""

from collections import Counter
from datetime import date, timedelta
from typing import Any

from django.contrib.auth import get_user_model
from django.utils import timezone

from ledger.kinds import ExpenseKind, get_kind
from ledger.models import LedgerEntry
from ledger.money import BASE_CURRENCY

DEBTS = ("debts", "Długi", "🤝")  # the stats-only category: key, label, emoji
DAY_BUCKETS_UP_TO = 62  # a longer period is charted by month

Status = LedgerEntry.Status


def _months(start: date, end: date) -> int:
    return (end.year - start.year) * 12 + end.month - start.month + 1


def _buckets(start: date, end: date, unit: str) -> list[str]:
    """Every day (or month) from `start` to `end`, as the labels the series is keyed by."""
    if unit == "day":
        return [(start + timedelta(days=i)).isoformat() for i in range((end - start).days + 1)]
    labels = []
    year, month = start.year, start.month
    for _ in range(_months(start, end)):
        labels.append(f"{year:04d}-{month:02d}")
        year, month = (year + 1, 1) if month == 12 else (year, month + 1)
    return labels


def spending(start: date | None, end: date | None) -> dict[str, Any]:
    """Stats for entries dated `start`..`end` (both included; a missing end is open, so
    `None, None` is all time)."""
    entries = LedgerEntry.objects.filter(
        status=Status.CONFIRMED, kind__in=("expense", "income", "debt")
    ).prefetch_related("obligations")
    if start:
        entries = entries.filter(occurred_on__gte=start)
    if end:
        entries = entries.filter(occurred_on__lte=end)
    rows = list(entries)

    expenses = income = 0
    by_category: Counter[str] = Counter()
    category_count: Counter[str] = Counter()
    share: Counter[int] = Counter()
    paid: Counter[int] = Counter()
    by_day: Counter[date] = Counter()
    for entry in rows:
        kind = get_kind(entry.kind)
        key = entry.category or "other"
        if isinstance(kind, ExpenseKind):
            breakdown = kind.base_breakdown(entry)
            total = kind.SIGN * breakdown.total
            for user, amount in breakdown.owed.items():
                share[user] += kind.SIGN * amount
            for user, amount in breakdown.paid.items():
                paid[user] += kind.SIGN * amount
            if kind.SIGN > 0:
                expenses += breakdown.total
            else:
                income += breakdown.total
        else:  # a debt: only its money, which the creditor paid out for the debtor
            key = DEBTS[0]
            total = 0
            for o in entry.obligations.all():
                if o.item:
                    continue
                total += o.amount
                share[o.debtor_id] += o.amount
                paid[o.creditor_id] += o.amount
            expenses += total
            if not total:
                continue
        by_category[key] += total
        category_count[key] += 1
        by_day[entry.occurred_on] += total

    first = min((e.occurred_on for e in rows), default=None)
    last = max((e.occurred_on for e in rows), default=None)
    begin = start or first or end or timezone.localdate()
    finish = end or last or begin
    unit = "day" if (finish - begin).days < DAY_BUCKETS_UP_TO else "month"
    series: dict[str, int] = dict.fromkeys(_buckets(begin, finish, unit), 0)
    for day, total in by_day.items():
        series[day.isoformat() if unit == "day" else day.strftime("%Y-%m")] += total

    labels = {key: (label, emoji) for key, label, emoji in [DEBTS]}
    labels.update({key: (label, "") for key, label in LedgerEntry.Category.choices})
    spent = expenses - income
    months = _months(begin, finish)
    users = get_user_model().objects.in_bulk(share.keys() | paid.keys())
    return {
        "currency": BASE_CURRENCY,
        "start": begin,
        "end": finish,
        "unit": unit,
        "spent": spent,
        "expenses": expenses,
        "income": income,
        "count": len(rows),
        "monthly_average": round(spent / months) if months > 1 else None,
        "categories": [
            {
                "key": key,
                "label": labels[key][0],
                "emoji": DEBTS[2] if key == DEBTS[0] else LedgerEntry.CATEGORY_EMOJI[key],
                "spent": total,
                "count": category_count[key],
            }
            for key, total in sorted(by_category.items(), key=lambda kv: (-kv[1], kv[0]))
            if total or category_count[key]
        ],
        "people": [
            {"user": users[user], "share": share[user], "paid": paid[user]}
            for user in sorted(share.keys() | paid.keys(), key=lambda u: (-share[u], u))
        ],
        "series": [{"period": period, "spent": total} for period, total in series.items()],
    }
