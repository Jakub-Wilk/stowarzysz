from django.conf import settings
from django.db import models
from django.db.models import F, Q


class LedgerEntry(models.Model):
    """One debt: `debtor` owes `creditor` `amount` (minor units, e.g. grosze).

    This is the atomic unit of the ledger. Anything that creates money owed between people
    (a settled bet, later a shared expense) records entries here via `ledger.services`, and
    `source_type` / `source_id` say where an entry came from without a foreign key.
    """

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
    paid_marked_at = models.DateTimeField(null=True)  # the debtor says they paid
    settled_at = models.DateTimeField(null=True)  # the creditor confirms they were paid

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
        return f"{self.debtor_id} -> {self.creditor_id}: {self.amount} {self.currency}"
