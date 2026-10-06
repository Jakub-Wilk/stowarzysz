from typing import Any

from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers

from accounts.serializers import PersonSerializer
from secretsanta import services
from secretsanta.models import SantaAssignment, SantaEvent, SantaGift


class ActiveEventSerializer(serializers.ModelSerializer):
    participants = serializers.SerializerMethodField()
    is_participant = serializers.SerializerMethodField()

    class Meta:
        model = SantaEvent
        fields = ("id", "mode", "deadline", "gift_tiers", "participants", "is_participant")
        read_only_fields = fields

    @extend_schema_field(PersonSerializer(many=True))
    def get_participants(self, obj: SantaEvent) -> Any:
        givers = {a.giver_id: a.giver for a in obj.assignments.select_related("giver")}.values()
        return PersonSerializer(givers, many=True).data

    def get_is_participant(self, obj: SantaEvent) -> bool:
        return obj.assignments.filter(giver=self.context["request"].user).exists()


class TierVictimSerializer(serializers.Serializer):
    amount = serializers.IntegerField()
    victim = PersonSerializer()


def _help_amount(event: SantaEvent, tier: int | None) -> int | None:
    return None if tier is None else services.clean_tiers_of(event)[tier]


class GiverHelpSerializer(serializers.Serializer):
    """A help request as its giver sees it."""

    victim_id = serializers.IntegerField()
    amount = serializers.IntegerField(allow_null=True)
    pending = serializers.BooleanField()
    ideas = serializers.ListField(child=serializers.CharField())


class ReceiverHelpSerializer(serializers.Serializer):
    """A help request as its victim sees it: which gift, never who asked."""

    id = serializers.IntegerField()
    amount = serializers.IntegerField(allow_null=True)
    pending = serializers.BooleanField()
    ideas = serializers.ListField(child=serializers.CharField())


class SantaStateSerializer(serializers.Serializer):
    """Serialises `{"event": SantaEvent | None}`. While an event is active the pairing is only
    ever revealed here, and only to the giver, so this is the one place that rule lives."""

    active = serializers.SerializerMethodField()
    event = serializers.SerializerMethodField()
    my_victim = serializers.SerializerMethodField()
    my_tier_victims = serializers.SerializerMethodField()
    my_help_requests = serializers.SerializerMethodField()
    help_requests_for_me = serializers.SerializerMethodField()

    def get_active(self, obj: dict[str, Any]) -> bool:
        return obj["event"] is not None

    @extend_schema_field(ActiveEventSerializer(allow_null=True))
    def get_event(self, obj: dict[str, Any]) -> Any:
        if obj["event"] is None:
            return None
        return ActiveEventSerializer(obj["event"], context=self.context).data

    @extend_schema_field(PersonSerializer(allow_null=True))
    def get_my_victim(self, obj: dict[str, Any]) -> Any:
        """The one victim who gets every tier (`single` mode)."""
        for tier, victim in self._victims(obj):
            if tier is None:
                return PersonSerializer(victim).data
        return None

    @extend_schema_field(TierVictimSerializer(many=True))
    def get_my_tier_victims(self, obj: dict[str, Any]) -> Any:
        """One victim per tier, highest amount first (`per_tier` mode)."""
        event = obj["event"]
        return [
            {"amount": event.gift_tiers[tier], "victim": PersonSerializer(victim).data}
            for tier, victim in self._victims(obj)
            if tier is not None
        ]

    @extend_schema_field(GiverHelpSerializer(many=True))
    def get_my_help_requests(self, obj: dict[str, Any]) -> Any:
        """Help the signed-in giver asked for, keyed by victim."""
        event = obj["event"]
        if event is None:
            return []
        user = self.context["request"].user
        return GiverHelpSerializer(
            [
                {
                    "victim_id": victim.pk,
                    "amount": _help_amount(event, r.tier_index),
                    "pending": r.pending,
                    "ideas": r.ideas,
                }
                for victim, r in services.help_for_giver(event, user)
            ],
            many=True,
        ).data

    @extend_schema_field(ReceiverHelpSerializer(many=True))
    def get_help_requests_for_me(self, obj: dict[str, Any]) -> Any:
        """Anonymous requests for ideas about the signed-in user."""
        event = obj["event"]
        if event is None:
            return []
        user = self.context["request"].user
        return ReceiverHelpSerializer(
            [
                {
                    "id": r.pk,
                    "amount": _help_amount(event, r.tier_index),
                    "pending": r.pending,
                    "ideas": r.ideas,
                }
                for r in services.help_for_receiver(event, user)
            ],
            many=True,
        ).data

    def _victims(self, obj: dict[str, Any]) -> list[tuple[int | None, Any]]:
        if obj["event"] is None:
            return []
        return services.get_victims(obj["event"], self.context["request"].user)


class StartRequestSerializer(serializers.Serializer):
    participant_ids = serializers.ListField(child=serializers.IntegerField())
    deadline = serializers.DateTimeField()
    gift_tiers = serializers.ListField(child=serializers.IntegerField())
    mode = serializers.ChoiceField(choices=SantaEvent.Mode.choices, default=SantaEvent.Mode.SINGLE)


class UpdateRequestSerializer(serializers.Serializer):
    deadline = serializers.DateTimeField(required=False)
    gift_tiers = serializers.ListField(child=serializers.IntegerField(), required=False)


class HelpRequestSerializer(serializers.Serializer):
    victim_id = serializers.IntegerField()


class HelpAnswerSerializer(serializers.Serializer):
    ideas = serializers.ListField(
        child=serializers.CharField(max_length=services.MAX_IDEA_LENGTH, allow_blank=True),
        max_length=services.MAX_IDEAS,
    )


class GiftSerializer(serializers.ModelSerializer):
    class Meta:
        model = SantaGift
        fields = ("id", "amount", "note")
        read_only_fields = ("id", "amount")


class GiftNoteRequestSerializer(serializers.Serializer):
    note = serializers.CharField(max_length=300, allow_blank=True, trim_whitespace=True)


class PairingSerializer(serializers.ModelSerializer):
    giver = PersonSerializer()
    receiver = PersonSerializer()
    gifts = GiftSerializer(many=True)

    class Meta:
        model = SantaAssignment
        fields = ("id", "giver", "receiver", "gifts")
        read_only_fields = fields


class HistoryEventSerializer(serializers.ModelSerializer):
    pairings = PairingSerializer(source="assignments", many=True)

    class Meta:
        model = SantaEvent
        fields = ("id", "mode", "deadline", "ended_at", "gift_tiers", "pairings")
        read_only_fields = fields
