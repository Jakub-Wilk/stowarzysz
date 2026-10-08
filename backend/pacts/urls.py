from django.urls import path

from pacts.views import (
    OutcomeConfirmView,
    OutcomeDisputeView,
    OutcomeProposeView,
    PactCancelView,
    PactDetailView,
    PactListCreateView,
    PactRespondView,
)

urlpatterns = [
    path("", PactListCreateView.as_view(), name="pact_list_create"),
    path("<int:pact_id>/", PactDetailView.as_view(), name="pact_detail"),
    path("<int:pact_id>/respond/", PactRespondView.as_view(), name="pact_respond"),
    path("<int:pact_id>/cancel/", PactCancelView.as_view(), name="pact_cancel"),
    path("<int:pact_id>/outcome/", OutcomeProposeView.as_view(), name="pact_outcome_propose"),
    path(
        "<int:pact_id>/outcome/<int:proposal_id>/confirm/",
        OutcomeConfirmView.as_view(),
        name="pact_outcome_confirm",
    ),
    path(
        "<int:pact_id>/outcome/<int:proposal_id>/dispute/",
        OutcomeDisputeView.as_view(),
        name="pact_outcome_dispute",
    ),
]
