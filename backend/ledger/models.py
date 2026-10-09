"""The ledger in three layers: an entry is what happened, its obligations are who owes whom
because of it, and balances are the sum of the obligations of the entries that count.

Everything kind-specific (what an entry's `details` hold, how they turn into obligations, who
may do what) lives in `ledger.kinds`; all changes go through `ledger.services`.
"""

from django.conf import settings
from django.db import models
from django.db.models import F, Q
from django.db.models.signals import post_delete
from django.dispatch import receiver


class LedgerEntry(models.Model):
    """Something that happened between members: an expense, a debt or a payment (`kind`).

    Only `confirmed` entries count. Expenses and debts are born confirmed; a payment is pending
    until the receiver confirms it. Cancelling an entry (deleting an expense, withdrawing a
    payment) is the only way one stops counting; nothing is ever deleted.
    """

    class Status(models.TextChoices):
        PENDING = "pending"  # a payment waiting for the receiver
        CONFIRMED = "confirmed"  # counts towards the balances
        REJECTED = "rejected"  # the receiver says they weren't paid
        CANCELLED = "cancelled"  # withdrawn: a deleted expense, a payment taken back

    class Category(models.TextChoices):
        FOOD = "food", "Jedzenie"
        GROCERIES = "groceries", "Zakupy"
        TRANSPORT = "transport", "Transport"
        LODGING = "lodging", "Nocleg"
        FUN = "fun", "Rozrywka"
        BILLS = "bills", "Rachunki"
        OTHER = "other", "Inne"

    kind = models.CharField(max_length=32)  # a key of `ledger.kinds.KINDS`
    status = models.CharField(max_length=10, choices=Status, default=Status.CONFIRMED)
    title = models.CharField(max_length=200)
    note = models.CharField(max_length=500, blank=True)
    category = models.CharField(max_length=16, choices=Category, blank=True)
    occurred_on = models.DateField()
    # The headline amount, as the user typed it: minor units of `currency`, or with `item` a
    # quantity of goods (then `currency` is empty). Null when one entry mixes units.
    amount = models.PositiveIntegerField(null=True)
    currency = models.CharField(max_length=3, blank=True)
    item = models.CharField(max_length=60, blank=True)
    base_amount = models.PositiveIntegerField(null=True)  # `amount` in the base currency
    rate = models.DecimalField(max_digits=18, decimal_places=8, null=True)  # null: base currency
    rate_date = models.DateField(null=True)  # the day the rate is from (a weekend uses Friday's)
    details = models.JSONField(default=dict)  # kind-specific, validated by the kind
    source_type = models.CharField(max_length=32, blank=True)  # e.g. "pact" (no foreign key)
    source_id = models.PositiveIntegerField(null=True, blank=True)
    created_by = models.ForeignKey(  # null: recorded by the system (a settled pact)
        settings.AUTH_USER_MODEL, on_delete=models.SET_NULL, null=True, related_name="+"
    )
    created_at = models.DateTimeField(auto_now_add=True)
    updated_by = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.SET_NULL, null=True, related_name="+"
    )
    updated_at = models.DateTimeField(auto_now=True)
    version = models.PositiveIntegerField(default=1)  # bumped on every change; edits must match
    decided_at = models.DateTimeField(null=True)  # when it was confirmed/rejected/cancelled

    objects = models.Manager()

    class Meta:
        ordering = ("-occurred_on", "-id")
        constraints = [  # noqa: RUF012
            models.CheckConstraint(
                condition=Q(amount__isnull=True) | Q(amount__gt=0),
                name="ledger_entry_amount_positive",
            ),
        ]
        indexes = [  # noqa: RUF012
            models.Index(fields=["source_type", "source_id"], name="ledger_entry_source_idx"),
        ]

    def __str__(self) -> str:
        return f"{self.kind} #{self.pk} ({self.status}): {self.title}"


class Obligation(models.Model):
    """`debtor` owes `creditor` `amount` because of `entry`: minor units of the base currency or,
    with `item`, a quantity of goods. Derived from the entry by its kind and replaced when the
    entry is edited, never written any other way.

    Within an expense the pairing is arbitrary (only the nets matter), so an obligation is never
    an explanation: the entry's breakdown is. A payment's obligation runs from the receiver back
    to the payer, which is exactly what cancels the debt it pays.
    """

    entry = models.ForeignKey(LedgerEntry, on_delete=models.CASCADE, related_name="obligations")
    debtor = models.ForeignKey(settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+")
    creditor = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    amount = models.PositiveIntegerField()
    item = models.CharField(max_length=60, blank=True)

    objects = models.Manager()

    class Meta:
        ordering = ("id",)
        constraints = [  # noqa: RUF012
            models.CheckConstraint(
                condition=Q(amount__gt=0), name="ledger_obligation_amount_positive"
            ),
            models.CheckConstraint(
                condition=~Q(debtor=F("creditor")), name="ledger_obligation_distinct_parties"
            ),
        ]

    def __str__(self) -> str:
        unit = f"x {self.item}" if self.item else "minor units"
        return f"{self.debtor_id} owes {self.creditor_id} {self.amount} {unit}"


class EntryAttachment(models.Model):
    """A picture on an entry: the receipt of an expense, proof of a payment.

    Stored via `accounts.avatars.process_photo` and served from `MEDIA_URL`."""

    entry = models.ForeignKey(LedgerEntry, on_delete=models.CASCADE, related_name="attachments")
    uploaded_by = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    image = models.ImageField(upload_to="ledger_attachments/")
    created_at = models.DateTimeField(auto_now_add=True)

    objects = models.Manager()

    class Meta:
        ordering = ("created_at", "id")


@receiver(post_delete, sender=EntryAttachment)
def delete_attachment_file(
    sender: type[EntryAttachment], instance: EntryAttachment, **kwargs
) -> None:
    if instance.image:
        instance.image.storage.delete(instance.image.name)


class ExchangeRate(models.Model):
    """How many base-currency units one unit of `currency` was worth on `requested_on`, as the
    ECB published it on `rate_date` (the last working day up to then). Never changes once known.
    """

    currency = models.CharField(max_length=3)
    requested_on = models.DateField()
    rate = models.DecimalField(max_digits=18, decimal_places=8)
    rate_date = models.DateField()

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(
                fields=["currency", "requested_on"], name="ledger_rate_unique_day"
            ),
        ]

    def __str__(self) -> str:
        return f"1 {self.currency} = {self.rate} ({self.rate_date})"
