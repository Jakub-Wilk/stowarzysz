from django.urls import path

from ledger.views import BalancesView, EntryConfirmView, EntryListView, EntryPaidView

urlpatterns = [
    path("balances/", BalancesView.as_view(), name="ledger_balances"),
    path("entries/", EntryListView.as_view(), name="ledger_entries"),
    path("entries/<int:entry_id>/paid/", EntryPaidView.as_view(), name="ledger_entry_paid"),
    path(
        "entries/<int:entry_id>/confirm/", EntryConfirmView.as_view(), name="ledger_entry_confirm"
    ),
]
