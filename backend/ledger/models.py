from django.conf import settings
from django.db import models
from django.db.models import F, Q


class LedgerEntry(models.Model):
    """One line of the ledger. Amounts are minor units (e.g. grosze). Entries are never edited
    or deleted: the balances are the sum of the confirmed ones.

    - `debt`: `debtor` owes `creditor` (a settled bet, later a shared expense). Recorded by the
      system, counts at once. `source_type` / `source_id` say where it came from without a
      foreign key.
    - `payment`: `debtor` paid `creditor` to work off what they owe. Started by the payer, it
      only counts once the receiver confirms it (and can be rejected or cancelled before that).
    """

    class Kind(models.TextChoices):
        DEBT = "debt"
        PAYMENT = "payment"

    class Status(models.TextChoices):
        PENDING = "pending"  # a payment waiting for the receiver
        CONFIRMED = "confirmed"  # counts towards the balances
        REJECTED = "rejected"  # the receiver says they weren't paid
        CANCELLED = "cancelled"  # the payer took it back

    kind = models.CharField(max_length=8, choices=Kind, default=Kind.DEBT)
    status = models.CharField(max_length=10, choices=Status, default=Status.CONFIRMED)
    debtor = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="ledger_debts"
    )
    creditor = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="ledger_credits"
    )
    amount = models.PositiveIntegerField()
    currency = models.CharField(max_length=3, default="PLN")
    description = models.CharField(max_length=200)
    source_type = models.CharField(max_length=32, blank=True)
    source_id = models.PositiveIntegerField(null=True, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)
    decided_at = models.DateTimeField(null=True)  # when a payment was confirmed/rejected/cancelled

    objects = models.Manager()

    class Meta:
        ordering = ("-created_at", "-id")
        constraints = [  # noqa: RUF012
            models.CheckConstraint(condition=Q(amount__gt=0), name="ledger_entry_amount_positive"),
            models.CheckConstraint(
                condition=~Q(debtor=F("creditor")), name="ledger_entry_distinct_parties"
            ),
        ]

    def __str__(self) -> str:
        return f"{self.kind} {self.debtor_id} -> {self.creditor_id}: {self.amount} {self.currency}"
