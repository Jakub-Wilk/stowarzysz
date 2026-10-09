import django.db.models.deletion
import django.utils.timezone
from django.conf import settings
from django.db import migrations, models


class Migration(migrations.Migration):
    """An entry becomes the header of what happened; who owes whom moves to `Obligation`
    (0006-0008). `occurred_on` stays nullable until 0007 has filled it in."""

    dependencies = [
        ("ledger", "0004_item_debts"),
        migrations.swappable_dependency(settings.AUTH_USER_MODEL),
    ]

    operations = [
        migrations.RenameField(model_name="ledgerentry", old_name="description", new_name="title"),
        migrations.AlterField(
            model_name="ledgerentry", name="kind", field=models.CharField(max_length=32)
        ),
        migrations.AlterField(
            model_name="ledgerentry",
            name="currency",
            field=models.CharField(blank=True, max_length=3),
        ),
        migrations.AlterField(
            model_name="ledgerentry", name="amount", field=models.PositiveIntegerField(null=True)
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="note",
            field=models.CharField(blank=True, max_length=500),
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="category",
            field=models.CharField(
                blank=True,
                choices=[
                    ("food", "Jedzenie"),
                    ("groceries", "Zakupy"),
                    ("transport", "Transport"),
                    ("lodging", "Nocleg"),
                    ("fun", "Rozrywka"),
                    ("bills", "Rachunki"),
                    ("other", "Inne"),
                ],
                max_length=16,
            ),
        ),
        migrations.AddField(
            model_name="ledgerentry", name="occurred_on", field=models.DateField(null=True)
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="base_amount",
            field=models.PositiveIntegerField(null=True),
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="rate",
            field=models.DecimalField(decimal_places=8, max_digits=18, null=True),
        ),
        migrations.AddField(
            model_name="ledgerentry", name="rate_date", field=models.DateField(null=True)
        ),
        migrations.AddField(
            model_name="ledgerentry", name="details", field=models.JSONField(default=dict)
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="created_by",
            field=models.ForeignKey(
                null=True,
                on_delete=django.db.models.deletion.SET_NULL,
                related_name="+",
                to=settings.AUTH_USER_MODEL,
            ),
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="updated_by",
            field=models.ForeignKey(
                null=True,
                on_delete=django.db.models.deletion.SET_NULL,
                related_name="+",
                to=settings.AUTH_USER_MODEL,
            ),
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="updated_at",
            field=models.DateTimeField(auto_now=True, default=django.utils.timezone.now),
            preserve_default=False,
        ),
        migrations.AddField(
            model_name="ledgerentry",
            name="version",
            field=models.PositiveIntegerField(default=1),
        ),
        migrations.RemoveConstraint(model_name="ledgerentry", name="ledger_entry_amount_positive"),
        migrations.AddConstraint(
            model_name="ledgerentry",
            constraint=models.CheckConstraint(
                condition=models.Q(("amount__isnull", True), ("amount__gt", 0), _connector="OR"),
                name="ledger_entry_amount_positive",
            ),
        ),
        migrations.AddIndex(
            model_name="ledgerentry",
            index=models.Index(fields=["source_type", "source_id"], name="ledger_entry_source_idx"),
        ),
        migrations.AlterModelOptions(
            name="ledgerentry", options={"ordering": ("-occurred_on", "-id")}
        ),
    ]
