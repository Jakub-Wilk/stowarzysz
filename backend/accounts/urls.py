from django.urls import path
from rest_framework_simplejwt.views import (
    TokenBlacklistView,
    TokenObtainPairView,
    TokenRefreshView,
)

from accounts.views import (
    ActivationCompleteView,
    ActivationLinkView,
    ActivationValidateView,
    MeView,
    UserCreateView,
)

urlpatterns = [
    path("users/", UserCreateView.as_view(), name="user_create"),
    path(
        "users/<int:user_id>/activation-link/",
        ActivationLinkView.as_view(),
        name="activation_link",
    ),
    path("activation/validate/", ActivationValidateView.as_view(), name="activation_validate"),
    path("activation/complete/", ActivationCompleteView.as_view(), name="activation_complete"),
    path("token/", TokenObtainPairView.as_view(), name="token_obtain_pair"),
    path("token/refresh/", TokenRefreshView.as_view(), name="token_refresh"),
    path("token/blacklist/", TokenBlacklistView.as_view(), name="token_blacklist"),
    path("me/", MeView.as_view(), name="me"),
]
