"""All ledger state changes go through here: validated by the entry's kind, locked, versioned,
and announced after commit.

The ledger is public, like Tricount: every change refreshes everyone's view over SSE (ids only),
and push goes only to the people it concerns. Other apps write to it only via `record_debts`.
"""

from datetime import date
from typing import Any
from uuid import uuid4

from django.db import transaction
from django.http import Http404
from django.utils import timezone
from rest_framework.exceptions import PermissionDenied, ValidationError

from accounts.avatars import process_photo
from core import events
from ledger import rates
from ledger.kinds import Conflict, DebtKind, Draft, EntryKind, Push, get_kind
from ledger.models import EntryAttachment, LedgerEntry, Obligation
from ledger.money import BASE_CURRENCY, Debt
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
