from typing import Any

from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers

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

    class Meta(PollSerializer.Meta):
        fields = (
            *PollSerializer.Meta.fields,
            "config",
            "close_reason",
            "participants",
            "my_ballot",
            "can_vote",
            "can_close",
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

    def get_can_close(self, poll: Poll) -> bool:
        return (
            poll.status == Poll.Status.OPEN and poll.creator_id == self.context["request"].user.pk
        )


class PollCreateSerializer(serializers.Serializer):
    title = serializers.CharField(max_length=200)
    kind = serializers.CharField(default="score")  # validated against the live registry
    config = serializers.JSONField(default=dict)
    participant_ids = serializers.ListField(
        child=serializers.IntegerField(), allow_empty=True, default=list
    )

    def validate_kind(self, value: str) -> str:
        get_kind(value)  # raises a ValidationError for unknown kinds
        return value


class BallotRequestSerializer(serializers.Serializer):
    ballot = serializers.JSONField()


class ReactionRequestSerializer(serializers.Serializer):
    emoji = serializers.ChoiceField(choices=REACTION_EMOJI)
