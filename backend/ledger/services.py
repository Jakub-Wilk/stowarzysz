"""All ledger state changes go through here (locked, validated, events sent after commit)."""

from collections import defaultdict
from typing import Any

from django.db import transaction
from django.utils import timezone
from rest_framework.exceptions import PermissionDenied, ValidationError

from core import events
from ledger.models import LedgerEntry
from push.sender import send_push


def _notify(entry: LedgerEntry, user_ids: list[int], title: str, body: str) -> None:
    def announce() -> None:
        for user_id in {entry.debtor_id, entry.creditor_id}:
            events.notify_user(user_id, "ledger.updated", {"entry_id": entry.pk})
        send_push(user_ids, title=title, body=body, url="/ledger")

    transaction.on_commit(announce)


def format_amount(amount: int, currency: str = "PLN") -> str:
    """Minor units to a Polish amount: 750 -> '7,50 zł'; whole amounts drop the decimals."""
    major, minor = divmod(amount, 100)
    text = f"{major}" if minor == 0 else f"{major},{minor:02d}"
    return f"{text} zł" if currency == "PLN" else f"{text} {currency}"


def record_debt(
    debtor: Any,
    creditor: Any,
    amount: int,
    description: str,
    *,
    source_type: str = "",
    source_id: int | None = None,
    currency: str = "PLN",
) -> LedgerEntry:
    """Record that `debtor` owes `creditor` `amount` minor units. Call inside a transaction."""
    if amount <= 0:
        raise ValidationError("Kwota musi być dodatnia.")
    if debtor.pk == creditor.pk:
        raise ValidationError("Nie można być dłużnikiem samego siebie.")
    entry = LedgerEntry.objects.create(
        debtor=debtor,
        creditor=creditor,
        amount=amount,
        currency=currency,
        description=description,
        source_type=source_type,
        source_id=source_id,
    )
    _notify(
        entry,
        [creditor.pk],
        "Nowe rozliczenie",
        f"{debtor.username} jest Ci winien {format_amount(amount, currency)}: {description}",
    )
    return entry


def _locked_open_entry(entry_id: int) -> LedgerEntry:
    entry = (
        LedgerEntry.objects.select_for_update()
        .select_related("debtor", "creditor")
        .get(pk=entry_id)
    )
    if entry.settled_at is not None:
        raise ValidationError("To rozliczenie jest już zamknięte.")
    return entry


def mark_paid(entry_id: int, user: Any) -> LedgerEntry:
    """The debtor says they paid; the creditor still has to confirm."""
    with transaction.atomic():
        entry = _locked_open_entry(entry_id)
        if entry.debtor_id != user.pk:
            raise PermissionDenied("Tylko dłużnik może oznaczyć dług jako zapłacony.")
        if entry.paid_marked_at is None:
            entry.paid_marked_at = timezone.now()
            entry.save(update_fields=["paid_marked_at"])
            _notify(
                entry,
                [entry.creditor_id],
                "Zapłacono?",
                f"{entry.debtor.username} twierdzi, że oddał(a) "
                f"{format_amount(entry.amount, entry.currency)}",
            )
    return entry


def confirm_paid(entry_id: int, user: Any) -> LedgerEntry:
    """The creditor confirms they were paid, which closes the entry."""
    with transaction.atomic():
        entry = _locked_open_entry(entry_id)
        if entry.creditor_id != user.pk:
            raise PermissionDenied("Tylko wierzyciel może potwierdzić zapłatę.")
        entry.settled_at = timezone.now()
        entry.save(update_fields=["settled_at"])
        _notify(
            entry,
            [entry.debtor_id],
            "Rozliczenie zamknięte",
            f"{entry.creditor.username} potwierdził(a) zapłatę "
            f"{format_amount(entry.amount, entry.currency)}",
        )
    return entry


def balances_for(user: Any) -> list[tuple[Any, str, int]]:
    """Open balances per counterpart and currency as (other, currency, amount).

    Positive: the other person owes `user`. Negative: `user` owes them. Zeros are left out.
    """
    totals: dict[tuple[int, str], int] = defaultdict(int)
    people: dict[int, Any] = {}
    open_entries = LedgerEntry.objects.filter(settled_at__isnull=True).select_related(
        "debtor", "creditor"
    )
    for entry in open_entries.filter(creditor=user):
        people[entry.debtor_id] = entry.debtor
        totals[(entry.debtor_id, entry.currency)] += entry.amount
    for entry in open_entries.filter(debtor=user):
        people[entry.creditor_id] = entry.creditor
        totals[(entry.creditor_id, entry.currency)] -= entry.amount
    return [
        (people[other_id], currency, amount)
        for (other_id, currency), amount in sorted(totals.items())
        if amount != 0
    ]
