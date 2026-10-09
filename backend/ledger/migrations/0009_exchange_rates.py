from django.db import migrations, models


class Migration(migrations.Migration):
    dependencies = [("ledger", "0008_finalize_entries")]

    operations = [
        migrations.CreateModel(
            name="ExchangeRate",
            fields=[
                (
                    "id",
                    models.BigAutoField(
                        auto_created=True, primary_key=True, serialize=False, verbose_name="ID"
                    ),
                ),
                ("currency", models.CharField(max_length=3)),
                ("requested_on", models.DateField()),
                ("rate", models.DecimalField(decimal_places=8, max_digits=18)),
                ("rate_date", models.DateField()),
            ],
            options={
                "constraints": [
                    models.UniqueConstraint(
                        fields=("currency", "requested_on"), name="ledger_rate_unique_day"
                    )
                ],
            },
        ),
    ]
