from decimal import Decimal
from typing import Any

from django.contrib.auth import get_user_model
from django.utils import timezone
from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers

from accounts.serializers import PersonSerializer
from ledger.kinds import ExpenseKind, get_kind
from ledger.models import EntryAttachment, LedgerEntry, Obligation
from ledger.money import BASE_CURRENCY, CURRENCIES

# --- requests ------------------------------------------------------------------------------


class _EntryInput(serializers.Serializer):
    occurred_on = serializers.DateField(default=timezone.localdate)
    note = serializers.CharField(max_length=500, required=False, allow_blank=True, default="")

    def validate_occurred_on(self, value: Any) -> Any:
        if value > timezone.localdate():
            raise serializers.ValidationError("Data nie może być z przyszłości.")
        return value


class PayerSerializer(serializers.Serializer):
    user_id = serializers.IntegerField()
    amount = serializers.IntegerField(min_value=1, help_text="Minor units of the currency.")


class ShareSerializer(serializers.Serializer):
    user_id = serializers.IntegerField()
    weight = serializers.IntegerField(
        min_value=1, default=1, help_text="Ignored for `equal`; the amount for `exact`."
    )


class ItemSerializer(serializers.Serializer):
    name = serializers.CharField(max_length=100)
    amount = serializers.IntegerField(min_value=1)
    split = serializers.ChoiceField(choices=ExpenseKind.SPLITS, default="equal")
    shares = ShareSerializer(many=True, allow_empty=False)


class ExpenseInputSerializer(_EntryInput):
    """Somebody paid for something the others shared. A plain expense is a single item."""

    kind = serializers.ChoiceField(choices=["expense"])
    title = serializers.CharField(max_length=200)
    currency = serializers.ChoiceField(choices=list(CURRENCIES), default=BASE_CURRENCY)
    category = serializers.ChoiceField(
        choices=LedgerEntry.Category.choices, default=LedgerEntry.Category.OTHER
    )
    payers = PayerSerializer(many=True, allow_empty=False)
    items = ItemSerializer(many=True, allow_empty=False, max_length=200)


class DebtInputSerializer(_EntryInput):
    """Goods somebody owes you ("2 x kawa"); it counts at once."""

    kind = serializers.ChoiceField(choices=["debt"])
    debtor_id = serializers.IntegerField()
    item = serializers.CharField(max_length=60)
    amount = serializers.IntegerField(min_value=1, max_value=1000, default=1)


class PaymentInputSerializer(_EntryInput):
    """You paid somebody back; it counts once they confirm."""

    kind = serializers.ChoiceField(choices=["payment"])
    to_user_id = serializers.IntegerField()
    amount = serializers.IntegerField(min_value=1, help_text="Grosze, or the quantity of `item`.")
    item = serializers.CharField(max_length=60, required=False, allow_blank=True, default="")


INPUTS: dict[str, type[_EntryInput]] = {
    "expense": ExpenseInputSerializer,
    "debt": DebtInputSerializer,
    "payment": PaymentInputSerializer,
}


class ExpenseUpdateSerializer(ExpenseInputSerializer):
    version = serializers.IntegerField(help_text="The version being edited (409 if outdated).")


UPDATES: dict[str, type[_EntryInput]] = {"expense": ExpenseUpdateSerializer}  # editable kinds


class EntryAttachmentUploadSerializer(serializers.Serializer):
    image = serializers.ImageField()


class RateQuerySerializer(serializers.Serializer):
    currency = serializers.ChoiceField(choices=list(CURRENCIES))
    date = serializers.DateField(default=timezone.localdate)


# --- responses -----------------------------------------------------------------------------


class ObligationSerializer(serializers.ModelSerializer):
    """`debtor` owes `creditor` `amount` (base minor units, or a quantity of `item`)."""

    debtor = PersonSerializer(read_only=True)
    creditor = PersonSerializer(read_only=True)

    class Meta:
        model = Obligation
        fields = ("debtor", "creditor", "amount", "item")
        read_only_fields = fields


class EntryAttachmentSerializer(serializers.ModelSerializer):
    uploaded_by = PersonSerializer(read_only=True)
    url = serializers.SerializerMethodField()

    class Meta:
        model = EntryAttachment
        fields = ("id", "url", "uploaded_by", "created_at")
        read_only_fields = fields

    def get_url(self, attachment: EntryAttachment) -> str:
        return attachment.image.url


class ShareRowSerializer(serializers.Serializer):
    """One person in an expense: what they paid and what their part is, in the expense's
    currency and in the base currency (the latter is what the balances use)."""

    user = PersonSerializer()
    paid = serializers.IntegerField()
    owed = serializers.IntegerField()
    paid_base = serializers.IntegerField()
    owed_base = serializers.IntegerField()


