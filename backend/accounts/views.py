from django.conf import settings
from django.db import transaction
from django.shortcuts import get_object_or_404
from django.utils import timezone
from drf_spectacular.utils import extend_schema
from rest_framework import serializers, status
from rest_framework.permissions import AllowAny, IsAdminUser
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.throttling import ScopedRateThrottle
from rest_framework.views import APIView
from rest_framework_simplejwt.tokens import RefreshToken

from accounts.models import ActivationToken, User
from accounts.serializers import (
    INVALID_TOKEN_MESSAGE,
    ActivationCompleteSerializer,
    ActivationLinkSerializer,
    ActivationValidateSerializer,
    LoginUserSerializer,
    MeSerializer,
    TokenPairSerializer,
    UserCreateSerializer,
)


class MeView(APIView):
    @extend_schema(responses={200: MeSerializer})
    def get(self, request: Request) -> Response:
        return Response(MeSerializer(request.user).data)


class LoginUserListView(APIView):
    """Public by design: the app has a small, closed set of users and the login screen lists them.

    Only activated, active accounts are shown, and only username and display name.
    """

    authentication_classes = ()
    permission_classes = (AllowAny,)
    throttle_classes = (ScopedRateThrottle,)
    throttle_scope = "login_users"

    @extend_schema(responses={200: LoginUserSerializer(many=True)})
    def get(self, request: Request) -> Response:
        users = [
            user
            for user in User.objects.filter(is_active=True).order_by("first_name", "username")
            if user.has_usable_password()
        ]
        return Response(LoginUserSerializer(users, many=True).data)


class UserCreateView(APIView):
    """Admin creates a user with no usable password; they activate via a personal link."""

    permission_classes = (IsAdminUser,)

    @extend_schema(request=UserCreateSerializer, responses={201: UserCreateSerializer})
    def post(self, request: Request) -> Response:
        serializer = UserCreateSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        serializer.save()
        return Response(serializer.data, status=status.HTTP_201_CREATED)


class ActivationLinkView(APIView):
    """Issue a one-time activation link; any earlier unused link for the user stops working."""

    permission_classes = (IsAdminUser,)

    @extend_schema(request=None, responses={201: ActivationLinkSerializer})
    def post(self, request: Request, user_id: int) -> Response:
        user = get_object_or_404(User, pk=user_id)
        if not user.is_active:
            raise serializers.ValidationError("Cannot issue a link for an inactive user.")
        token, raw = ActivationToken.issue(user, created_by=request.user)
        data = {
            "token": raw,
            "url": f"{settings.FRONTEND_URL}/activate/{raw}",
            "expires_at": token.expires_at,
        }
        return Response(ActivationLinkSerializer(data).data, status=status.HTTP_201_CREATED)


class PublicActivationView(APIView):
    """Unauthenticated and throttled; the token travels in the body, not the URL."""

    authentication_classes = ()
    permission_classes = (AllowAny,)
    throttle_classes = (ScopedRateThrottle,)
    throttle_scope = "activation"


class ActivationValidateView(PublicActivationView):
    @extend_schema(
        request=ActivationValidateSerializer, responses={200: ActivationValidateSerializer}
    )
    def post(self, request: Request) -> Response:
        serializer = ActivationValidateSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        token: ActivationToken = serializer.validated_data["token"]
        return Response({"username": token.user.get_username()})  # ty: ignore[unresolved-attribute]


class ActivationCompleteView(PublicActivationView):
    @extend_schema(request=ActivationCompleteSerializer, responses={200: TokenPairSerializer})
    def post(self, request: Request) -> Response:
        serializer = ActivationCompleteSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        candidate: ActivationToken = serializer.validated_data["token"]

        with transaction.atomic():
            # Re-check under a row lock so concurrent submissions can't both succeed.
            token = (
                ActivationToken.objects.select_related("user")
                .select_for_update()
                .get(pk=candidate.pk)
            )
            if not token.is_valid:
                raise serializers.ValidationError({"token": [INVALID_TOKEN_MESSAGE]})
            user = token.user
            user.set_password(serializer.validated_data["password"])
            user.save(update_fields=["password"])
            token.used_at = timezone.now()
            token.save(update_fields=["used_at"])

        refresh = RefreshToken.for_user(user)
        return Response({"access": str(refresh.access_token), "refresh": str(refresh)})
