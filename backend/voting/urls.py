from django.urls import path

from voting.views import (
    PollAbstainView,
    PollBallotView,
    PollCloseView,
    PollDetailView,
    PollListCreateView,
    PollReactionView,
    PollTieBreakView,
    PollVetoView,
)

urlpatterns = [
    path("", PollListCreateView.as_view(), name="poll_list_create"),
    path("<int:poll_id>/", PollDetailView.as_view(), name="poll_detail"),
    path("<int:poll_id>/ballot/", PollBallotView.as_view(), name="poll_ballot"),
    path("<int:poll_id>/veto/", PollVetoView.as_view(), name="poll_veto"),
    path("<int:poll_id>/abstain/", PollAbstainView.as_view(), name="poll_abstain"),
    path("<int:poll_id>/close/", PollCloseView.as_view(), name="poll_close"),
    path("<int:poll_id>/tie-break/", PollTieBreakView.as_view(), name="poll_tie_break"),
    path("<int:poll_id>/reactions/", PollReactionView.as_view(), name="poll_reactions"),
]
