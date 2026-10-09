from django.urls import path

from pacts.views import (
    AttachmentDetailView,
    AttachmentListView,
    JoinDecideView,
    JudgmentView,
    OutcomeConfirmView,
    OutcomeDisputeView,
    OutcomeEscalateView,
    OutcomeProposeView,
    PactCancelView,
    PactDetailView,
    PactJoinView,
    PactListCreateView,
    PactRespondView,
    PactWithdrawView,
)

urlpatterns = [
    path("", PactListCreateView.as_view(), name="pact_list_create"),
    path("<int:pact_id>/", PactDetailView.as_view(), name="pact_detail"),
    path("<int:pact_id>/respond/", PactRespondView.as_view(), name="pact_respond"),
    path("<int:pact_id>/join/", PactJoinView.as_view(), name="pact_join"),
    path("<int:pact_id>/withdraw/", PactWithdrawView.as_view(), name="pact_withdraw"),
    path(
        "<int:pact_id>/participants/<int:participant_id>/decide/",
        JoinDecideView.as_view(),
        name="pact_join_decide",
    ),
    path("<int:pact_id>/attachments/", AttachmentListView.as_view(), name="pact_attachments"),
    path(
        "<int:pact_id>/attachments/<int:attachment_id>/",
        AttachmentDetailView.as_view(),
        name="pact_attachment",
    ),
    path("<int:pact_id>/judgment/", JudgmentView.as_view(), name="pact_judgment"),
    path("<int:pact_id>/cancel/", PactCancelView.as_view(), name="pact_cancel"),
    path("<int:pact_id>/outcome/", OutcomeProposeView.as_view(), name="pact_outcome_propose"),
    path(
        "<int:pact_id>/outcome/<int:proposal_id>/confirm/",
        OutcomeConfirmView.as_view(),
        name="pact_outcome_confirm",
    ),
    path(
        "<int:pact_id>/outcome/<int:proposal_id>/escalate/",
        OutcomeEscalateView.as_view(),
        name="pact_outcome_escalate",
    ),
    path(
        "<int:pact_id>/outcome/<int:proposal_id>/dispute/",
        OutcomeDisputeView.as_view(),
        name="pact_outcome_dispute",
    ),
]
