"""Poll kinds: the single extension point for new voting types.

A kind decides what a valid config/ballot looks like, what a veto means, and how the final
result is computed. Everything else (participants, hiding votes until the end, closing,
reactions, notifications) is generic. To add a kind, subclass `PollKind` and call `register`.
"""

from abc import ABC, abstractmethod
from collections.abc import Sequence
from dataclasses import dataclass
from fractions import Fraction
from typing import TYPE_CHECKING, Any, ClassVar

from django.contrib.auth import get_user_model
from django.core.exceptions import ValidationError as DjangoValidationError
from django.core.files.base import ContentFile
from rest_framework import serializers

if TYPE_CHECKING:
    from voting.models import Poll


@dataclass(frozen=True)
class Entry:
    """One participant's cast ballot, as seen by `compute_result`."""

    ballot: dict[str, Any]
    vetoed: bool


class PollKind(ABC):
    key: ClassVar[str]
    allows_veto: ClassVar[bool] = True  # may participants veto?
    allows_early_close: ClassVar[bool] = True  # may the creator end it before everyone voted?
    everyone_participates: ClassVar[bool] = False  # ignore the chosen participants, take all
    accepts_image: ClassVar[bool] = False  # may the proposal carry an uploaded picture?

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

    def validate_proposal(  # noqa: B027 (optional hook)
        self, config: dict[str, Any], *, creator: Any, has_image: bool
    ) -> None:
        """Checks that need to know who is calling the vote. Raise a `ValidationError`."""

    def generate_title(self, config: dict[str, Any]) -> str | None:
        """A fixed title for kinds whose title follows from the config (else the caller's)."""
        return None

    def on_close(self, poll: Poll, result: dict[str, Any]) -> dict[str, Any]:
        """Runs once when the poll is resolved, in its transaction. Returned keys join `result`."""
        return {}


class ScoreKind(PollKind):
    """Integer score from -5 to +5; the result is the average."""

    key = "score"
    MIN = -5
    MAX = 5
    NEUTRAL_LIMIT = Fraction(1, 2)  # |average| <= 0.5 is neutral

    def validate_config(self, config: Any) -> dict[str, Any]:
        if config not in ({}, None):
            raise serializers.ValidationError("Głosowanie punktowe nie przyjmuje konfiguracji.")
        return {}

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        value = ballot.get("value") if isinstance(ballot, dict) else None
        if not isinstance(value, int) or isinstance(value, bool):
            raise serializers.ValidationError(
                "Głos musi mieć postać {'value': <liczba całkowita>}."
            )
        if not self.MIN <= value <= self.MAX:
            raise serializers.ValidationError(
                f"Ocena musi mieścić się w zakresie od {self.MIN} do {self.MAX}."
            )
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


def _username_error(user: Any, username: str) -> str | None:
    """Why `user` can't be called `username` (validators and uniqueness), or None."""
    user.username = username
    try:
        user.full_clean(exclude=[f.name for f in user._meta.fields if f.name != "username"])
    except DjangoValidationError as exc:
        return " ".join(exc.messages)
    return None


class ProfileChangeKind(ScoreKind):
    """Base for votes to change another member's profile: everyone votes, nobody can cut it short.

    Passes when the average score is at least 1; there is no veto. The change is applied in
    `on_close`, so it also happens when the poll expires.
    """

    allows_veto = False
    allows_early_close = False
    everyone_participates = True
    APPROVAL_SCORE = 1

    def _target(self, config: Any) -> Any:
        """The active member the vote is about, or a `ValidationError`."""
        target_id = config.get("target_user_id") if isinstance(config, dict) else None
        target = None
        if isinstance(target_id, int) and not isinstance(target_id, bool):
            target = get_user_model().objects.members().filter(pk=target_id).first()
        if target is None:
            raise serializers.ValidationError({"target_user_id": "Nieznany lub nieaktywny poseł."})
        return target

    def validate_proposal(self, config: dict[str, Any], *, creator: Any, has_image: bool) -> None:
        if config["target_user_id"] == creator.pk:
            raise serializers.ValidationError("Nie możesz głosować nad własnym profilem.")

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        result = super().compute_result(config, entries)
        # `sum >= n` is the exact form of `average >= 1`
        approved = bool(entries) and (
            sum(e.ballot["value"] for e in entries) >= self.APPROVAL_SCORE * len(entries)
        )
        result["approved"] = approved
        result["tone"] = "positive" if approved else "negative"
        return result

    def on_close(self, poll: Poll, result: dict[str, Any]) -> dict[str, Any]:
        if not result.get("approved"):
            return {"applied": False}
        target = get_user_model().objects.members().filter(pk=poll.config["target_user_id"]).first()
        if target is None:
            return {"applied": False, "apply_error": "Ten poseł już nie istnieje."}
        error = self.apply(poll, target)
        return {"applied": error is None} | ({"apply_error": error} if error else {})

    def apply(self, poll: Poll, target: Any) -> str | None:
        """Make the change; return a Polish reason if it can't be made any more."""
        raise NotImplementedError