class EntryActionsSerializer(serializers.Serializer):
    """What the caller can do right now, so the UI doesn't re-derive the rules."""

    confirm = serializers.BooleanField()
    reject = serializers.BooleanField()
    cancel = serializers.BooleanField()
    edit = serializers.BooleanField()
    attach = serializers.BooleanField()


def people_for(entries: list[LedgerEntry]) -> dict[int, Any]:
    """Everyone the entries' breakdowns mention, in one query (put it in the context)."""
    ids: set[int] = set()
    for entry in entries:
        breakdown = get_kind(entry.kind).breakdown(entry)
        if breakdown is not None:
            ids |= breakdown.paid.keys() | breakdown.owed.keys()
    return get_user_model().objects.in_bulk(ids)


class EntrySerializer(serializers.ModelSerializer):
    """An entry as everyone sees it. `details` is kind-specific (ids resolve via `breakdown`
    and `obligations`); an expense's `breakdown` explains it, its `obligations` don't."""

    created_by = PersonSerializer(read_only=True, allow_null=True)
    obligations = ObligationSerializer(many=True, read_only=True)
    attachments = EntryAttachmentSerializer(many=True, read_only=True)
    breakdown = serializers.SerializerMethodField()
    actions = serializers.SerializerMethodField()

    class Meta:
        model = LedgerEntry
        fields = (
            "id",
            "kind",
            "status",
            "title",
            "note",
            "category",
            "occurred_on",
            "amount",
            "currency",
            "item",
            "base_amount",
            "rate",
            "rate_date",
            "details",
            "source_type",
            "source_id",
            "created_by",
            "created_at",
            "updated_at",
            "decided_at",
            "version",
            "obligations",
            "breakdown",
            "attachments",
            "actions",
        )
        read_only_fields = fields

    @extend_schema_field(ShareRowSerializer(many=True, allow_null=True))
    def get_breakdown(self, entry: LedgerEntry) -> list[dict[str, Any]] | None:
        breakdown = get_kind(entry.kind).breakdown(entry)
        if breakdown is None:
            return None
        base = breakdown.to_base(entry.rate or Decimal(1), CURRENCIES[entry.currency])
        people = self.context.get("people") or people_for([entry])
        rows = [
            {
                "user": people[user_id],
                "paid": breakdown.paid.get(user_id, 0),
                "owed": breakdown.owed.get(user_id, 0),
                "paid_base": base.paid.get(user_id, 0),
                "owed_base": base.owed.get(user_id, 0),
            }
            for user_id in sorted(breakdown.paid.keys() | breakdown.owed.keys())
        ]
        rows.sort(key=lambda r: (-r["paid"], r["user"].username))
        return ShareRowSerializer(rows, many=True, context=self.context).data

    @extend_schema_field(EntryActionsSerializer)
    def get_actions(self, entry: LedgerEntry) -> dict[str, bool]:
        return get_kind(entry.kind).actions(entry, self.context["request"].user)


class MemberBalanceSerializer(serializers.Serializer):
    """One person's total in one unit. `net` > 0: others owe them; `net` < 0: they owe."""

    user = PersonSerializer()
    item = serializers.CharField(help_text="Empty for money (base currency minor units).")
    net = serializers.IntegerField()


class TransferSerializer(serializers.Serializer):
    """`debtor` should give `creditor` `amount` (minor units, or a quantity of `item`)."""

    debtor = PersonSerializer()
    creditor = PersonSerializer()
    item = serializers.CharField(help_text="Empty for money.")
    amount = serializers.IntegerField()


class PendingPaymentSerializer(TransferSerializer):
    entry_id = serializers.IntegerField()


class BalancesSerializer(serializers.Serializer):
    currency = serializers.CharField()
    members = MemberBalanceSerializer(many=True)
    settlements = TransferSerializer(many=True)
    pending = PendingPaymentSerializer(many=True)


class RateSerializer(serializers.Serializer):
    currency = serializers.CharField()
    rate = serializers.DecimalField(max_digits=18, decimal_places=8)
    rate_date = serializers.DateField()


class CurrencySerializer(serializers.Serializer):
    code = serializers.CharField()
    exponent = serializers.IntegerField(help_text="Minor-unit digits: 2 for PLN, 0 for JPY.")


class ChoiceSerializer(serializers.Serializer):
    key = serializers.CharField()
    label = serializers.CharField()


class MetaSerializer(serializers.Serializer):
    base_currency = serializers.CharField()
    currencies = CurrencySerializer(many=True)
    categories = ChoiceSerializer(many=True)
