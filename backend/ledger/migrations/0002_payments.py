from django.db import migrations, models


def settled_debts_become_payments(apps, schema_editor):
    """Debts used to be closed per entry. Now they stay, and each closed one gets a confirmed
    payment of the same amount, so every balance comes out as before."""
    LedgerEntry = apps.get_model("ledger", "LedgerEntry")
    for entry in LedgerEntry.objects.filter(settled_at__isnull=False):
        LedgerEntry.objects.create(
            kind="payment",
            status="confirmed",
            debtor_id=entry.debtor_id,
            creditor_id=entry.creditor_id,
            amount=entry.amount,
            currency=entry.currency,
            description="Spłata długu",
            decided_at=entry.settled_at,
        )


class Migration(migrations.Migration):
    dependencies = [("ledger", "0001_initial")]

    operations = [
        migrations.AddField(
            model_name="ledgerentry",
            name="kind",
            field=models.CharField(
                choices=[("debt", "Debt"), ("payment", "Payment")], default="debt", max_length=8
            ),
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="status",
            field=models.CharField(
                choices=[
                    ("pending", "Pending"),
                    ("confirmed", "Confirmed"),
                    ("rejected", "Rejected"),
                    ("cancelled", "Cancelled"),
                ],
                default="confirmed",
                max_length=10,
            ),
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="decided_at",
            field=models.DateTimeField(null=True),
        ),
        migrations.RunPython(settled_debts_become_payments, migrations.RunPython.noop),
    ]
