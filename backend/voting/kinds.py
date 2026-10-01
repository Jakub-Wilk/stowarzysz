"""Poll kinds: the single extension point for new voting types.

A kind decides what a valid config/ballot looks like, what a veto means, and how the final
result is computed. Everything else (participants, hiding votes until the end, closing,
reactions, notifications) is generic. To add a kind, subclass `PollKind` and call `register`.
"""

from abc import ABC, abstractmethod
from collections.abc import Sequence
from dataclasses import dataclass
from fractions import Fraction
from typing import Any, ClassVar

from rest_framework import serializers


@dataclass(frozen=True)
class Entry:
    """One participant's cast ballot, as seen by `compute_result`."""

    ballot: dict[str, Any]
    vetoed: bool


class PollKind(ABC):
    key: ClassVar[str]

    @abstractmethod
    def validate_config(self, config: Any) -> dict[str, Any]:
        """Return the cleaned config or raise a `serializers.ValidationError`."""

    @abstractmethod
    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        """Return the cleaned ballot or raise a `serializers.ValidationError`."""

    @abstractmethod
    def veto_ballot(self, config: dict[str, Any]) -> dict[str, Any]:
        """The ballot a veto counts as for this kind."""

    @abstractmethod
    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        """Final result from the ballots cast. Include `tone` (positive/neutral/negative) if any."""


class ScoreKind(PollKind):
    """Integer score from -5 to +5; the result is the average."""

    key = "score"
    MIN = -5
    MAX = 5
    NEUTRAL_LIMIT = Fraction(1, 2)  # |average| <= 0.5 is neutral

    def validate_config(self, config: Any) -> dict[str, Any]:
        if config not in ({}, None):
            raise serializers.ValidationError("A score vote takes no configuration.")
        return {}

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        value = ballot.get("value") if isinstance(ballot, dict) else None
        if not isinstance(value, int) or isinstance(value, bool):
            raise serializers.ValidationError("Ballot must be {'value': <integer>}.")
        if not self.MIN <= value <= self.MAX:
            raise serializers.ValidationError(f"Score must be between {self.MIN} and {self.MAX}.")
        return {"value": value}

    def veto_ballot(self, config: dict[str, Any]) -> dict[str, Any]:
        return {"value": self.MIN}

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        vetoes = sum(1 for e in entries if e.vetoed)
        result: dict[str, Any] = {
            "votes_cast": len(entries),
            "veto_count": vetoes,
            "vetoed": vetoes > 0,
            "score": None,
            "tone": None,
        }
        if not entries:
            return result
        mean = Fraction(sum(e.ballot["value"] for e in entries), len(entries))  # exact thresholds
        result["score"] = round(float(mean), 3)
        if mean > self.NEUTRAL_LIMIT:
            result["tone"] = "positive"
        elif mean < -self.NEUTRAL_LIMIT:
            result["tone"] = "negative"
        else:
            result["tone"] = "neutral"
        return result


KINDS: dict[str, PollKind] = {}


def register(kind: PollKind) -> PollKind:
    KINDS[kind.key] = kind
    return kind


def get_kind(key: str) -> PollKind:
    try:
        return KINDS[key]
    except KeyError:
        raise serializers.ValidationError(f"Unknown vote type: {key!r}.") from None


register(ScoreKind())
