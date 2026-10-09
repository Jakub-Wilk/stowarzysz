from typing import Any

from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers

from accounts.serializers import PersonSerializer
from pacts.kinds import KINDS
from pacts.models import OutcomeProposal, Pact, PactAttachment, PactParticipant
from pacts.services import ATTACH_STATES as _ATTACH
from pacts.services import MAX_ATTACHMENTS


class ParticipantSerializer(serializers.ModelSerializer):
    user = PersonSerializer(read_only=True)

    class Meta:
        model = PactParticipant
        fields = ("id", "user", "role", "side", "stake_amount", "stake_note", "state")
        read_only_fields = fields


class ProposalSerializer(serializers.ModelSerializer):
    proposed_by = PersonSerializer(read_only=True)
    wager_id = serializers.IntegerField(read_only=True, allow_null=True)
    ruling_poll_id = serializers.IntegerField(read_only=True, allow_null=True)
    confirmed_by = serializers.SerializerMethodField()

    class Meta:
        model = OutcomeProposal
        fields = (
            "id",
            "wager_id",
            "proposed_by",
            "result",
            "state",
            "created_at",
            "decided_at",
            "ruling_poll_id",
            "confirmed_by",
        )
        read_only_fields = fields

    def get_confirmed_by(self, proposal: OutcomeProposal) -> list[int]:
        return [c.user_id for c in proposal.confirmations.all()]


class AttachmentSerializer(serializers.ModelSerializer):
    uploaded_by = PersonSerializer(read_only=True)
    url = serializers.SerializerMethodField()

    class Meta:
        model = PactAttachment
        fields = ("id", "url", "caption", "uploaded_by", "created_at")
        read_only_fields = fields

    def get_url(self, attachment: PactAttachment) -> str:
        return attachment.image.url


class AttachmentUploadSerializer(serializers.Serializer):
    image = serializers.ImageField()
    caption = serializers.CharField(max_length=200, required=False, allow_blank=True, default="")


class MySerializer(serializers.Serializer):
    participant_id = serializers.IntegerField(allow_null=True)
    role = serializers.CharField(allow_null=True)
    state = serializers.CharField(allow_null=True)


class ActionsSerializer(serializers.Serializer):
    """What the caller can do right now, so the UI doesn't have to re-derive the rules."""

    can_respond = serializers.BooleanField()
    can_request_join = serializers.BooleanField()
    can_withdraw_request = serializers.BooleanField()
    can_attach = serializers.BooleanField()
    to_decide = serializers.ListField(child=serializers.IntegerField())  # participant ids
    to_confirm = serializers.ListField(child=serializers.IntegerField())  # proposal ids
    can_escalate = serializers.ListField(child=serializers.IntegerField())  # proposal ids


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
    actions = serializers.SerializerMethodField()
    attachments = AttachmentSerializer(many=True, read_only=True)

    class Meta(PactSerializer.Meta):
        fields = (
            *PactSerializer.Meta.fields,
            "notes",
            "config",
            "outcome",
            "participants",
            "proposals",
            "actions",
            "attachments",
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

    @extend_schema_field(ActionsSerializer)
    def get_actions(self, pact: Pact) -> dict[str, Any]:
        user = self.context["request"].user
        state = PactParticipant.State
        mine = next((p for p in pact.participants.all() if p.user_id == user.pk), None)
        running = pact.status in (Pact.Status.PROPOSED, Pact.Status.ACTIVE)
        retry = (state.DECLINED, state.REJECTED, state.WITHDRAWN, state.EXPIRED)
        to_decide = [
            p.pk
            for p in pact.participants.all()
            if p.state == state.REQUESTED
            and any(c.approver_id == user.pk and c.approved is None for c in p.consents.all())
        ]
        to_confirm: list[int] = []
        can_escalate: list[int] = []
        for proposal in pact.proposals.all():
            if proposal.state == OutcomeProposal.State.PENDING:
                already = {c.user_id for c in proposal.confirmations.all()}
                if (
                    proposal.proposed_by_id != user.pk
                    and _must_confirm(pact, proposal, user.pk)
                    and user.pk not in already
                ):
                    to_confirm.append(proposal.pk)
            elif (
                proposal.state == OutcomeProposal.State.DISPUTED
                and proposal.ruling_poll_id is None
                and mine is not None
                and mine.state == state.ACTIVE
            ):
                can_escalate.append(proposal.pk)
        return {
            "can_respond": running and mine is not None and mine.state == state.INVITED,
            "can_request_join": pact.is_open and running and (mine is None or mine.state in retry),
            "can_withdraw_request": mine is not None and mine.state == state.REQUESTED,
            "can_attach": mine is not None
            and mine.state in _ATTACH
            and len(pact.attachments.all()) < MAX_ATTACHMENTS,
            "to_decide": to_decide,
            "to_confirm": to_confirm,
            "can_escalate": can_escalate,
        }


def _must_confirm(pact: Pact, proposal: OutcomeProposal, user_id: int) -> bool:
    if proposal.wager is not None:
        return user_id in (pact.creator_id, proposal.wager.user_id)
    return any(
        p.user_id == user_id and p.state == PactParticipant.State.ACTIVE
        for p in pact.participants.all()
    )


class TermsSerializer(serializers.Serializer):
    stake_amount = serializers.IntegerField(min_value=0, required=False, allow_null=True)
    stake_note = serializers.CharField(max_length=200, required=False, allow_blank=True, default="")
    side = serializers.CharField(max_length=32, required=False, allow_blank=True, default="")


class InviteeSerializer(TermsSerializer):
    user_id = serializers.IntegerField()


class PactCreateSerializer(serializers.Serializer):
    kind = serializers.ChoiceField(choices=sorted(KINDS))
    title = serializers.CharField(max_length=200)
    condition = serializers.CharField()
    notes = serializers.CharField(required=False, allow_blank=True, default="")
    due_at = serializers.DateTimeField(required=False, allow_null=True)
    is_open = serializers.BooleanField(required=False, default=False)
    config = serializers.JSONField(required=False, default=dict)
    host = TermsSerializer(required=False)  # the creator's own side/stake (group bets, predictions)
    opponents = InviteeSerializer(many=True, required=False, default=list)


class RespondSerializer(TermsSerializer):
    accept = serializers.BooleanField()
    stake_note = serializers.CharField(
        max_length=200, required=False, allow_blank=True, allow_null=True
    )


class JoinDecisionSerializer(serializers.Serializer):
    approve = serializers.BooleanField()


class ProposeOutcomeSerializer(serializers.Serializer):
    wager_id = serializers.IntegerField(required=False, allow_null=True)
    result = serializers.JSONField()


class KindStatsSerializer(serializers.Serializer):
    won = serializers.IntegerField()
    lost = serializers.IntegerField()
    draw = serializers.IntegerField()


class PactStatsSerializer(serializers.Serializer):
    """One member's record across settled pacts. Money is in minor units, PLN only."""

    user = PersonSerializer()
    won = serializers.IntegerField()
    lost = serializers.IntegerField()
    draw = serializers.IntegerField()
    money_won = serializers.IntegerField()
    money_lost = serializers.IntegerField()
    by_kind = serializers.DictField(child=KindStatsSerializer())