class NicknameKind(ProfileChangeKind):
    """Change another member's username."""

    key = "nickname"

    def validate_config(self, config: Any) -> dict[str, Any]:
        target = self._target(config)
        new_username = get_user_model().normalize_username(str(config.get("new_username", "")))
        new_username = new_username.strip()
        current_username = target.username
        if new_username == current_username:
            raise serializers.ValidationError({"new_username": "To już jest ten nick."})
        error = _username_error(target, new_username)
        if error:
            raise serializers.ValidationError({"new_username": error})
        return {
            "target_user_id": target.pk,
            "target_username": current_username,
            "new_username": new_username,
        }

    def generate_title(self, config: dict[str, Any]) -> str:
        return f"Zmiana nicku: {config['target_username']} → {config['new_username']}"

    def apply(self, poll: Poll, target: Any) -> str | None:
        new_username = poll.config["new_username"]
        error = _username_error(target, new_username)
        if error:  # e.g. someone took the name while the vote was running
            return error
        target.save(update_fields=["username"])
        return None


class AvatarKind(ProfileChangeKind):
    """Change (or remove) another member's profile picture."""

    key = "avatar"
    accepts_image = True

    def validate_config(self, config: Any) -> dict[str, Any]:
        target = self._target(config)
        remove = config.get("remove", False)
        if not isinstance(remove, bool):
            raise serializers.ValidationError({"remove": "Wartość musi być prawdą lub fałszem."})
        return {"target_user_id": target.pk, "target_username": target.username, "remove": remove}

    def validate_proposal(self, config: dict[str, Any], *, creator: Any, has_image: bool) -> None:
        super().validate_proposal(config, creator=creator, has_image=has_image)
        if config["remove"] and has_image:
            raise serializers.ValidationError("Usuwanie zdjęcia nie przyjmuje nowego obrazu.")
        if not config["remove"] and not has_image:
            raise serializers.ValidationError("Dołącz zdjęcie albo zaproponuj jego usunięcie.")

    def generate_title(self, config: dict[str, Any]) -> str:
        action = "Usunięcie zdjęcia" if config["remove"] else "Zmiana zdjęcia"
        return f"{action}: {config['target_username']}"

    def apply(self, poll: Poll, target: Any) -> str | None:
        if poll.config["remove"]:
            target.clear_avatar()
        else:
            with poll.proposed_avatar_file.open("rb") as image:
                target.set_avatar(ContentFile(image.read()))
        return None

    def on_close(self, poll: Poll, result: dict[str, Any]) -> dict[str, Any]:
        try:
            return super().on_close(poll, result)
        finally:
            poll.discard_proposed_avatar()  # the pending picture is never needed after this


KINDS: dict[str, PollKind] = {}


def register(kind: PollKind) -> PollKind:
    KINDS[kind.key] = kind
    return kind


def get_kind(key: str) -> PollKind:
    try:
        return KINDS[key]
    except KeyError:
        raise serializers.ValidationError(f"Nieznany rodzaj głosowania: {key!r}.") from None


register(ScoreKind())
register(NicknameKind())
register(AvatarKind())
