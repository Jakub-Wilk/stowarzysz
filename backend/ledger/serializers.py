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
            "debtor",
            "creditor",
            "amount",
            "currency",
            "description",
            "source_type",
            "source_id",
            "created_at",
            "paid_marked_at",
            "settled_at",
        )
        read_only_fields = fields


class BalanceSerializer(serializers.Serializer):
    """`amount` > 0: `user` owes me. `amount` < 0: I owe `user`. Minor units."""

    user = PersonSerializer()
    currency = serializers.CharField()
    amount = serializers.IntegerField()
