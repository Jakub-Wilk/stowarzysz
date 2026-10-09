"""Everything kind-specific about a ledger entry: what its `details` hold, which obligations it
creates, who may do what with it and how it is announced.

Every kind is a different way of arriving at the same fact, `money.Debt` (X owes Y n units):

- `expense`: somebody paid for something others shared. Its breakdown (who paid, who owes what)
  becomes the obligations that settle it up.
- `debt`: exactly the debts given, from a settled pact or goods a member says they are owed.
- `payment`: somebody paid somebody back. It is pending until the receiver confirms it, and then
  the receiver owes the payer that amount, which is what cancels the debt.

To add a kind, subclass `EntryKind` and `register()` an instance, plus its input serializer
(`serializers.INPUTS`) and its UI (`frontend/src/features/ledger/kinds/`); models, endpoints
and the generic services don't change.
"""

from abc import ABC, abstractmethod
from collections import Counter
from collections.abc import Iterable
from dataclasses import asdict, dataclass
from datetime import date
from decimal import Decimal
from typing import Any, ClassVar

from django.contrib.auth import get_user_model
from rest_framework.exceptions import APIException, PermissionDenied, ValidationError

from ledger.models import LedgerEntry
from ledger.money import (
    BASE_CURRENCY,
    CURRENCIES,
    Breakdown,
    Debt,
    allocate,
    format_amount,
    settle_up,
)

Status = LedgerEntry.Status

ACTIONS = ("confirm", "reject", "cancel", "edit", "attach")
ITEM_MAX_LENGTH = 60


class Conflict(APIException):
    """The entry is not in a state that allows this (any more)."""

    status_code = 409
    default_detail = "Tego wpisu nie można już zmienić."
    default_code = "conflict"


@dataclass(frozen=True)
class Draft:
    """A validated entry, ready to be stored (the services add the bookkeeping)."""

    title: str
    occurred_on: date
    amount: int | None
    currency: str
    details: dict[str, Any]
    item: str = ""
    category: str = ""
    note: str = ""
    source_type: str = ""
    source_id: int | None = None


@dataclass(frozen=True)
class Push:
    user_ids: list[int]
    title: str
    body: str


def normalize_item(item: str, field: str = "item") -> str:
    """Tidy a free-text item ('  Piwo ' -> 'Piwo'). Empty stays empty (that means money)."""
    item = " ".join(item.split())
    if len(item) > ITEM_MAX_LENGTH:
        raise ValidationError({field: f"Nazwa może mieć najwyżej {ITEM_MAX_LENGTH} znaków."})
    return item


def require_members(user_ids: Iterable[int], field: str) -> None:
    ids = set(user_ids)
    members = get_user_model().objects.members().filter(pk__in=ids)
    if set(members.values_list("pk", flat=True)) != ids:
        raise ValidationError({field: "Nieznany lub nieaktywny poseł."})


def _username(user: Any) -> str:
    return user.username if user is not None else "System"


class EntryKind(ABC):
    key: ClassVar[str]
    starts_pending: ClassVar[bool] = False  # counts only once someone confirms it

    @abstractmethod
    def clean(self, data: dict[str, Any], author: Any) -> Draft:
        """Validate a request (already shaped by the kind's input serializer) into a draft."""

    @abstractmethod
    def obligations(self, entry: LedgerEntry) -> list[Debt]:
        """What the entry means, in base-currency minor units or quantities of goods."""

    def breakdown(self, entry: LedgerEntry) -> Breakdown | None:
        """Who paid and who owes what, in the entry's currency, for kinds where that is the
        explanation (an expense). Others are described by their kind in the UI."""
        return None

    def base_amount(self, entry: LedgerEntry) -> int | None:
        """The headline amount in the base currency; None for goods and mixed entries."""
        if entry.amount is not None and entry.currency == BASE_CURRENCY:
            return entry.amount
        return None

    def check(self, entry: LedgerEntry, user: Any, action: str) -> None:
        """Raise `Conflict` if the entry's kind or state doesn't allow `action`, or
        `PermissionDenied` if `user` may not do it. By default nothing is allowed."""
        raise Conflict()

    def actions(self, entry: LedgerEntry, user: Any) -> dict[str, bool]:
        """What `user` can do right now: the `actions` block of the API, so the UI never
        re-derives the rules."""
        allowed = {}
        for action in ACTIONS:
            try:
                self.check(entry, user, action)
            except Conflict, PermissionDenied:
                allowed[action] = False
            else:
                allowed[action] = True
        return allowed

    def involved(self, entry: LedgerEntry) -> set[int]:
        """Everyone the entry concerns."""
        return {u for d in self.obligations(entry) for u in (d.debtor_id, d.creditor_id)}

    @abstractmethod
    def announcements(self, entry: LedgerEntry, event: str, actor: Any) -> list[Push]:
        """Push notifications for `event` (created, updated, confirmed, rejected, cancelled),
        by `actor` (None: the system). Everyone's view refreshes anyway, over SSE."""


