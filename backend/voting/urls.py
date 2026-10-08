from django.urls import path

from voting.views import (
    AvatarArchiveListView,
    AvatarArchiveView,
    PollBallotView,
    PollCloseView,
    PollDetailView,
    PollListCreateView,
    PollReactionView,
    PollVetoView,
)

urlpatterns = [
    path("", PollListCreateView.as_view(), name="poll_list_create"),
    path("avatar-archive/", AvatarArchiveListView.as_view(), name="avatar_archive"),
    path("<int:poll_id>/archive-pictures/", AvatarArchiveView.as_view(), name="archive_pictures"),
    path("<int:poll_id>/", PollDetailView.as_view(), name="poll_detail"),
    path("<int:poll_id>/ballot/", PollBallotView.as_view(), name="poll_ballot"),
    path("<int:poll_id>/veto/", PollVetoView.as_view(), name="poll_veto"),
    path("<int:poll_id>/close/", PollCloseView.as_view(), name="poll_close"),
    path("<int:poll_id>/reactions/", PollReactionView.as_view(), name="poll_reactions"),
]
