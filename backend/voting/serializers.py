from typing import Any

from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers

from accounts.avatars import process_avatar
from accounts.serializers import PersonSerializer as UserBriefSerializer
from voting.kinds import get_kind
from voting.models import Poll, PollParticipant
from voting.services import REACTION_EMOJI


class MySerializer(serializers.Serializer):
    participating = serializers.BooleanField()
    has_voted = serializers.BooleanField()
    vetoed = serializers.BooleanField()


class ParticipantSerializer(serializers.Serializer):
    """`ballot`/`vetoed` are only present once the poll has ended."""

    user = UserBriefSerializer()
    has_voted = serializers.BooleanField()
    ballot = serializers.JSONField(required=False, allow_null=True)
    vetoed = serializers.BooleanField(required=False)


class PollSerializer(serializers.ModelSerializer):
    """List item. Never contains anyone's ballot; own state comes from `my`."""

    creator = UserBriefSerializer(read_only=True)
    participant_count = serializers.SerializerMethodField()
    voted_count = serializers.SerializerMethodField()
    my = serializers.SerializerMethodField()

    class Meta:
        model = Poll
        fields = (
            "id",
            "title",
            "kind",
            "status",
            "creator",
            "created_at",
            "closed_at",
            "participant_count",
            "voted_count",
            "my",
            "result",
        )
        read_only_fields = fields

    def _participants(self, poll: Poll) -> list[PollParticipant]:
        return list(poll.participants.all())  # prefetched by the views

    def _mine(self, poll: Poll) -> PollParticipant | None:
        user = self.context["request"].user
        return next((p for p in self._participants(poll) if p.user_id == user.pk), None)

    def get_participant_count(self, poll: Poll) -> int:
        return len(self._participants(poll))

    def get_voted_count(self, poll: Poll) -> int:
        return sum(1 for p in self._participants(poll) if p.has_ballot)

    @extend_schema_field(MySerializer)
    def get_my(self, poll: Poll) -> dict[str, bool]:
        mine = self._mine(poll)
        return {
            "participating": mine is not None,
            "has_voted": bool(mine and mine.has_ballot),
            "vetoed": bool(mine and mine.vetoed),
        }


class PollDetailSerializer(PollSerializer):
    """Detail view. The single place that decides what is hidden while a poll is open."""

    participants = serializers.SerializerMethodField()
    my_ballot = serializers.SerializerMethodField()
    can_vote = serializers.SerializerMethodField()
    can_close = serializers.SerializerMethodField()
    can_veto = serializers.SerializerMethodField()
    proposed_avatar_url = serializers.SerializerMethodField()
    previous_avatar_url = serializers.SerializerMethodField()

    class Meta(PollSerializer.Meta):
        fields = (
            *PollSerializer.Meta.fields,
            "config",
            "close_reason",
            "participants",
            "my_ballot",
            "can_vote",
            "can_close",
            "can_veto",
            "proposed_avatar_url",
            "previous_avatar_url",
        )
        read_only_fields = fields

    @extend_schema_field(ParticipantSerializer(many=True))
    def get_participants(self, poll: Poll) -> list[dict[str, Any]]:
        revealed = poll.status == Poll.Status.CLOSED
        rows: list[dict[str, Any]] = []
        for p in sorted(self._participants(poll), key=lambda p: p.user.username):
            row: dict[str, Any] = {
                "user": UserBriefSerializer(p.user).data,
                "has_voted": p.has_ballot,
            }
            if revealed:  # hidden until the end, for everyone
                row["ballot"] = p.ballot
                row["vetoed"] = p.vetoed
            rows.append(row)
        return rows

    @extend_schema_field(serializers.JSONField(allow_null=True))
    def get_my_ballot(self, poll: Poll) -> Any:
        mine = self._mine(poll)
        return mine.ballot if mine else None  # your own vote is always yours to see

    def get_can_vote(self, poll: Poll) -> bool:
        return poll.status == Poll.Status.OPEN and self._mine(poll) is not None

    def get_can_veto(self, poll: Poll) -> bool:
        return self.get_can_vote(poll) and get_kind(poll.kind).allows_veto

    def get_can_close(self, poll: Poll) -> bool:
        return (
            poll.status == Poll.Status.OPEN
            and poll.creator_id == self.context["request"].user.pk
            and get_kind(poll.kind).allows_early_close
        )

    def get_proposed_avatar_url(self, poll: Poll) -> str | None:
        return poll.proposed_avatar.url if poll.proposed_avatar else None

    def get_previous_avatar_url(self, poll: Poll) -> str | None:
        return poll.previous_avatar.url if poll.previous_avatar else None


class PollCreateSerializer(serializers.Serializer):
    """JSON, or multipart (`config` as a JSON string plus `image`) for profile-picture votes."""

    title = serializers.CharField(max_length=200, required=False, allow_blank=True, default="")
    kind = serializers.CharField(default="score")  # validated against the live registry
    config = serializers.JSONField(default=dict)
    participant_ids = serializers.ListField(
        child=serializers.IntegerField(), allow_empty=True, default=list
    )

    image = serializers.ImageField(required=False, write_only=True)

    def validate_image(self, value: Any) -> Any:
        return process_avatar(value)  # the validated value is the normalised image, ready to store

    def validate_kind(self, value: str) -> str:
        get_kind(value)  # raises a ValidationError for unknown kinds
        return value


class BallotRequestSerializer(serializers.Serializer):
    ballot = serializers.JSONField()


class ReactionRequestSerializer(serializers.Serializer):
    emoji = serializers.ChoiceField(choices=REACTION_EMOJI)


class ArchivePicturesSerializer(serializers.Serializer):
    """TEMPORARY: restore the pictures of avatar votes closed before they were kept."""

    previous = serializers.ImageField(write_only=True, required=False)
    proposed = serializers.ImageField(write_only=True, required=False)

    def validate_previous(self, value: Any) -> Any:
        return process_avatar(value)

    def validate_proposed(self, value: Any) -> Any:
        return process_avatar(value)