class ExpenseKind(EntryKind):
    """Somebody paid for something the others shared, Tricount's bread and butter.

    `details`: `payers` [{user_id, amount}] and `items` [{name, amount, split, shares:
    [{user_id, weight}]}], amounts in minor units of the entry's currency. A plain expense is one
    item; a receipt is many, each shared by its own people. An item is split `equal`ly, by
    `shares` (weights) or into `exact` amounts. Payers and items both add up to the total.

    The breakdown (paid and owed per person) is converted to the base currency in one go, and
    `settle_up` turns its nets into obligations. Anyone may edit or delete an expense.
    """

    key = "expense"
    SPLITS = ("equal", "shares", "exact")

    def clean(self, data: dict[str, Any], author: Any) -> Draft:
        currency = data["currency"]
        if currency not in CURRENCIES:
            raise ValidationError({"currency": "Nieobsługiwana waluta."})
        title = " ".join(data["title"].split())
        if not title:
            raise ValidationError({"title": "Podaj, za co był wydatek."})
        items = [self._clean_item(item, currency) for item in data["items"]]
        total = sum(item["amount"] for item in items)
        payers = sorted(data["payers"], key=lambda p: p["user_id"])
        if len({p["user_id"] for p in payers}) != len(payers):
            raise ValidationError({"payers": "Każdy płacący może wystąpić tylko raz."})
        if sum(p["amount"] for p in payers) != total:
            raise ValidationError(
                {"payers": f"Płacący muszą razem wyłożyć {format_amount(total, currency)}."}
            )
        people = {p["user_id"] for p in payers} | {
            s["user_id"] for item in items for s in item["shares"]
        }
        require_members(people, "items")
        return Draft(
            title=title,
            occurred_on=data["occurred_on"],
            amount=total,
            currency=currency,
            category=data["category"],
            note=data["note"].strip(),
            details={
                "payers": [{"user_id": p["user_id"], "amount": p["amount"]} for p in payers],
                "items": items,
            },
        )

    def _clean_item(self, item: dict[str, Any], currency: str) -> dict[str, Any]:
        name = " ".join(item["name"].split())
        if not name:
            raise ValidationError({"items": "Każda pozycja potrzebuje nazwy."})
        split = item["split"]
        shares = sorted(item["shares"], key=lambda s: s["user_id"])
        if len({s["user_id"] for s in shares}) != len(shares):
            raise ValidationError({"items": f"„{name}”: każda osoba może wystąpić tylko raz."})
        if split == "equal":
            shares = [{"user_id": s["user_id"], "weight": 1} for s in shares]
        elif split == "exact" and sum(s["weight"] for s in shares) != item["amount"]:
            amount = format_amount(item["amount"], currency)
            raise ValidationError({"items": f"„{name}”: kwoty muszą dać razem {amount}."})
        return {
            "name": name,
            "amount": item["amount"],
            "split": split,
            "shares": [{"user_id": s["user_id"], "weight": s["weight"]} for s in shares],
        }

    def breakdown(self, entry: LedgerEntry) -> Breakdown:
        owed: Counter[int] = Counter()
        for position, item in enumerate(entry.details["items"]):
            owed.update(self._owed(item, position))
        paid = {p["user_id"]: p["amount"] for p in entry.details["payers"]}
        return Breakdown(paid, dict(owed))

    @staticmethod
    def _owed(item: dict[str, Any], position: int) -> dict[int, int]:
        """One item divided among its sharers. An odd grosz goes to whoever is first, and the
        first place rotates from item to item, so on a long receipt it doesn't always land on
        the same person."""
        weights = {s["user_id"]: s["weight"] for s in item["shares"]}
        ids = sorted(weights)
        turn = position % len(ids)
        ids = ids[turn:] + ids[:turn]
        return dict(zip(ids, allocate(item["amount"], [weights[u] for u in ids]), strict=True))

    def base_breakdown(self, entry: LedgerEntry) -> Breakdown:
        return self.breakdown(entry).to_base(entry.rate or Decimal(1), CURRENCIES[entry.currency])

    def obligations(self, entry: LedgerEntry) -> list[Debt]:
        return settle_up(self.base_breakdown(entry).nets())

    def base_amount(self, entry: LedgerEntry) -> int | None:
        return self.base_breakdown(entry).total

    def involved(self, entry: LedgerEntry) -> set[int]:
        breakdown = self.breakdown(entry)
        return set(breakdown.paid) | set(breakdown.owed)

    def check(self, entry: LedgerEntry, user: Any, action: str) -> None:
        if action not in ("edit", "cancel", "attach"):
            raise Conflict()
        if entry.status != Status.CONFIRMED:
            raise Conflict("Ten wydatek został usunięty.")

    def announcements(self, entry: LedgerEntry, event: str, actor: Any) -> list[Push]:
        titles = {
            "created": "Nowy wydatek",
            "updated": "Wydatek zmieniony",
            "cancelled": "Wydatek usunięty",
        }
        if event not in titles:
            return []
        recipients = sorted(self.involved(entry) - {getattr(actor, "pk", None)})
        amount = format_amount(entry.amount or 0, entry.currency)
        body = f"{_username(actor)}: {entry.title} ({amount})"
        return [Push(recipients, titles[event], body)] if recipients else []


