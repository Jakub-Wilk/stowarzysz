from zoneinfo import ZoneInfo

from django.db import migrations

WARSAW = ZoneInfo("Europe/Warsaw")


def entries_get_obligations(apps, schema_editor):
    """Every old entry was one two-party fact; it becomes an entry with one obligation, so every
    balance comes out exactly as before.

    A debt keeps its direction. A payment's obligation runs from the receiver back to the payer
    (that is what cancels the debt it pays), which is how the old balance code read it too.
    Old rows were all PLN or goods; anything else would be reinterpreted, so stop if found.
    """
    LedgerEntry = apps.get_model("ledger", "LedgerEntry")
    Obligation = apps.get_model("ledger", "Obligation")
    if LedgerEntry.objects.filter(item="").exclude(currency="PLN").exists():
        raise RuntimeError("Found money entries in a currency other than PLN; migrate by hand.")

    entries = list(LedgerEntry.objects.order_by("id"))
    obligations = []
    for entry in entries:
        debtor, creditor = entry.debtor_id, entry.creditor_id
        fact = {"debtor_id": debtor, "creditor_id": creditor, "amount": entry.amount}
        if entry.item:
            fact["item"] = entry.item
        if entry.kind == "payment":
            obligations.append(
                Obligation(
                    entry_id=entry.pk,
                    debtor_id=creditor,
                    creditor_id=debtor,
                    amount=entry.amount,
                    item=entry.item,
                )
            )
            entry.details = {"from": debtor, "to": creditor}
            entry.created_by_id = debtor  # the payer said they paid
        else:
            obligations.append(
                Obligation(
                    entry_id=entry.pk,
                    debtor_id=debtor,
                    creditor_id=creditor,
                    amount=entry.amount,
                    item=entry.item,
                )
            )
            entry.details = {"debts": [fact]}
            if entry.source_type == "manual":
                entry.created_by_id = creditor  # recorded by the person owed
        entry.occurred_on = entry.created_at.astimezone(WARSAW).date()
        entry.base_amount = None if entry.item else entry.amount
        entry.updated_by_id = entry.created_by_id
    Obligation.objects.bulk_create(obligations)
    LedgerEntry.objects.bulk_update(
        entries,
        ["details", "created_by", "updated_by", "occurred_on", "base_amount"],
        batch_size=500,
    )


def obligations_back_to_entries(apps, schema_editor):
    """Reversible while 0008 hasn't run: the parties are still on the entry."""
    apps.get_model("ledger", "Obligation").objects.all().delete()


class Migration(migrations.Migration):
    dependencies = [("ledger", "0006_obligations")]

    operations = [migrations.RunPython(entries_get_obligations, obligations_back_to_entries)]
