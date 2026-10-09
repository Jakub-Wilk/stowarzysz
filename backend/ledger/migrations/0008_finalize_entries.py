from django.db import migrations, models


class Migration(migrations.Migration):
    """Separate from 0007 because Postgres can't alter a table in the transaction that just
    wrote rows into it."""

    dependencies = [("ledger", "0007_backfill_obligations")]

    operations = [
        migrations.AlterField(
            model_name="ledgerentry", name="occurred_on", field=models.DateField()
        ),
        migrations.RemoveConstraint(model_name="ledgerentry", name="ledger_entry_distinct_parties"),
        migrations.RemoveField(model_name="ledgerentry", name="debtor"),
        migrations.RemoveField(model_name="ledgerentry", name="creditor"),
    ]