class DebtKind(EntryKind):
    """Somebody owes somebody: the debts of a settled pact (recorded by the system through
    `services.record_debts`) or goods a member says they are owed ("2 x kawa").

    `details`: `debts` [{debtor_id, creditor_id, amount, item}], the obligations as they are.
    Money comes only from the system; a member records money owed to them as an expense, the
    Tricount way. A manual debt can be deleted by the person owed; a pact's belongs to the pact.
    """

    key = "debt"

    def clean(self, data: dict[str, Any], author: Any) -> Draft:
        item = normalize_item(data["item"])
        if not item:
            raise ValidationError({"item": "Podaj, co jest winien."})
        require_members([data["debtor_id"]], "debtor_id")
        if data["debtor_id"] == author.pk:
            raise ValidationError({"debtor_id": "Nie możesz być sobie winien."})
        debt = Debt(data["debtor_id"], author.pk, data["amount"], item)
        return DebtKind.draft(
            [debt], title=data["note"].strip() or item, on=data["occurred_on"], source="manual"
        )

    @staticmethod
    def draft(
        debts: list[Debt],
        *,
        title: str,
        on: date,
        source: str,
        source_id: int | None = None,
    ) -> Draft:
        """A debt entry for `debts`. One unit gives a headline amount; a mix (a pot plus stakes
        in kind) has none, and the UI lists the debts."""
        debts = [Debt(d.debtor_id, d.creditor_id, d.amount, normalize_item(d.item)) for d in debts]
        if any(d.amount <= 0 for d in debts):
            raise ValidationError("Kwota musi być dodatnia.")
        if any(d.debtor_id == d.creditor_id for d in debts):
            raise ValidationError("Nie można być dłużnikiem samego siebie.")
        units = {d.item.casefold() for d in debts}
        single = len(units) == 1
        item = debts[0].item if single else ""
        return Draft(
            title=title[:200],
            occurred_on=on,
            amount=sum(d.amount for d in debts) if single else None,
            currency=BASE_CURRENCY if single and not item else "",
            item=item,
            details={"debts": [asdict(d) for d in debts]},
            source_type=source,
            source_id=source_id,
        )

    def obligations(self, entry: LedgerEntry) -> list[Debt]:
        return [Debt(**debt) for debt in entry.details["debts"]]

    def check(self, entry: LedgerEntry, user: Any, action: str) -> None:
        if action == "attach":
            if user.pk not in self.involved(entry):
                raise PermissionDenied("Zdjęcie mogą dodać tylko osoby z tego wpisu.")
            return
        if action != "cancel" or entry.status != Status.CONFIRMED:
            raise Conflict()
        if entry.source_type != "manual":
            raise Conflict("Rozliczenie zakładu zmienia się tylko przez zakład.")
        if user.pk not in {d.creditor_id for d in self.obligations(entry)}:
            raise PermissionDenied("Wpis może usunąć tylko osoba, której ktoś jest winien.")

    def announcements(self, entry: LedgerEntry, event: str, actor: Any) -> list[Push]:
        users = get_user_model().objects.in_bulk(self.involved(entry))
        pushes = []
        for debt in self.obligations(entry):
            debtor, creditor = users[debt.debtor_id], users[debt.creditor_id]
            what = format_amount(debt.amount, item=debt.item)
            if event == "created" and actor is None:
                body = f"{debtor.username} jest Ci winien {what}: {entry.title}"
                pushes.append(Push([creditor.pk], "Nowe rozliczenie", body))
            elif event == "created" and actor.pk == creditor.pk:
                body = f"{creditor.username} zapisał(a), że jesteś winien(-na) {what}"
                pushes.append(Push([debtor.pk], "Nowe rozliczenie", body))
            elif event == "cancelled":
                body = f"{_username(actor)} usunął(-ęła) wpis: {what}"
                pushes.append(Push([debtor.pk], "Wpis usunięty", body))
        return pushes


