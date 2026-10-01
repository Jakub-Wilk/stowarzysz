from django.urls import path

from secretsanta.views import SantaGiftView, SantaHistoryView, SantaView

urlpatterns = [
    path("", SantaView.as_view(), name="santa"),
    path("history/", SantaHistoryView.as_view(), name="santa_history"),
    path("gifts/<int:gift_id>/", SantaGiftView.as_view(), name="santa_gift"),
]
