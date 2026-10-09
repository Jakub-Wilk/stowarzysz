import django.db.models.deletion
from django.conf import settings
from django.db import migrations, models


class Migration(migrations.Migration):
    dependencies = [
        ("ledger", "0005_entry_fields"),
        migrations.swappable_dependency(settings.AUTH_USER_MODEL),
    ]

    operations = [
        migrations.CreateModel(
            name="Obligation",
            fields=[
                (
                    "id",
                    models.BigAutoField(
                        auto_created=True, primary_key=True, serialize=False, verbose_name="ID"
                    ),
                ),
                ("amount", models.PositiveIntegerField()),
                ("item", models.CharField(blank=True, max_length=60)),
                (
                    "creditor",
                    models.ForeignKey(
                        on_delete=django.db.models.deletion.CASCADE,
                        related_name="+",
                        to=settings.AUTH_USER_MODEL,
                    ),
                ),
                (
                    "debtor",
                    models.ForeignKey(
                        on_delete=django.db.models.deletion.CASCADE,
                        related_name="+",
                        to=settings.AUTH_USER_MODEL,
                    ),
                ),
                (
                    "entry",
                    models.ForeignKey(
                        on_delete=django.db.models.deletion.CASCADE,
                        related_name="obligations",
                        to="ledger.ledgerentry",
                    ),
                ),
            ],
            options={
                "ordering": ("id",),
                "constraints": [
                    models.CheckConstraint(
                        condition=models.Q(("amount__gt", 0)),
                        name="ledger_obligation_amount_positive",
                    ),
                    models.CheckConstraint(
                        condition=models.Q(("debtor", models.F("creditor")), _negated=True),
                        name="ledger_obligation_distinct_parties",
                    ),
                ],
            },
        ),
    ]
