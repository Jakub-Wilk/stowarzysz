"""All ledger state changes go through here: validated by the entry's kind, locked, versioned,
and announced after commit.

The ledger is public, like Tricount: every change refreshes everyone's view over SSE (ids only),
and push goes only to the people it concerns. Other apps write to it only via `record_debts`.
"""

from dataclasses import replace
from datetime import date
from typing import Any
from uuid import uuid4

from django.contrib.auth import get_user_model
from django.db import transaction
from django.http import Http404
from django.utils import timezone
from rest_framework.exceptions import PermissionDenied, ValidationError

from accounts.avatars import process_photo
from core import events
from ledger import rates, tricount
from ledger.kinds import Conflict, DebtKind, Draft, EntryKind, Push, get_kind
from ledger.models import EntryAttachment, LedgerEntry, Obligation
from ledger.money import BASE_CURRENCY, CURRENCIES, Debt, to_base
from push.sender import send_push

Status = LedgerEntry.Status

MAX_ATTACHMENTS = 10  # per entry


def _announce(entry: LedgerEntry, pushes: list[Push]) -> None:
    def announce() -> None:
        events.broadcast("ledger.updated", {"entry_id": entry.pk})
        for push in pushes:
            send_push(push.user_ids, title=push.title, body=push.body, url=f"/ledger/{entry.pk}")

    transaction.on_commit(announce)


def _locked(entry_id: int) -> LedgerEntry:
    entry = LedgerEntry.objects.select_for_update().filter(pk=entry_id).first()
    if entry is None:
        raise Http404
    return entry


def _rate(draft: Draft) -> rates.Rate | None:
    """The day's rate for a foreign-currency draft (may hit the network: call it before locking)."""
    if draft.currency in ("", BASE_CURRENCY):
        return None
    return rates.rate_for(draft.currency, draft.occurred_on)


def _apply(entry: LedgerEntry, kind: EntryKind, draft: Draft, rate: rates.Rate | None) -> None:
    """Write a draft onto an entry and (re)derive what it means. Inside a transaction."""
    for field in ("title", "occurred_on", "amount", "currency", "item", "details"):
        setattr(entry, field, getattr(draft, field))
    entry.category, entry.note = draft.category, draft.note
    entry.source_type, entry.source_id = draft.source_type, draft.source_id
    entry.rate = rate.value if rate else None
    entry.rate_date = rate.day if rate else None
    entry.base_amount = kind.base_amount(entry)
    entry.save()
    entry.obligations.all().delete()
    Obligation.objects.bulk_create(
        Obligation(
            entry=entry,
            debtor_id=d.debtor_id,
            creditor_id=d.creditor_id,
            amount=d.amount,
            item=d.item,
        )
        for d in kind.obligations(entry)
    )


def create_entry(kind_key: str, author: Any, data: dict[str, Any]) -> LedgerEntry:
    """A member adds an expense, a goods debt or a payment."""
    kind = get_kind(kind_key)
    draft = kind.clean(data, author)
    rate = _rate(draft)
    with transaction.atomic():
        entry = LedgerEntry(
            kind=kind.key,
            status=Status.PENDING if kind.starts_pending else Status.CONFIRMED,
            created_by=author,
            updated_by=author,
        )
        _apply(entry, kind, draft, rate)
        _announce(entry, kind.announcements(entry, "created", author))
    return entry


def update_entry(entry_id: int, user: Any, data: dict[str, Any], version: int) -> LedgerEntry:
    """Replace an entry's content (an expense, by anyone). `version` is the one the user edited;
    if somebody saved in between, nothing is overwritten (409) and they reload."""
    current = LedgerEntry.objects.filter(pk=entry_id).first()
    if current is None:
        raise Http404
    kind = get_kind(current.kind)
    kind.check(current, user, "edit")  # fail fast, before validating or fetching a rate
    draft = kind.clean(data, user)
    rate = _rate(draft)
    with transaction.atomic():
        entry = _locked(entry_id)
        kind.check(entry, user, "edit")
        if entry.version != version:
            raise Conflict("Ktoś zmienił ten wpis w międzyczasie. Odśwież i spróbuj jeszcze raz.")
        entry.version += 1
        entry.updated_by = user
        _apply(entry, kind, draft, rate)
        _announce(entry, kind.announcements(entry, "updated", user))
    return entry


def _decide(entry_id: int, user: Any, action: str, status: str) -> LedgerEntry:
    with transaction.atomic():
        entry = _locked(entry_id)
        kind = get_kind(entry.kind)
        kind.check(entry, user, action)
        entry.status = status
        entry.decided_at = timezone.now()
        entry.version += 1
        entry.save(update_fields=["status", "decided_at", "version", "updated_at"])
        _announce(entry, kind.announcements(entry, status, user))
    return entry


def confirm(entry_id: int, user: Any) -> LedgerEntry:
    """The receiver confirms a payment: it now counts."""
    return _decide(entry_id, user, "confirm", Status.CONFIRMED)


def reject(entry_id: int, user: Any) -> LedgerEntry:
    """The receiver says they weren't paid: the payment never counts."""
    return _decide(entry_id, user, "reject", Status.REJECTED)


def cancel(entry_id: int, user: Any) -> LedgerEntry:
    """Withdraw an entry: delete an expense, take back a payment, drop a goods debt."""
    return _decide(entry_id, user, "cancel", Status.CANCELLED)


