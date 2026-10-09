"""All ledger state changes go through here (locked, validated, events sent after commit)."""

from collections import defaultdict
from typing import Any

from django.contrib.auth import get_user_model
from django.db import transaction
from django.utils import timezone
from rest_framework.exceptions import APIException, PermissionDenied, ValidationError

from core import events
from ledger.models import LedgerEntry
from push.sender import send_push


def _notify(entry: LedgerEntry, user_ids: list[int], title: str, body: str) -> None:
    """The ledger is public, so everyone's view refreshes (ids only); push only the people it
    concerns."""

    def announce() -> None:
        events.broadcast("ledger.updated", {"entry_id": entry.pk})
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


class Conflict(APIException):
    status_code = 409
    default_detail = "Ta wpłata nie czeka już na decyzję."
    default_code = "conflict"


def create_payment(
    payer: Any, receiver_id: int, amount: int, note: str = "", *, currency: str = "PLN"
) -> LedgerEntry:
    """The payer says they paid `receiver` `amount`. It counts once the receiver confirms."""
    if amount <= 0:
        raise ValidationError({"amount": "Kwota musi być dodatnia."})
    receiver = get_user_model().objects.members().filter(pk=receiver_id).first()
    if receiver is None:
        raise ValidationError({"to_user_id": "Nieznany lub nieaktywny poseł."})
    if receiver.pk == payer.pk:
        raise ValidationError({"to_user_id": "Nie możesz zapłacić samemu sobie."})
    with transaction.atomic():
        entry = LedgerEntry.objects.create(
            kind=LedgerEntry.Kind.PAYMENT,
            status=LedgerEntry.Status.PENDING,
            debtor=payer,
            creditor=receiver,
            amount=amount,
            currency=currency,
            description=note.strip()[:200] or "Spłata długu",
        )
        _notify(
            entry,
            [receiver.pk],
            "Wpłata do potwierdzenia",
            f"{payer.username} twierdzi, że zapłacił(a) Ci {format_amount(amount, currency)}",
        )
    return entry


def _decide(entry_id: int, user: Any, *, party: str, status: str) -> LedgerEntry:
    with transaction.atomic():
        entry = (
            LedgerEntry.objects.select_for_update()
            .select_related("debtor", "creditor")
            .get(pk=entry_id)
        )
        if entry.kind != LedgerEntry.Kind.PAYMENT or entry.status != LedgerEntry.Status.PENDING:
            raise Conflict()
        allowed = entry.creditor_id if party == "receiver" else entry.debtor_id
        if user.pk != allowed:
            raise PermissionDenied(
                "Wpłatę potwierdza lub odrzuca odbiorca, a wycofać ją może tylko płacący."
            )
        entry.status = status
        entry.decided_at = timezone.now()
        entry.save(update_fields=["status", "decided_at"])
        amount = format_amount(entry.amount, entry.currency)
        if status == LedgerEntry.Status.CONFIRMED:
            _notify(
                entry,
                [entry.debtor_id],
                "Wpłata potwierdzona",
                f"{entry.creditor.username} potwierdził(a) wpłatę {amount}",
            )
        elif status == LedgerEntry.Status.REJECTED:
            _notify(
                entry,
                [entry.debtor_id],
                "Wpłata odrzucona",
                f"{entry.creditor.username} nie potwierdza wpłaty {amount}",
            )
        else:
            _notify(
                entry,
                [entry.creditor_id],
                "Wpłata wycofana",
                f"{entry.debtor.username} wycofał(a) wpłatę {amount}",
            )
    return entry


def confirm_payment(entry_id: int, user: Any) -> LedgerEntry:
    """The receiver confirms they were paid: the payment now counts."""
    return _decide(entry_id, user, party="receiver", status=LedgerEntry.Status.CONFIRMED)


def reject_payment(entry_id: int, user: Any) -> LedgerEntry:
    """The receiver says they weren't paid: the payment never counts."""
    return _decide(entry_id, user, party="receiver", status=LedgerEntry.Status.REJECTED)


def cancel_payment(entry_id: int, user: Any) -> LedgerEntry:
    """The payer takes back a payment the receiver hasn't answered."""
    return _decide(entry_id, user, party="payer", status=LedgerEntry.Status.CANCELLED)


def group_balances() -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Balances for the whole group, like a Tricount "Bilans", from the confirmed entries.

    Returns `(members, pairs)`. `members` has each person's `net` per currency (positive: others
    owe them), biggest creditor first; people at zero are left out. `pairs` is who owes whom
    after netting each two people's entries against each other, biggest first. A confirmed
    payment counts as a debt the other way round, which is what pays a debt off.
    """
    net: dict[tuple[int, str], int] = defaultdict(int)
    pair_net: dict[tuple[int, int, str], int] = defaultdict(int)  # (low id, high id): low owes
    people: dict[int, Any] = {}
    confirmed = LedgerEntry.objects.filter(status=LedgerEntry.Status.CONFIRMED).select_related(
        "debtor", "creditor"
    )
    for entry in confirmed:
        people[entry.debtor_id] = entry.debtor
        people[entry.creditor_id] = entry.creditor
        owing, owed = (entry.debtor_id, entry.creditor_id)
        if entry.kind == LedgerEntry.Kind.PAYMENT:
            owing, owed = owed, owing
        net[(owing, entry.currency)] -= entry.amount
        net[(owed, entry.currency)] += entry.amount
        low, high = sorted((owing, owed))
        pair_net[(low, high, entry.currency)] += (1 if owing == low else -1) * entry.amount

    members = [
        {"user": people[user_id], "currency": currency, "net": amount}
        for (user_id, currency), amount in net.items()
        if amount != 0
    ]
    members.sort(key=lambda m: (-m["net"], m["user"].username))

    pairs = []
    for (low, high, currency), amount in pair_net.items():
        if amount == 0:
            continue
        debtor, creditor = (low, high) if amount > 0 else (high, low)
        pairs.append(
            {
                "debtor": people[debtor],
                "creditor": people[creditor],
                "currency": currency,
                "amount": abs(amount),
            }
        )
    pairs.sort(key=lambda p: (-p["amount"], p["debtor"].username))
    return members, pairs
