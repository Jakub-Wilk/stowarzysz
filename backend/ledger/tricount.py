"""Reads the raw JSON that `tricount-exporter --save-response` saves (the Tricount registry) into
plain entries the ledger can import. Pure: no Django, no network.

Amounts are decimals as printed; they become integer minor units here, never floats. Money
entries use the original currency of each entry (`amount_local`) when Tricount has one, so the
ledger converts it with its own rate like any other foreign-currency expense.
"""

from dataclasses import dataclass, field
from datetime import date, datetime
from decimal import Decimal, InvalidOperation
from typing import Any

from ledger.money import CURRENCIES, allocate

SOURCE_TYPE = "tricount"
MAX_SOURCE_ID = 2_147_483_647  # `LedgerEntry.source_id` is a PositiveIntegerField

# Tricount's `type_transaction` -> the ledger kind it becomes.
KINDS = {"NORMAL": "expense", "INCOME": "income", "BALANCE": "payment"}

# Substrings of a Tricount category (built-in or custom, lowercase) -> `LedgerEntry.Category`.
CATEGORY_HINTS = (
    ("lodg", "lodging"),
    ("accommod", "lodging"),
    ("hotel", "lodging"),
    ("rent", "bills"),
    ("bill", "bills"),
    ("utilit", "bills"),
    ("grocer", "groceries"),
    ("super", "groceries"),
    ("shop", "shopping"),
    ("restaur", "food"),
    ("food", "food"),
    ("drink", "food"),
    ("bar", "food"),
    ("transport", "transport"),
    ("fuel", "transport"),
    ("car", "transport"),
    ("entertain", "fun"),
    ("fun", "fun"),
    ("health", "health"),
    ("insur", "insurance"),
)


class TricountError(ValueError):
    """The dump is not something we can read (the message is shown to the admin, in Polish)."""


@dataclass(frozen=True)
class ParsedEntry:
    source_id: int
    kind: str  # "expense", "income" or "payment"
    occurred_on: date
    title: str
    currency: str
    category: str
    payer: str  # a Tricount display name (the one who received the money, for an income)
    total: int  # minor units of `currency`
    shares: dict[str, int]  # name -> minor units; sums to `total` (a payment: one receiver)
    attachments: int = 0


@dataclass
class ParsedTricount:
    title: str
    participants: list[str]
    entries: list[ParsedEntry]
    skipped_deleted: int = 0
    currencies: set[str] = field(default_factory=set)


def _name(membership: Any) -> str:
    try:
        return str(membership["RegistryMembershipNonUser"]["alias"]["display_name"])
    except (KeyError, TypeError) as exc:
        raise TricountError("Nie rozpoznaję uczestnika w pliku.") from exc


def _minor(amount: Any, currency: str) -> int:
    """`{"value": "-12.50", ...}` -> 1250 (the sign is dropped: direction comes from the kind)."""
    if currency not in CURRENCIES:
        raise TricountError(f"Nieobsługiwana waluta: {currency}.")
    try:
        value = abs(Decimal(str(amount["value"])))
    except (KeyError, TypeError, InvalidOperation) as exc:
        raise TricountError("Nie rozpoznaję kwoty w pliku.") from exc
    return int((value * 10 ** CURRENCIES[currency]).to_integral_value())


def category_for(entry: dict[str, Any]) -> str:
    text = f"{entry.get('category_custom') or ''} {entry.get('category') or ''}".lower()
    return next((key for hint, key in CATEGORY_HINTS if hint in text), "other")


def _parse_entry(entry: dict[str, Any]) -> ParsedEntry:
    kind = KINDS.get(str(entry.get("type_transaction", "NORMAL")).upper())
    if kind is None:
        raise TricountError(f"Nieznany typ wpisu: {entry.get('type_transaction')}.")
    source_id = entry.get("id")
    if not isinstance(source_id, int) or not 0 < source_id <= MAX_SOURCE_ID:
        raise TricountError("Wpis w pliku nie ma poprawnego numeru.")
    total_amount = entry.get("amount_local") or entry["amount"]
    currency = total_amount.get("currency") or entry["amount"]["currency"]
    total = _minor(total_amount, currency)
    weights: dict[str, int] = {}
    for allocation in entry["allocations"]:
        local = allocation.get("amount_local") or allocation["amount"]
        weight = _minor(local, local.get("currency") or currency)
        if weight:
            name = _name(allocation["membership"])
            weights[name] = weights.get(name, 0) + weight
    payer = _name(entry["membership_owned"])
    if total == 0 or not weights:
        raise TricountError(f"„{entry.get('description') or source_id}”: kwota zero.")
    names = sorted(weights)
    # Allocations are exact amounts or ratios already worked out into amounts; this only evens
    # out a grosz of rounding so they add up to the total.
    shares = dict(zip(names, allocate(total, [weights[n] for n in names]), strict=True))
    if kind == "payment":
        receivers = [name for name in shares if name != payer]
        if len(receivers) != 1:
            raise TricountError("Spłata musi mieć dokładnie jednego odbiorcę.")
        shares = {receivers[0]: total}
    title = " ".join(str(entry.get("description") or "").split())[:200]
    return ParsedEntry(
        source_id=source_id,
        kind=kind,
        occurred_on=datetime.fromisoformat(str(entry["date"])).date(),
        title=title or "Tricount",
        currency=currency,
        category=category_for(entry),
        payer=payer,
        total=total,
        shares=shares,
        attachments=len(entry.get("attachment") or []),
    )


def parse(dump: Any) -> ParsedTricount:
    try:
        registry = dump["Response"][0]["Registry"]
        participants = [_name(m) for m in registry["memberships"]]
        raw_entries = [e["RegistryEntry"] for e in registry["all_registry_entry"]]
        title = str(registry["title"])
    except (KeyError, IndexError, TypeError) as exc:
        raise TricountError(
            "To nie wygląda na plik z tricount-exporter (opcja --save-response)."
        ) from exc
    duplicates = sorted({n for n in participants if participants.count(n) > 1})
    if duplicates:
        raise TricountError(f"Uczestnicy o tych samych nazwach: {', '.join(duplicates)}.")
    parsed = ParsedTricount(title=title, participants=participants, entries=[])
    for raw in raw_entries:
        if str(raw.get("status", "")).upper() == "DELETED":
            parsed.skipped_deleted += 1
            continue
        try:
            entry = _parse_entry(raw)
        except (KeyError, TypeError, ValueError) as exc:
            if isinstance(exc, TricountError):
                raise
            raise TricountError("Nie rozpoznaję wpisu w pliku.") from exc
        unknown = ({entry.payer} | set(entry.shares)) - set(participants)
        if unknown:
            raise TricountError(f"Wpis odwołuje się do nieznanej osoby: {', '.join(unknown)}.")
        parsed.entries.append(entry)
        parsed.currencies.add(entry.currency)
    return parsed
