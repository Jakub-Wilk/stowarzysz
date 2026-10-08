"""Everything kind-specific about a pact: terms, outcomes and what gets settled.

To add a kind, subclass `PactKind` and `register()` an instance; models, endpoints and the
generic services don't change.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any

from rest_framework.exceptions import ValidationError


@dataclass(frozen=True)
class Terms:
    """What one invited person agreed to (before they accepted)."""

    user_id: int
    stake_amount: int | None
    stake_note: str


@dataclass(frozen=True)
class Debt:
    """Money one participant owes another once a wager is settled (minor units)."""

    debtor_id: int
    creditor_id: int
    amount: int


class PactKind(ABC):
    key: str
    has_money: bool = False
    allows_open_join: bool = False  # may be created open for anyone to ask to join
    needs_due_date: bool = False

    @abstractmethod
    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Return the cleaned kind-specific config or raise ValidationError."""

    @abstractmethod
    def validate_terms(self, terms: Terms) -> Terms:
        """Check one invitee's stake and return it cleaned."""

    @abstractmethod
    def validate_outcome(self, result: dict[str, Any]) -> dict[str, Any]:
        """Return the cleaned outcome claim or raise ValidationError."""

    @abstractmethod
    def settle(
        self, *, host_id: int, wager_user_id: int, terms: Terms, result: dict[str, Any]
    ) -> list[Debt]:
        """The debts that follow from one confirmed outcome (empty when nothing is owed)."""


class BetKind(PactKind):
    """The host bets the condition happens; each opponent bets it doesn't. Every opponent is a
    separate wager with their own stake, and the host matches it: the loser of a wager owes the
    winner that wager's stake."""

    key = "bet"
    has_money = True
    allows_open_join = True

    HOST = "host"
    OPPONENT = "opponent"
    DRAW = "draw"

    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        return {}

    def validate_terms(self, terms: Terms) -> Terms:
        stake_note = terms.stake_note.strip()
        if not terms.stake_amount and not stake_note:
            raise ValidationError("Podaj stawkę: kwotę albo opis stawki.")
        return Terms(terms.user_id, terms.stake_amount or None, stake_note)

    def validate_outcome(self, result: dict[str, Any]) -> dict[str, Any]:
        winner = result.get("winner")
        if winner not in (self.HOST, self.OPPONENT, self.DRAW):
            raise ValidationError("Wynik musi wskazywać zwycięzcę: host, przeciwnik albo remis.")
        return {"winner": winner}

    def settle(
        self, *, host_id: int, wager_user_id: int, terms: Terms, result: dict[str, Any]
    ) -> list[Debt]:
        winner = result.get("winner")
        if not terms.stake_amount or winner not in (self.HOST, self.OPPONENT):
            return []
        if winner == self.HOST:
            return [Debt(debtor_id=wager_user_id, creditor_id=host_id, amount=terms.stake_amount)]
        return [Debt(debtor_id=host_id, creditor_id=wager_user_id, amount=terms.stake_amount)]


KINDS: dict[str, PactKind] = {}


def register(kind: PactKind) -> None:
    KINDS[kind.key] = kind


def get_kind(key: str) -> PactKind:
    try:
        return KINDS[key]
    except KeyError:
        raise ValidationError("Nieznany rodzaj zakładu.") from None


register(BetKind())