class PaymentKind(EntryKind):
    """Somebody paid somebody back (money, or goods returned). It counts once the receiver
    confirms; the receiver can reject it and the payer can take it back until then.

    `details`: `from` (the payer) and `to` (the receiver). Its obligation runs from the receiver
    back to the payer, which is what cancels the debt it pays. Base currency or goods only.
    """

    key = "payment"
    starts_pending = True

    def clean(self, data: dict[str, Any], author: Any) -> Draft:
        item = normalize_item(data["item"])
        receiver = data["to_user_id"]
        require_members([receiver], "to_user_id")
        if receiver == author.pk:
            raise ValidationError({"to_user_id": "Nie możesz zapłacić samemu sobie."})
        return Draft(
            title="Zwrot" if item else "Spłata",
            occurred_on=data["occurred_on"],
            amount=data["amount"],
            currency="" if item else BASE_CURRENCY,
            item=item,
            note=data["note"].strip(),
            details={"from": author.pk, "to": receiver},
        )

    def obligations(self, entry: LedgerEntry) -> list[Debt]:
        payer, receiver = entry.details["from"], entry.details["to"]
        return [Debt(receiver, payer, entry.amount or 0, entry.item)]

    def check(self, entry: LedgerEntry, user: Any, action: str) -> None:
        payer, receiver = entry.details["from"], entry.details["to"]
        if action == "attach":
            if user.pk not in (payer, receiver):
                raise PermissionDenied("Zdjęcie mogą dodać tylko płacący i odbiorca.")
            return
        if action not in ("confirm", "reject", "cancel"):
            raise Conflict()
        if entry.status != Status.PENDING:
            raise Conflict("Ta wpłata nie czeka już na decyzję.")
        if action == "cancel" and user.pk != payer:
            raise PermissionDenied("Wpłatę może wycofać tylko płacący.")
        if action != "cancel" and user.pk != receiver:
            raise PermissionDenied("Wpłatę potwierdza lub odrzuca jej odbiorca.")

    def announcements(self, entry: LedgerEntry, event: str, actor: Any) -> list[Push]:
        payer, receiver = entry.details["from"], entry.details["to"]
        goods = bool(entry.item)
        what = format_amount(entry.amount or 0, entry.currency, entry.item)
        name = _username(actor)
        noun = "zwrot" if goods else "wpłatę"
        if event == "created":
            verb = "oddał(a)" if goods else "zapłacił(a)"
            title = "Oddane do potwierdzenia" if goods else "Wpłata do potwierdzenia"
            return [Push([receiver], title, f"{name} twierdzi, że {verb} Ci {what}")]
        if event == "confirmed":
            title = "Zwrot potwierdzony" if goods else "Wpłata potwierdzona"
            return [Push([payer], title, f"{name} potwierdził(a) {noun} {what}")]
        if event == "rejected":
            title = "Zwrot odrzucony" if goods else "Wpłata odrzucona"
            noun_gen = "zwrotu" if goods else "wpłaty"
            return [Push([payer], title, f"{name} nie potwierdza {noun_gen} {what}")]
        if event == "cancelled":
            title = "Zwrot wycofany" if goods else "Wpłata wycofana"
            return [Push([receiver], title, f"{name} wycofał(a) {noun} {what}")]
        return []


KINDS: dict[str, EntryKind] = {}


def register(kind: EntryKind) -> None:
    KINDS[kind.key] = kind


def get_kind(key: str) -> EntryKind:
    try:
        return KINDS[key]
    except KeyError:
        raise ValidationError({"kind": "Nieznany rodzaj wpisu."}) from None


register(ExpenseKind())
register(DebtKind())
register(PaymentKind())
