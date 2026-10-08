from typing import Any

from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers

from accounts.serializers import PersonSerializer
from pacts.kinds import KINDS
from pacts.models import OutcomeProposal, Pact, PactParticipant


class ParticipantSerializer(serializers.ModelSerializer):
    user = PersonSerializer(read_only=True)

    class Meta:
        model = PactParticipant
        fields = ("id", "user", "role", "side", "stake_amount", "stake_note", "state")
        read_only_fields = fields


class ProposalSerializer(serializers.ModelSerializer):
    proposed_by = PersonSerializer(read_only=True)
    wager_id = serializers.IntegerField(read_only=True)

    class Meta:
        model = OutcomeProposal
        fields = ("id", "wager_id", "proposed_by", "result", "state", "created_at", "decided_at")
        read_only_fields = fields


class MySerializer(serializers.Serializer):
    participant_id = serializers.IntegerField(allow_null=True)
    role = serializers.CharField(allow_null=True)
    state = serializers.CharField(allow_null=True)


class PactSerializer(serializers.ModelSerializer):
    """List item."""

    creator = PersonSerializer(read_only=True)
    active_count = serializers.SerializerMethodField()
    my = serializers.SerializerMethodField()

    class Meta:
        model = Pact
        fields = (
            "id",
            "kind",
            "title",
            "condition",
            "due_at",
            "is_open",
            "status",
            "creator",
            "created_at",
            "resolved_at",
            "active_count",
            "my",
        )
        read_only_fields = fields

    def get_active_count(self, pact: Pact) -> int:
        return sum(1 for p in pact.participants.all() if p.state == PactParticipant.State.ACTIVE)

    @extend_schema_field(MySerializer)
    def get_my(self, pact: Pact) -> dict[str, Any]:
        user = self.context["request"].user
        mine = next((p for p in pact.participants.all() if p.user_id == user.pk), None)
        return {
            "participant_id": mine.pk if mine else None,
            "role": mine.role if mine else None,
            "state": mine.state if mine else None,
        }


class PactDetailSerializer(PactSerializer):
    participants = ParticipantSerializer(many=True, read_only=True)
    proposals = serializers.SerializerMethodField()

    class Meta(PactSerializer.Meta):
        fields = (
            *PactSerializer.Meta.fields,
            "notes",
            "config",
            "outcome",
            "participants",
            "proposals",
        )
        read_only_fields = fields

    @extend_schema_field(ProposalSerializer(many=True))
    def get_proposals(self, pact: Pact) -> list[dict[str, Any]]:
        """Open and disputed claims only; superseded and confirmed ones are history."""
        live = [
            p
            for p in pact.proposals.all()
            if p.state in (OutcomeProposal.State.PENDING, OutcomeProposal.State.DISPUTED)
        ]
        return ProposalSerializer(live, many=True, context=self.context).data


class InviteeSerializer(serializers.Serializer):
    user_id = serializers.IntegerField()
    stake_amount = serializers.IntegerField(min_value=0, required=False, allow_null=True)
    stake_note = serializers.CharField(max_length=200, required=False, allow_blank=True, default="")


class PactCreateSerializer(serializers.Serializer):
    kind = serializers.ChoiceField(choices=sorted(KINDS))
    title = serializers.CharField(max_length=200)
    condition = serializers.CharField()
    notes = serializers.CharField(required=False, allow_blank=True, default="")
    due_at = serializers.DateTimeField(required=False, allow_null=True)
    is_open = serializers.BooleanField(required=False, default=False)
    config = serializers.JSONField(required=False, default=dict)
    opponents = InviteeSerializer(many=True, required=False, default=list)


class RespondSerializer(serializers.Serializer):
    accept = serializers.BooleanField()


class ProposeOutcomeSerializer(serializers.Serializer):
    wager_id = serializers.IntegerField()
    result = serializers.JSONField()
