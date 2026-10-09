from django.urls import path

from ledger.views import (
    AttachmentDetailView,
    AttachmentListView,
    BalancesView,
    CancelView,
    ConfirmView,
    EntryDetailView,
    EntryListView,
    MetaView,
    RateView,
    RejectView,
    StatsView,
)

urlpatterns = [
    path("entries/", EntryListView.as_view(), name="ledger_entries"),
    path("entries/<int:entry_id>/", EntryDetailView.as_view(), name="ledger_entry"),
    path("entries/<int:entry_id>/confirm/", ConfirmView.as_view(), name="ledger_confirm"),
    path("entries/<int:entry_id>/reject/", RejectView.as_view(), name="ledger_reject"),
    path("entries/<int:entry_id>/cancel/", CancelView.as_view(), name="ledger_cancel"),
    path(
        "entries/<int:entry_id>/attachments/",
        AttachmentListView.as_view(),
        name="ledger_attachments",
    ),
    path(
        "entries/<int:entry_id>/attachments/<int:attachment_id>/",
        AttachmentDetailView.as_view(),
        name="ledger_attachment",
    ),
    path("balances/", BalancesView.as_view(), name="ledger_balances"),
    path("rates/", RateView.as_view(), name="ledger_rate"),
    path("stats/", StatsView.as_view(), name="ledger_stats"),
    path("meta/", MetaView.as_view(), name="ledger_meta"),
]
