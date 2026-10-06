from django.urls import path

from secretsanta.views import (
    SantaGiftView,
    SantaHelpAnswerView,
    SantaHelpView,
    SantaHistoryView,
    SantaView,
)

urlpatterns = [
    path("", SantaView.as_view(), name="santa"),
    path("history/", SantaHistoryView.as_view(), name="santa_history"),
    path("help/", SantaHelpView.as_view(), name="santa_help"),
    path("help/<int:request_id>/", SantaHelpAnswerView.as_view(), name="santa_help_answer"),
    path("gifts/<int:gift_id>/", SantaGiftView.as_view(), name="santa_gift"),
]
