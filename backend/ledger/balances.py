"""The group's Bilans: everyone's totals and who should pay whom, straight from the obligations.

Money is settled across the whole group (`settle_up`: the fewest transfers, whoever originally
lent it); goods are settled per pair, since "kolacja" owed to Bob is owed to Bob. Suggestions
assume pending payments will go through, so "Zapłać" doesn't ask for the same money twice;
those payments are listed separately until the receiver answers.
"""

from collections import defaultdict
from dataclasses import dataclass
from typing import Any

from django.contrib.auth import get_user_model
from django.db.models import Min, QuerySet, Sum
from django.db.models.functions import Lower

from ledger.models import LedgerEntry, Obligation
from ledger.money import settle_up

Status = LedgerEntry.Status

Unit = str  # "" for money (base currency), else an item's case-insensitive key


@dataclass(frozen=True)
class Balances:
    members: list[dict[str, Any]]  # {user, item, net}: net > 0 means the others owe them
    settlements: list[dict[str, Any]]  # {debtor, creditor, item, amount}: what to pay to be even
    pending: list[dict[str, Any]]  # {entry_id, debtor (payer), creditor (receiver), item, amount}


def _nets(obligations: QuerySet) -> tuple[dict[tuple[int, Unit], int], dict[Unit, str]]:
    """Net per (user, unit) in two aggregates, plus a display spelling for each unit."""
    keyed = obligations.annotate(unit=Lower("item"))
    nets: dict[tuple[int, Unit], int] = defaultdict(int)
    labels: dict[Unit, str] = {}
    for side, sign in (("creditor", 1), ("debtor", -1)):
        for row in keyed.values(side, "unit").annotate(total=Sum("amount"), label=Min("item")):
            nets[(row[side], row["unit"])] += sign * row["total"]
            labels.setdefault(row["unit"], row["label"])
    return nets, labels


def _pairs(obligations: QuerySet) -> dict[tuple[int, int, Unit], int]:
    """Goods netted per pair: (debtor, creditor, unit) -> quantity, each pair once."""
    rows = (
        obligations.exclude(item="")
        .annotate(unit=Lower("item"))
        .values("debtor", "creditor", "unit")
        .annotate(total=Sum("amount"))
    )
    net: dict[tuple[int, int, Unit], int] = defaultdict(int)
    for row in rows:
        low, high = sorted((row["debtor"], row["creditor"]))
        net[(low, high, row["unit"])] += row["total"] if row["debtor"] == low else -row["total"]
    return {
        (low, high, unit) if amount > 0 else (high, low, unit): abs(amount)
        for (low, high, unit), amount in net.items()
        if amount
    }


def balances() -> Balances:
    confirmed = Obligation.objects.filter(entry__status=Status.CONFIRMED)
    planned = Obligation.objects.filter(entry__status__in=(Status.CONFIRMED, Status.PENDING))
    nets, labels = _nets(confirmed)
    planned_nets, planned_labels = _nets(planned)
    labels = planned_labels | labels

    money = {user: net for (user, unit), net in planned_nets.items() if unit == "" and net}
    transfers = [(d.debtor_id, d.creditor_id, "", d.amount) for d in settle_up(money)]
    transfers += [(d, c, unit, amount) for (d, c, unit), amount in _pairs(planned).items()]
    waiting = list(LedgerEntry.objects.filter(kind="payment", status=Status.PENDING))

    people = {u for (u, _unit) in nets} | {u for t in transfers for u in t[:2]}
    people |= {u for e in waiting for u in (e.details["from"], e.details["to"])}
    users = get_user_model().objects.in_bulk(people)

    members = [
        {"user": users[user], "item": labels[unit], "net": net}
        for (user, unit), net in nets.items()
        if net
    ]
    members.sort(key=lambda m: (m["item"].casefold(), -m["net"], m["user"].username))
    settlements = [
        {"debtor": users[d], "creditor": users[c], "item": labels[unit], "amount": amount}
        for d, c, unit, amount in transfers
    ]
    settlements.sort(key=lambda s: (s["item"].casefold(), -s["amount"], s["debtor"].username))
    pending = [
        {
            "entry_id": entry.pk,
            "debtor": users[entry.details["from"]],
            "creditor": users[entry.details["to"]],
            "item": entry.item,
            "amount": entry.amount,
        }
        for entry in waiting
    ]
    return Balances(members, settlements, pending)
