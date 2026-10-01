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
        fields = ("id", "deadline", "gift_tiers", "participants", "is_participant")
        read_only_fields = fields

    @extend_schema_field(PersonSerializer(many=True))
    def get_participants(self, obj: SantaEvent) -> Any:
        givers = [a.giver for a in obj.assignments.select_related("giver")]
        return PersonSerializer(givers, many=True).data

    def get_is_participant(self, obj: SantaEvent) -> bool:
        return obj.assignments.filter(giver=self.context["request"].user).exists()


class SantaStateSerializer(serializers.Serializer):
    """Serialises `{"event": SantaEvent | None}`. While an event is active the pairing is only
    ever revealed here, and only to the giver, so this is the one place that rule lives."""

    active = serializers.SerializerMethodField()
    event = serializers.SerializerMethodField()
    my_victim = serializers.SerializerMethodField()

    def get_active(self, obj: dict[str, Any]) -> bool:
        return obj["event"] is not None

    @extend_schema_field(ActiveEventSerializer(allow_null=True))
    def get_event(self, obj: dict[str, Any]) -> Any:
        if obj["event"] is None:
            return None
        return ActiveEventSerializer(obj["event"], context=self.context).data

    @extend_schema_field(PersonSerializer(allow_null=True))
    def get_my_victim(self, obj: dict[str, Any]) -> Any:
        if obj["event"] is None:
            return None
        victim = services.get_victim(obj["event"], self.context["request"].user)
        return None if victim is None else PersonSerializer(victim).data


class StartRequestSerializer(serializers.Serializer):
    participant_ids = serializers.ListField(child=serializers.IntegerField())
    deadline = serializers.DateTimeField()
    gift_tiers = serializers.ListField(child=serializers.IntegerField())


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
        fields = ("giver", "receiver", "gifts")
        read_only_fields = fields


class HistoryEventSerializer(serializers.ModelSerializer):
    pairings = PairingSerializer(source="assignments", many=True)

    class Meta:
        model = SantaEvent
        fields = ("id", "deadline", "ended_at", "gift_tiers", "pairings")
        read_only_fields = fields
