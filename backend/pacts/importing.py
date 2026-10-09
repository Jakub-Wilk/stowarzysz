"""One-off import of the old Google Sheet (exported as CSV) into pacts.

The sheet has no participants column, so rows are imported as the host's own pacts unless a
mapping says who was on the other side. Nothing is notified: these are historical records.
Columns: Nazwa zakładu/postanowienia, Warunek, Czas, Nagroda, Dodatkowe info, Rezultat, Wypłacone.
"""

import re
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from django.contrib.auth import get_user_model
from django.core.files import File
from django.db import transaction
from django.utils import timezone

from accounts.avatars import process_photo
from ledger.models import LedgerEntry
from pacts.models import OutcomeProposal, Pact, PactAttachment, PactParticipant

TITLE, CONDITION, DUE, PRIZE, INFO, RESULT, PAID = (
    "Nazwa zakładu/postanowienia",
    "Warunek",
    "Czas",
    "Nagroda",
    "Dodatkowe info",
    "Rezultat",
    "Wypłacone",
)
WON, LOST, VOID = "✅", "❌", "➖"  # noqa: RUF001 (the sheet's own symbols); ❓/empty = still open


class SheetImportError(Exception):
    """A row or mapping that can't be imported; the message is shown to the operator."""


@dataclass
class Opponent:
    user: Any
    stake_amount: int | None  # grosze


def parse_due(text: str) -> datetime | None:
    """Understands 23.01.2042, 2042-01-23 and a bare year (end of that year)."""
    text = text.strip()
    for pattern, fmt in (
        (r"\d{1,2}\.\d{1,2}\.\d{4}", "%d.%m.%Y"),
        (r"\d{4}-\d{2}-\d{2}", "%Y-%m-%d"),
        (r"\d{1,2}/\d{1,2}/\d{4}", "%d/%m/%Y"),
    ):
        match = re.search(pattern, text)
        if match:
            try:
                parsed = datetime.strptime(match.group(), fmt)
            except ValueError:
                return None
            return parsed.replace(hour=23, minute=59, tzinfo=UTC)
    if re.fullmatch(r"\d{4}", text):
        return datetime(int(text), 12, 31, 23, 59, tzinfo=UTC)
    return None


def get_user(username: str) -> Any:
    user = get_user_model().objects.filter(username=username).first()
    if user is None:
        raise SheetImportError(f"Nie ma użytkownika {username!r}.")
    return user


def _notes(row: dict[str, str], due_text: str, due: datetime | None) -> str:
    parts = []
    if row.get(PRIZE, "").strip():
        parts.append(f"Nagroda: {row[PRIZE].strip()}")
    if row.get(INFO, "").strip():
        parts.append(f"Dodatkowe info: {row[INFO].strip()}")
    if due_text and due is None:
        parts.append(f"Czas: {due_text}")  # couldn't be parsed into a deadline
    if row.get(PAID, "").strip():
        parts.append(f"Wypłacone: {row[PAID].strip()}")
    parts.append("Zaimportowano z arkusza.")
    return "\n".join(parts)


def import_row(
    row: dict[str, str], *, host: Any, opponents: list[Opponent], now: datetime | None = None
) -> Pact | None:
    """Create one pact from a sheet row; None when it was imported before."""
    now = now or timezone.now()
    title = row.get(TITLE, "").strip()
    if not title:
        return None
    if Pact.objects.filter(creator=host, title=title).exists():
        return None
    due_text = row.get(DUE, "").strip()
    due = parse_due(due_text)
    symbol = next((s for s in (WON, LOST, VOID) if s in row.get(RESULT, "")), "")
    paid = WON in row.get(PAID, "")
    bet = bool(opponents)

    status = Pact.Status.ACTIVE
    if symbol == VOID:
        status = Pact.Status.VOID
    elif symbol:
        status = Pact.Status.RESOLVED
    elif due is not None and due <= now:
        status = Pact.Status.AWAITING_RESULT

    with transaction.atomic():
        pact = Pact.objects.create(
            creator=host,
            kind="bet" if bet else "resolution",
            title=title,
            condition=row.get(CONDITION, "").strip() or title,
            notes=_notes(row, due_text, due),
            due_at=due,
            status=status,
            resolved_at=now if symbol else None,
        )
        PactParticipant.objects.create(
            pact=pact, user=host, role=PactParticipant.Role.HOST, state=PactParticipant.State.ACTIVE
        )
        settled = status in (Pact.Status.RESOLVED, Pact.Status.VOID)
        wagers = [
            PactParticipant.objects.create(
                pact=pact,
                user=o.user,
                stake_amount=o.stake_amount,
                state=(
                    (
                        PactParticipant.State.VOID
                        if symbol == VOID
                        else PactParticipant.State.SETTLED
                    )
                    if settled
                    else PactParticipant.State.ACTIVE
                ),
            )
            for o in opponents
        ]
        if not settled:
            return pact
        _record_result(pact, host, wagers, symbol, paid=paid, now=now)
    return pact


