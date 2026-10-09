from django.db import migrations


class Migration(migrations.Migration):
    """Separate from 0002 because Postgres can't alter a table in the transaction that just
    inserted rows into it."""

    dependencies = [("ledger", "0002_payments")]

    operations = [
        migrations.RemoveField(model_name="ledgerentry", name="paid_marked_at"),
        migrations.RemoveField(model_name="ledgerentry", name="settled_at"),
    ]
