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
from uuid import uuid4

from django.contrib.auth import get_user_model
from django.core.exceptions import ValidationError as DjangoValidationError
from django.core.files.base import ContentFile
from django.db import transaction
from rest_framework import serializers

from voting import tally

if TYPE_CHECKING:
    from voting.models import Poll


@dataclass(frozen=True)
class Entry:
    """One participant's cast ballot, as seen by `compute_result`."""

    ballot: dict[str, Any]
    vetoed: bool


@dataclass(frozen=True)
class ResolvedBallot:
    """A validated ballot, plus the config change casting it caused (an option added to an open
    pool), if any."""

    ballot: dict[str, Any]
    config: dict[str, Any] | None = None  # the poll's new config, when casting changed it
    added_option: dict[str, str] | None = None  # `{"id", "label"}` of an option created by it


class PollKind(ABC):
    key: ClassVar[str]
    allows_veto: ClassVar[bool] = True  # may participants veto?
    allows_abstain: ClassVar[bool] = True  # may participants abstain (and non-voters count so)?
    allows_early_close: ClassVar[bool] = True  # may the creator end it before everyone voted?
    everyone_participates: ClassVar[bool] = False  # ignore the chosen participants, take all
    accepts_image: ClassVar[bool] = False  # may the proposal carry an uploaded picture?

    @abstractmethod
    def validate_config(self, config: Any) -> dict[str, Any]:
        """Return the cleaned config or raise a `serializers.ValidationError`."""

    @abstractmethod
    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        """Return the cleaned ballot or raise a `serializers.ValidationError`."""

    def resolve_ballot(self, poll: Poll, ballot: Any) -> ResolvedBallot:
        """Validate a ballot being cast on `poll` (locked). Override when casting can change the
        poll itself; the default just calls `validate_ballot`."""
        return ResolvedBallot(self.validate_ballot(poll.config, ballot))

    @abstractmethod
    def veto_ballot(self, config: dict[str, Any]) -> dict[str, Any]:
        """The ballot a veto counts as for this kind."""

    @abstractmethod
    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        """Final result from the ballots cast. Include `tone` (positive/neutral/negative) if any."""

    def eligible_participants(self, config: dict[str, Any], creator: Any) -> list[Any] | None:
        """The exact voters, when the kind decides (the caller is then not added). None: the
        caller picks the participants."""
        return None

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
        if mean > 0:
            result["tone"] = "positive"
        elif mean < 0:
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

    Passes when the average score is positive; there is no veto. The change is applied in
    `on_close`, so it also happens when the poll expires.
    """

    allows_veto = False
    allows_abstain = False
    allows_early_close = False
    everyone_participates = True

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
        approved = bool(entries) and sum(e.ballot["value"] for e in entries) > 0
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
        if target.avatar:  # keep a copy: the live file is deleted when it is replaced
            with target.avatar_file.open("rb") as old:
                poll.previous_avatar_file.save(
                    f"{uuid4().hex}.webp", ContentFile(old.read()), save=False
                )
        if poll.config["remove"]:
            target.clear_avatar()
        else:
            with poll.proposed_avatar_file.open("rb") as image:
                target.set_avatar(ContentFile(image.read()))
        return None


class PactRulingKind(PollKind):
    """Members outside a pact decide whether a disputed claim about it stands.

    Ballot `{"upheld": bool}`. Only a strict majority of the votes cast upholds the claim; on a
    tie or with no votes it is overruled and the parties can claim again. The outcome is applied
    by `pacts.services.apply_ruling` in `on_close`.
    """

    key = "pact_ruling"
    allows_veto = False
    allows_early_close = False

    def _proposal(self, config: Any) -> Any:
        from pacts.models import OutcomeProposal

        proposal_id = config.get("proposal_id") if isinstance(config, dict) else None
        proposal = None
        if isinstance(proposal_id, int) and not isinstance(proposal_id, bool):
            proposal = OutcomeProposal.objects.select_related("pact").filter(pk=proposal_id).first()
        if proposal is None:
            raise serializers.ValidationError({"proposal_id": "Nie ma takiego wyniku."})
        return proposal

    def validate_config(self, config: Any) -> dict[str, Any]:
        proposal = self._proposal(config)
        return {
            "proposal_id": proposal.pk,
            "pact_id": proposal.pact_id,
            "pact_title": proposal.pact.title,
            "claim": proposal.result,
        }

    def generate_title(self, config: dict[str, Any]) -> str:
        return f"Spór o umowę: {config['pact_title']}"

    def eligible_participants(self, config: dict[str, Any], creator: Any) -> list[Any]:
        from pacts.models import PactParticipant

        in_pact = PactParticipant.objects.filter(pact_id=config["pact_id"]).values("user_id")
        return list(get_user_model().objects.members().exclude(pk__in=in_pact))

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        upheld = ballot.get("upheld") if isinstance(ballot, dict) else None
        if not isinstance(upheld, bool):
            raise serializers.ValidationError("Głos musi mieć postać {'upheld': true/false}.")
        return {"upheld": upheld}

    def veto_ballot(self, config: dict[str, Any]) -> dict[str, Any]:
        return {"upheld": False}

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        upheld = sum(1 for e in entries if e.ballot["upheld"])
        rejected = len(entries) - upheld
        approved = upheld > rejected
        return {
            "votes_cast": len(entries),
            "upheld": upheld,
            "rejected": rejected,
            "approved": approved,
            "tone": "positive" if approved else "negative",
        }

    def on_close(self, poll: Poll, result: dict[str, Any]) -> dict[str, Any]:
        from pacts import services

        try:
            with transaction.atomic():  # a failure here must not poison the poll's own close
                return services.apply_ruling(poll.config["proposal_id"], upheld=result["approved"])
        except Exception:
            return {"applied": False, "apply_error": "Nie udało się zastosować rozstrzygnięcia."}


class ResolutionJudgmentKind(PollKind):
    """Every member except the author judges whether a resolution (postanowienie) was kept.

    Ballot `{"kept": bool}`. A strict majority of the votes cast decides; a tie or no votes
    decides nothing and the resolution stays open. Applied by `pacts.services.apply_judgment`.
    """

    key = "resolution_judgment"
    allows_veto = False
    allows_early_close = False

    def _pact(self, config: Any) -> Any:
        from pacts.models import Pact

        pact_id = config.get("pact_id") if isinstance(config, dict) else None
        pact = None
        if isinstance(pact_id, int) and not isinstance(pact_id, bool):
            pact = Pact.objects.filter(pk=pact_id, kind="resolution").first()
        if pact is None:
            raise serializers.ValidationError({"pact_id": "Nie ma takiego postanowienia."})
        return pact

    def validate_config(self, config: Any) -> dict[str, Any]:
        pact = self._pact(config)
        return {
            "pact_id": pact.pk,
            "pact_title": pact.title,
            "condition": pact.condition,
            "author_id": pact.creator_id,
            "author_username": pact.creator.username,
        }

    def generate_title(self, config: dict[str, Any]) -> str:
        return f"Ocena postanowienia: {config['pact_title']}"

    def eligible_participants(self, config: dict[str, Any], creator: Any) -> list[Any]:
        return list(get_user_model().objects.members().exclude(pk=config["author_id"]))

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        kept = ballot.get("kept") if isinstance(ballot, dict) else None
        if not isinstance(kept, bool):
            raise serializers.ValidationError("Głos musi mieć postać {'kept': true/false}.")
        return {"kept": kept}

    def veto_ballot(self, config: dict[str, Any]) -> dict[str, Any]:
        return {"kept": False}

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        kept = sum(1 for e in entries if e.ballot["kept"])
        broken = len(entries) - kept
        decided = kept != broken
        return {
            "votes_cast": len(entries),
            "kept": kept,
            "broken": broken,
            "decided": decided,
            "approved": kept > broken,  # "kept their word"
            "tone": ("positive" if kept > broken else "negative") if decided else "neutral",
        }

    def on_close(self, poll: Poll, result: dict[str, Any]) -> dict[str, Any]:
        from pacts import services

        try:
            with transaction.atomic():  # a failure here must not poison the poll's own close
                return services.apply_judgment(
                    poll.config["pact_id"], kept=result["approved"], decided=result["decided"]
                )
        except Exception:
            return {"applied": False, "apply_error": "Nie udało się zastosować oceny."}


MIN_OPTIONS = 2
MAX_OPTIONS = 10  # when the poll is created
MAX_OPTIONS_OPEN = 20  # once participants have added more to an open pool
MAX_LABEL_LENGTH = 60


def _clean_label(value: Any, field: str) -> str:
    if not isinstance(value, str):
        raise serializers.ValidationError({field: "Opcja musi być tekstem."})
    label = " ".join(value.split())
    if not label or len(label) > MAX_LABEL_LENGTH:
        raise serializers.ValidationError(
            {field: f"Opcja musi mieć od 1 do {MAX_LABEL_LENGTH} znaków."}
        )
    return label


class OptionsKind(PollKind):
    """Base for votes over options the creator defines (single, multi, ranked).

    Config `{"options": [{"id", "label"}, ...]}`; the client sends plain labels and the ids are
    assigned here (`o1`, `o2`, ...). With `open_options` (single and multi only) a participant
    can add one new option with each vote: the ballot request then carries `new_option`, which
    `resolve_ballot` turns into an id-only ballot and a longer option list. The ballots stored
    and the result only ever mention option ids. Every result has `options` (`id`, `label`,
    `count`), `winners` (ids), `tie` and `tone`.
    """

    allows_veto = False
    ALLOWED_CONFIG: ClassVar[frozenset[str]] = frozenset({"options"})

    def validate_config(self, config: Any) -> dict[str, Any]:
        if not isinstance(config, dict):
            raise serializers.ValidationError("Konfiguracja musi być obiektem.")
        for key in config.keys() - self.ALLOWED_CONFIG:
            message = (
                "Ten rodzaj głosowania nie pozwala uczestnikom dodawać opcji."
                if key == "open_options"
                else f"Nieznane ustawienie: {key}."
            )
            raise serializers.ValidationError({key: message})
        raw = config.get("options")
        if not isinstance(raw, list) or not MIN_OPTIONS <= len(raw) <= MAX_OPTIONS:
            raise serializers.ValidationError(
                {"options": f"Podaj od {MIN_OPTIONS} do {MAX_OPTIONS} opcji."}
            )
        labels = [_clean_label(value, "options") for value in raw]
        if len({label.casefold() for label in labels}) != len(labels):
            raise serializers.ValidationError({"options": "Opcje muszą się różnić."})
        cleaned: dict[str, Any] = {
            "options": [{"id": f"o{n}", "label": label} for n, label in enumerate(labels, 1)]
        }
        open_options = config.get("open_options", False)
        if not isinstance(open_options, bool):
            raise serializers.ValidationError(
                {"open_options": "Wartość musi być prawdą lub fałszem."}
            )
        if open_options:
            cleaned["open_options"] = True
        return cleaned | self.clean_extra_config(config, len(labels))

    def clean_extra_config(self, config: dict[str, Any], option_count: int) -> dict[str, Any]:
        """Kind-specific settings beyond the options (already limited to `ALLOWED_CONFIG`)."""
        return {}

    @staticmethod
    def option_ids(config: dict[str, Any]) -> list[str]:
        return [option["id"] for option in config["options"]]

    def resolve_ballot(self, poll: Poll, ballot: Any) -> ResolvedBallot:
        if not isinstance(ballot, dict):
            raise serializers.ValidationError("Nieprawidłowy głos.")
        if "new_option" not in ballot:
            return super().resolve_ballot(poll, ballot)
        config = poll.config
        if not config.get("open_options"):
            raise serializers.ValidationError("W tym głosowaniu nie można dodawać opcji.")
        label = _clean_label(ballot["new_option"], "new_option")
        options = config["options"]
        existing = next((o for o in options if o["label"].casefold() == label.casefold()), None)
        new_config: dict[str, Any] | None = None
        added: dict[str, str] | None = None
        if existing is not None:  # already there: the vote simply goes to it
            option_id = existing["id"]
        else:
            if len(options) >= MAX_OPTIONS_OPEN:
                raise serializers.ValidationError(
                    {"new_option": f"Osiągnięto limit {MAX_OPTIONS_OPEN} opcji."}
                )
            added = {"id": f"o{max(int(o['id'][1:]) for o in options) + 1}", "label": label}
            option_id = added["id"]
            new_config = {**config, "options": [*options, added]}
        rest = {key: value for key, value in ballot.items() if key != "new_option"}
        cleaned = self.validate_ballot(new_config or config, self.with_option(rest, option_id))
        return ResolvedBallot(cleaned, new_config, added)

    def with_option(self, ballot: dict[str, Any], option_id: str) -> dict[str, Any]:
        """`ballot` (without `new_option`) extended to include the option just added."""
        raise serializers.ValidationError("W tym głosowaniu nie można dodawać opcji.")

    def veto_ballot(self, config: dict[str, Any]) -> dict[str, Any]:
        return {}  # never used: there is no veto

    def picks_result(
        self, config: dict[str, Any], votes_cast: int, counts: dict[str, int]
    ) -> dict[str, Any]:
        ids = self.option_ids(config)
        order = {option_id: i for i, option_id in enumerate(ids)}
        rows = sorted(
            (
                {"id": o["id"], "label": o["label"], "count": counts[o["id"]]}
                for o in config["options"]
            ),
            key=lambda row: (-row["count"], order[row["id"]]),
        )
        winners = tally.top_ids(ids, counts)
        return {
            "votes_cast": votes_cast,
            "options": rows,
            "winners": winners,
            "tie": len(winners) > 1,
            "tone": "positive" if len(winners) == 1 else "neutral",
        }


class SingleChoiceKind(OptionsKind):
    """Everyone picks one option; the most picked wins. Ballot `{"choice": "o2"}`."""

    key = "single_choice"
    ALLOWED_CONFIG = frozenset({"options", "open_options"})

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        choice = ballot.get("choice") if isinstance(ballot, dict) else None
        if not isinstance(choice, str) or choice not in self.option_ids(config):
            raise serializers.ValidationError("Wybierz jedną z opcji.")
        return {"choice": choice}

    def with_option(self, ballot: dict[str, Any], option_id: str) -> dict[str, Any]:
        if ballot.get("choice") is not None:
            raise serializers.ValidationError(
                "Wybierz istniejącą opcję albo dodaj nową, nie obie naraz."
            )
        return {"choice": option_id}

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        counts = tally.count_picks(self.option_ids(config), ([e.ballot["choice"]] for e in entries))
        return self.picks_result(config, len(entries), counts)


class MultiChoiceKind(OptionsKind):
    """Everyone ticks the options they approve of (optionally at most `max_choices`); the most
    approved win. Ballot `{"choices": ["o1", "o3"]}`."""

    key = "multi_choice"
    ALLOWED_CONFIG = frozenset({"options", "open_options", "max_choices"})

    def clean_extra_config(self, config: dict[str, Any], option_count: int) -> dict[str, Any]:
        if "max_choices" not in config:
            return {}
        limit = config["max_choices"]
        if (
            not isinstance(limit, int)
            or isinstance(limit, bool)
            or not 1 <= limit <= option_count - 1
        ):
            raise serializers.ValidationError(
                {"max_choices": f"Limit wyborów musi być liczbą od 1 do {option_count - 1}."}
            )
        return {"max_choices": limit}

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        choices = ballot.get("choices") if isinstance(ballot, dict) else None
        ids = self.option_ids(config)
        if (
            not isinstance(choices, list)
            or not choices
            or not all(isinstance(c, str) and c in ids for c in choices)
            or len(set(choices)) != len(choices)
        ):
            raise serializers.ValidationError("Wybierz co najmniej jedną z opcji, każdą raz.")
        limit = config.get("max_choices")
        if limit is not None and len(choices) > limit:
            raise serializers.ValidationError(f"Możesz wybrać najwyżej {limit} opcji.")
        return {"choices": [i for i in ids if i in choices]}

    def with_option(self, ballot: dict[str, Any], option_id: str) -> dict[str, Any]:
        choices = ballot.get("choices", [])
        if not isinstance(choices, list):
            raise serializers.ValidationError("Głos musi mieć postać {'choices': [...]}.")
        return {"choices": choices if option_id in choices else [*choices, option_id]}

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        counts = tally.count_picks(self.option_ids(config), (e.ballot["choices"] for e in entries))
        return self.picks_result(config, len(entries), counts)


class RankedChoiceKind(OptionsKind):
    """Everyone ranks the options, best first; counted with the Schulze method. Ballot
    `{"ranking": ["o3", "o1", ...]}` (may leave options out: they count as tied for last)."""

    key = "ranked_choice"

    def validate_ballot(self, config: dict[str, Any], ballot: Any) -> dict[str, Any]:
        ranking = ballot.get("ranking") if isinstance(ballot, dict) else None
        ids = self.option_ids(config)
        if (
            not isinstance(ranking, list)
            or not ranking
            or not all(isinstance(r, str) and r in ids for r in ranking)
            or len(set(ranking)) != len(ranking)
        ):
            raise serializers.ValidationError("Uszereguj opcje, każdą najwyżej raz.")
        return {"ranking": ranking}

    def compute_result(self, config: dict[str, Any], entries: Sequence[Entry]) -> dict[str, Any]:
        ids = self.option_ids(config)
        rankings = [e.ballot["ranking"] for e in entries]
        outcome = tally.schulze(ids, rankings)
        first_choices = tally.count_picks(ids, ([r[0]] for r in rankings))
        order = {option_id: i for i, option_id in enumerate(ids)}
        rows = sorted(
            (
                {
                    "id": o["id"],
                    "label": o["label"],
                    "count": first_choices[o["id"]],
                    "wins": outcome.wins[o["id"]],
                }
                for o in config["options"]
            ),
            key=lambda row: (-row["wins"], order[row["id"]]),
        )
        return {
            "votes_cast": len(entries),
            "options": rows,
            "winners": outcome.winners,
            "tie": len(outcome.winners) > 1,
            "tone": "positive" if len(outcome.winners) == 1 else "neutral",
            "pairwise": outcome.pairwise,
            "condorcet_winner": outcome.condorcet_winner,
        }


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
register(PactRulingKind())
register(ResolutionJudgmentKind())
register(SingleChoiceKind())
register(MultiChoiceKind())
register(RankedChoiceKind())
