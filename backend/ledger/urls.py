from django.urls import path

from ledger.views import (
    BalancesView,
    EntryListView,
    PaymentCancelView,
    PaymentConfirmView,
    PaymentCreateView,
    PaymentRejectView,
)

urlpatterns = [
    path("balances/", BalancesView.as_view(), name="ledger_balances"),
    path("entries/", EntryListView.as_view(), name="ledger_entries"),
    path("payments/", PaymentCreateView.as_view(), name="ledger_payment_create"),
    path("entries/<int:entry_id>/confirm/", PaymentConfirmView.as_view(), name="ledger_confirm"),
    path("entries/<int:entry_id>/reject/", PaymentRejectView.as_view(), name="ledger_reject"),
    path("entries/<int:entry_id>/cancel/", PaymentCancelView.as_view(), name="ledger_cancel"),
]
