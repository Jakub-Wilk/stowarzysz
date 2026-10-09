from rest_framework import serializers

from accounts.serializers import PersonSerializer
from ledger.models import LedgerEntry


class LedgerEntrySerializer(serializers.ModelSerializer):
    debtor = PersonSerializer(read_only=True)
    creditor = PersonSerializer(read_only=True)

    class Meta:
        model = LedgerEntry
        fields = (
            "id",
            "kind",
            "status",
            "debtor",
            "creditor",
            "amount",
            "currency",
            "description",
            "source_type",
            "source_id",
            "created_at",
            "decided_at",
        )
        read_only_fields = fields


class MemberBalanceSerializer(serializers.Serializer):
    """One person's total. `net` > 0: others owe them; `net` < 0: they owe others."""

    user = PersonSerializer()
    currency = serializers.CharField()
    net = serializers.IntegerField()


class PairBalanceSerializer(serializers.Serializer):
    """`debtor` owes `creditor` `amount` after netting the two people's debts. Minor units."""

    debtor = PersonSerializer()
    creditor = PersonSerializer()
    currency = serializers.CharField()
    amount = serializers.IntegerField()


class BalancesSerializer(serializers.Serializer):
    members = MemberBalanceSerializer(many=True)
    pairs = PairBalanceSerializer(many=True)


class PaymentCreateSerializer(serializers.Serializer):
    """Starting a payment: who was paid and how much (minor units)."""

    to_user_id = serializers.IntegerField()
    amount = serializers.IntegerField(min_value=1)
    note = serializers.CharField(max_length=200, required=False, allow_blank=True, default="")
