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
    allows_open_join: ClassVar[bool] = False  # may be created open for anyone to ask to join
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
    allows_open_join = True
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


class _SidedKind(PactKind):
    """Participants each pick a side of a question; the winning side is the one that was right."""

    allows_open_join = True

    def validate_config(self, config: dict[str, Any]) -> dict[str, Any]:
        return {"sides": _clean_sides(config)}

    def validate_outcome(self, result: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
        winner = result.get("winner")
        if winner != DRAW and winner not in config["sides"]:
            raise ValidationError("Wynik musi wskazywać jedną ze stron albo remis.")
        return {"winner": winner}

    def _check_side(self, terms: Terms, config: dict[str, Any], *, required: bool) -> str:
        side = terms.side.strip()
        if not side and not required:
            return ""
        if side not in config["sides"]:
            raise ValidationError({"side": "Wybierz jedną ze stron zakładu."})
        return side

    def _verdicts(self, parties: list[Party], winner: str) -> dict[int, str]:
        if winner == DRAW:
            return {p.user_id: DREW for p in parties}
        return {p.user_id: WON if p.side == winner else LOST for p in parties}


class GroupBetKind(_SidedKind):
    """A shared pot: everyone picks a side and a stake. The winning side splits the losing side's
    stakes in proportion to their own, so nobody can lose more than they put in."""

    key = "group_bet"
    has_money = True
    all_must_answer = True  # every stake changes everyone's payout

    def validate_terms(
        self, terms: Terms, config: dict[str, Any], *, host: bool, final: bool
    ) -> Terms:
        committing = host or final
        side = self._check_side(terms, config, required=committing)
        if committing and not terms.stake_amount:
            raise ValidationError({"stake_amount": "Podaj stawkę."})
        return Terms(terms.user_id, _clean_amount(terms), terms.stake_note.strip(), side)

    def join_approvers(self, host_id: int, active_ids: list[int]) -> set[int]:
        return set(active_ids)

    def settle(
        self, parties: list[Party], result: dict[str, Any], config: dict[str, Any]
    ) -> Settlement:
        winner = result["winner"]
        verdicts = self._verdicts(parties, winner)
        if winner == DRAW:
            return Settlement([], verdicts)
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


class PredictionKind(_SidedKind):
    """A dated claim; everyone picks a side and the ones who were right score. No money."""

    key = "prediction"
    needs_due_date = True

    def validate_terms(
        self, terms: Terms, config: dict[str, Any], *, host: bool, final: bool
    ) -> Terms:
        if terms.stake_amount:
            raise ValidationError("Przewidywania nie mają stawki pieniężnej.")
        side = self._check_side(terms, config, required=host or final)
        return Terms(terms.user_id, None, terms.stake_note.strip(), side)

    def settle(
        self, parties: list[Party], result: dict[str, Any], config: dict[str, Any]
    ) -> Settlement:
        return Settlement([], self._verdicts(parties, result["winner"]))


class ResolutionKind(PactKind):
    """A personal resolution (postanowienie): the host commits to something and the others judge
    whether it was kept. No money, only pride."""

    key = "resolution"
    needs_due_date = True

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
register(PredictionKind())
register(ResolutionKind())