def record_debts(
    debts: list[Debt], *, title: str, source_type: str, source_id: int, on: date | None = None
) -> LedgerEntry | None:
    """Record what another app decided is owed (a settled pact) as one debt entry. It counts at
    once and can only change through its source. Nothing owed (a draw, a void) records nothing.
    Call inside the caller's transaction."""
    if not debts:
        return None
    kind = get_kind(DebtKind.key)
    draft = DebtKind.draft(
        debts,
        title=title,
        on=on or timezone.localdate(),
        source=source_type,
        source_id=source_id,
    )
    with transaction.atomic():
        entry = LedgerEntry(kind=kind.key, status=Status.CONFIRMED)
        _apply(entry, kind, draft, None)
        _announce(entry, kind.announcements(entry, "created", None))
    return entry


def add_attachment(entry_id: int, user: Any, upload: Any) -> EntryAttachment:
    """A picture on an entry: the receipt, proof of a transfer."""
    with transaction.atomic():
        entry = _locked(entry_id)
        get_kind(entry.kind).check(entry, user, "attach")
        if entry.attachments.count() >= MAX_ATTACHMENTS:
            raise ValidationError(f"Do wpisu można dodać maksymalnie {MAX_ATTACHMENTS} zdjęć.")
        attachment = EntryAttachment(entry=entry, uploaded_by=user)
        attachment.image.save(f"{uuid4().hex}.webp", process_photo(upload), save=False)
        attachment.save()
        _announce(entry, [])
    return attachment


def delete_attachment(entry_id: int, attachment_id: int, user: Any) -> None:
    """The uploader or the entry's author can take a picture down."""
    with transaction.atomic():
        entry = _locked(entry_id)
        attachment = entry.attachments.filter(pk=attachment_id).first()
        if attachment is None:
            raise Http404
        if user.pk not in (attachment.uploaded_by_id, entry.created_by_id):
            raise PermissionDenied("Zdjęcie może usunąć jego autor albo autor wpisu.")
        attachment.delete()
        _announce(entry, [])


def import_tricount(parsed: tricount.ParsedTricount, mapping: dict[str, int]) -> dict[str, int]:
    """Load a parsed Tricount into the ledger, quietly: everything counts at once, authored by
    whoever paid, and nobody gets a push (one refresh is broadcast at the end). Entries already
    imported (same Tricount id) are skipped, so a dump can be imported again. All or nothing.
    `mapping` is Tricount name -> user id and must cover every participant."""
    missing = [name for name in parsed.participants if name not in mapping]
    if missing:
        raise ValidationError({"mapping": f"Przypisz osoby: {', '.join(missing)}."})
    if len(set(mapping.values())) != len(mapping):
        raise ValidationError({"mapping": "Każda osoba może być przypisana tylko raz."})
    users = {u.pk: u for u in get_user_model().objects.members().filter(pk__in=mapping.values())}
    if len(users) != len(mapping):
        raise ValidationError({"mapping": "Nieznany lub nieaktywny poseł."})
    done = set(
        LedgerEntry.objects.filter(source_type=tricount.SOURCE_TYPE).values_list(
            "source_id", flat=True
        )
    )
    todo = [e for e in parsed.entries if e.source_id not in done]
    # Validate and fetch rates before opening the transaction (rates may hit the network).
    prepared: list[tuple[Any, EntryKind, Draft, rates.Rate | None, Any]] = []
    for entry in todo:
        author = users[mapping[entry.payer]]
        kind = get_kind(entry.kind)
        if entry.kind == "payment":
            kind_data = _payment_data(entry, mapping)
            if entry.currency != BASE_CURRENCY:  # payments are base currency only: convert
                rate = rates.rate_for(entry.currency, entry.occurred_on)
                converted = to_base(entry.total, rate.value, CURRENCIES[entry.currency])
                kind_data["amount"] = max(converted, 1)
        else:
            kind_data = {
                "title": entry.title,
                "occurred_on": entry.occurred_on,
                "note": "",
                "currency": entry.currency,
                "category": entry.category,
                "payers": [{"user_id": author.pk, "amount": entry.total}],
                "items": [
                    {
                        "name": entry.title[:100],
                        "amount": entry.total,
                        "split": "exact",
                        "shares": [
                            {"user_id": mapping[n], "weight": a} for n, a in entry.shares.items()
                        ],
                    }
                ],
            }
        draft = kind.clean(kind_data, author)
        rate = _rate(draft)  # a payment is always base currency: no rate
        draft = replace(draft, source_type=tricount.SOURCE_TYPE, source_id=entry.source_id)
        prepared.append((entry, kind, draft, rate, author))
    with transaction.atomic():
        for _, kind, draft, rate, author in prepared:
            row = LedgerEntry(
                kind=kind.key,
                status=Status.CONFIRMED,
                created_by=author,
                updated_by=author,
                decided_at=timezone.now() if kind.starts_pending else None,
            )
            _apply(row, kind, draft, rate)
        if prepared:
            transaction.on_commit(lambda: events.broadcast("ledger.updated", {"entry_id": None}))
    return {
        "imported": len(prepared),
        "skipped_existing": len(parsed.entries) - len(todo),
        "skipped_deleted": parsed.skipped_deleted,
    }


def _payment_data(entry: tricount.ParsedEntry, mapping: dict[str, int]) -> dict[str, Any]:
    (receiver,) = entry.shares
    return {
        "occurred_on": entry.occurred_on,
        "amount": entry.total,
        "item": "",
        "note": f"Import z Tricount: {entry.title}"[:500],
        "to_user_id": mapping[receiver],
    }
