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


class SantaStateSerializer(serializers.Serializer):
    """Serialises `{"event": SantaEvent | None}`. While an event is active the pairing is only
    ever revealed here, and only to the giver, so this is the one place that rule lives."""

    active = serializers.SerializerMethodField()
    event = serializers.SerializerMethodField()
    my_victim = serializers.SerializerMethodField()
    my_tier_victims = serializers.SerializerMethodField()

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