def _record_result(
    pact: Pact, host: Any, wagers: list[PactParticipant], symbol: str, *, paid: bool, now: datetime
) -> None:
    host_won = symbol == WON
    if symbol == VOID:
        result: dict[str, Any] = {"void": True}
        verdicts: dict[int, str] = {}
    elif wagers:
        result = {"winner": "host" if host_won else "opponent"}
    else:
        result = {"kept": host_won}
    claims = []
    if wagers:
        for wager in wagers:
            claim = dict(result)
            wager_verdicts = {}
            if symbol != VOID:
                wager_verdicts = {
                    host.pk: "won" if host_won else "lost",
                    wager.user_id: "lost" if host_won else "won",
                }
                if wager.stake_amount:
                    debtor, creditor = (wager.user, host) if host_won else (host, wager.user)
                    LedgerEntry.objects.create(
                        debtor=debtor,
                        creditor=creditor,
                        amount=wager.stake_amount,
                        description=pact.title,
                        source_type="pact",
                        source_id=pact.pk,
                        paid_marked_at=now if paid else None,
                        settled_at=now if paid else None,
                    )
            claims.append((wager, claim, wager_verdicts))
    else:
        verdicts = {} if symbol == VOID else {host.pk: "won" if host_won else "lost"}
        claims.append((None, result, verdicts))
    for wager, claim, claim_verdicts in claims:
        OutcomeProposal.objects.create(
            pact=pact,
            wager=wager,
            proposed_by=host,
            result=claim,
            state=OutcomeProposal.State.CONFIRMED,
            decided_at=now,
            verdicts={str(uid): v for uid, v in claim_verdicts.items()},
        )
    pact.outcome = (
        {"wagers": [{"participant_id": w.pk, "result": c} for w, c, _ in claims]}
        if wagers
        else {"result": result}
    )
    pact.save(update_fields=["outcome"])


def parse_mapping(raw: dict[str, Any]) -> dict[str, tuple[Any, list[Opponent]]]:
    """`{title: {"host": username, "opponents": [{"username": u, "stake_pln": 100}]}}`."""
    mapping = {}
    for title, entry in raw.items():
        host = get_user(entry["host"])
        opponents = [
            Opponent(
                get_user(o["username"]),
                round(float(o["stake_pln"]) * 100) if o.get("stake_pln") else None,
            )
            for o in entry.get("opponents", [])
        ]
        mapping[title] = (host, opponents)
    return mapping


def attach_images(pact: Pact, host: Any, directory: Path) -> int:
    """Attach the pictures named after a pact: `<title>.<ext>`, or `<title>__2.<ext>` etc. for
    more than one. Returns how many were attached."""
    count = 0
    for path in sorted(directory.iterdir()):
        if not path.is_file() or path.stem.split("__")[0] != pact.title:
            continue
        with path.open("rb") as handle:
            attachment = PactAttachment(pact=pact, uploaded_by=host)
            try:
                content = process_photo(File(handle))
            except Exception as exc:
                raise SheetImportError(f"{path.name}: nie da się wczytać obrazu.") from exc
            attachment.image.save(f"{path.stem[:40]}.webp".replace("/", "_"), content, save=False)
            attachment.save()
        count += 1
    return count
