"""Everything kind-specific about a pact: terms, outcomes, who approves joiners, what is owed.

To add a kind, subclass `PactKind` and `register()` an instance; models, endpoints and the
generic services don't change.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any, ClassVar

from rest_framework.exceptions import ValidationError

DRAW = "draw"
WON, LOST, DREW = "won", "lost", "draw"


@dataclass(frozen=True)
class Terms:
    """What one person puts into a pact. `side` is only used by sided kinds."""

    user_id: int
    stake_amount: int | None = None
    stake_note: str = ""
    side: str = ""


@dataclass(frozen=True)
class Party:
    """A participant as `settle` sees them."""

    user_id: int
    is_host: bool
    side: str
    stake_amount: int | None


@dataclass(frozen=True)
class Debt:
    """Money one participant owes another once a claim is settled (minor units)."""

    debtor_id: int
    creditor_id: int
    amount: int


@dataclass(frozen=True)
class Settlement:
    debts: list[Debt]
    verdicts: dict[int, str]  # user id -> won / lost / draw (people with no verdict are omitted)


def _allocate(total: int, weights: list[int]) -> list[int]:
    """Split `total` in proportion to `weights` in whole units, summing exactly to `total`
    (largest remainder; ties go to the earlier entry)."""
    weight_sum = sum(weights)
    shares = [total * w // weight_sum for w in weights]
    leftover = total - sum(shares)
    by_remainder = sorted(
        range(len(weights)), key=lambda i: (-(total * weights[i] % weight_sum), i)
    )
    for i in by_remainder[:leftover]:
        shares[i] += 1
    return shares


class PactKind(ABC):
    key: ClassVar[str]
    has_money: ClassVar[bool] = False
    joinable: ClassVar[bool] = True  # anyone may ask to join (the participants decide)
    judged_by_everyone: ClassVar[bool] = False  # a Sejmik vote of all members decides it
    needs_due_date: ClassVar[bool] = False
    wager_based: ClassVar[bool] = False  # each opponent is a separate wager against the host
    all_must_answer: ClassVar[bool] = False  # starts only once every invitee has answered

    @abstractmethod
    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Return the cleaned kind-specific config or raise ValidationError."""

    @abstractmethod
    def validate_terms(
        self, terms: Terms, config: dict[str, Any], *, host: bool, final: bool
    ) -> Terms:
        """Check one person's terms and return them cleaned.

        `final` is false for what the creator pre-fills for an invitee (which may be partial)
        and true once the person is actually committing (accepting, requesting to join)."""

    @abstractmethod
    def validate_outcome(self, result: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
        """Return the cleaned outcome claim or raise ValidationError."""

    @abstractmethod
    def settle(
        self, parties: list[Party], result: dict[str, Any], config: dict[str, Any]
    ) -> Settlement:
        """What follows from one confirmed outcome (debts and who won)."""

    def join_approvers(self, host_id: int, active_ids: list[int]) -> set[int]:
        """Who must approve a join request. Default: only the host."""
        return {host_id}


def _clean_amount(terms: Terms) -> int | None:
    return terms.stake_amount or None


def _clean_sides(config: dict[str, Any]) -> list[str]:
    raw = config.get("sides", ["tak", "nie"])
    if not isinstance(raw, list) or not 2 <= len(raw) <= 6:
        raise ValidationError({"sides": "Podaj od 2 do 6 stron."})
    sides = [str(s).strip() for s in raw]
    if any(not s or len(s) > 32 for s in sides) or len(set(sides)) != len(sides):
        raise ValidationError({"sides": "Strony muszą być niepuste, różne i krótsze niż 32 znaki."})
    return sides


class BetKind(PactKind):
    """The host bets the condition happens; each opponent bets it doesn't. Every opponent is a
    separate wager with their own stake, and the host matches it: the loser of a wager owes the
    winner that wager's stake."""

    key = "bet"
    has_money = True
    wager_based = True

    HOST = "host"
    OPPONENT = "opponent"

    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        return {}

    def validate_terms(
        self, terms: Terms, config: dict[str, Any], *, host: bool, final: bool
    ) -> Terms:
        if host:  # the host matches each opponent's stake, so has no stake of their own
            return Terms(terms.user_id)
        note = terms.stake_note.strip()
        if not terms.stake_amount and not note:
            raise ValidationError("Podaj stawkę: kwotę albo opis stawki.")
        return Terms(terms.user_id, _clean_amount(terms), note)

    def validate_outcome(self, result: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
        winner = result.get("winner")
        if winner not in (self.HOST, self.OPPONENT, DRAW):
            raise ValidationError("Wynik musi wskazywać zwycięzcę: host, przeciwnik albo remis.")
        return {"winner": winner}

    def settle(
        self, parties: list[Party], result: dict[str, Any], config: dict[str, Any]
    ) -> Settlement:
        host = next(p for p in parties if p.is_host)
        opponent = next(p for p in parties if not p.is_host)
        winner = result["winner"]
        if winner == DRAW:
            return Settlement([], {host.user_id: DREW, opponent.user_id: DREW})
        won, lost = (host, opponent) if winner == self.HOST else (opponent, host)
        verdicts = {won.user_id: WON, lost.user_id: LOST}
        stake = opponent.stake_amount
        debts = [Debt(lost.user_id, won.user_id, stake)] if stake else []
        return Settlement(debts, verdicts)


class GroupBetKind(PactKind):
    """Everyone picks a side of a question; the winning side is the one that was right.

    With stakes it is a shared pot: the winning side splits the losing side's stakes in
    proportion to their own, so nobody loses more than they put in. Stakes are optional, so
    without any it is a plain prediction that just scores who was right."""

    key = "group_bet"
    has_money = True
    all_must_answer = True  # every stake changes everyone's payout

    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        return {"sides": _clean_sides(config)}

    def validate_terms(
        self, terms: Terms, config: dict[str, Any], *, host: bool, final: bool
    ) -> Terms:
        side = terms.side.strip()
        if (host or final or side) and side not in config["sides"]:
            raise ValidationError({"side": "Wybierz jedną ze stron zakładu."})
        return Terms(terms.user_id, _clean_amount(terms), terms.stake_note.strip(), side)

    def validate_outcome(self, result: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
        winner = result.get("winner")
        if winner != DRAW and winner not in config["sides"]:
            raise ValidationError("Wynik musi wskazywać jedną ze stron albo remis.")
        return {"winner": winner}

    def join_approvers(self, host_id: int, active_ids: list[int]) -> set[int]:
        return set(active_ids)

    def settle(
        self, parties: list[Party], result: dict[str, Any], config: dict[str, Any]
    ) -> Settlement:
        winner = result["winner"]
        if winner == DRAW:
            return Settlement([], {p.user_id: DREW for p in parties})
        verdicts = {p.user_id: WON if p.side == winner else LOST for p in parties}
        winners = [p for p in parties if p.side == winner and p.stake_amount]
        losers = [p for p in parties if p.side != winner and p.stake_amount]
        if not winners or not losers:
            return Settlement([], verdicts)
        weights = [w.stake_amount or 0 for w in winners]
        debts = []
        for loser in losers:
            for w, amount in zip(winners, _allocate(loser.stake_amount or 0, weights), strict=True):
                if amount:
                    debts.append(Debt(loser.user_id, w.user_id, amount))
        return Settlement(debts, verdicts)


class ResolutionKind(PactKind):
    """A personal resolution (postanowienie): the host commits to something and every other
    member judges, in a Sejmik vote, whether it was kept. No money, only pride, and nobody is
    invited: the whole house is the jury."""

    key = "resolution"
    needs_due_date = True
    joinable = False
    judged_by_everyone = True

    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        return {}

    def validate_terms(
        self, terms: Terms, config: dict[str, Any], *, host: bool, final: bool
    ) -> Terms:
        if terms.stake_amount or terms.side.strip():
            raise ValidationError("Postanowienia nie mają stawki pieniężnej ani stron.")
        return Terms(terms.user_id, None, terms.stake_note.strip())

    def validate_outcome(self, result: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
        kept = result.get("kept")
        if not isinstance(kept, bool):
            raise ValidationError(
                "Wynik musi mówić, czy postanowienie dotrzymano (kept: true/false)."
            )
        return {"kept": kept}

    def settle(
        self, parties: list[Party], result: dict[str, Any], config: dict[str, Any]
    ) -> Settlement:
        host = next(p for p in parties if p.is_host)
        return Settlement([], {host.user_id: WON if result["kept"] else LOST})


KINDS: dict[str, PactKind] = {}


def register(kind: PactKind) -> None:
    KINDS[kind.key] = kind


def get_kind(key: str) -> PactKind:
    try:
        return KINDS[key]
    except KeyError:
        raise ValidationError("Nieznany rodzaj zakładu.") from None


register(BetKind())
register(GroupBetKind())
register(ResolutionKind())
