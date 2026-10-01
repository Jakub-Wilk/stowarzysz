from django.urls import path

from push.views import PublicKeyView, SubscriptionView

urlpatterns = [
    path("public-key/", PublicKeyView.as_view(), name="push_public_key"),
    path("subscriptions/", SubscriptionView.as_view(), name="push_subscriptions"),
]
