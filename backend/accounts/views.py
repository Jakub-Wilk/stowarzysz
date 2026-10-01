from django.conf import settings
from django.db import transaction
from django.shortcuts import get_object_or_404
from django.utils import timezone
from drf_spectacular.utils import extend_schema
from rest_framework import generics, serializers, status
from rest_framework.parsers import MultiPartParser
from rest_framework.permissions import AllowAny
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.throttling import ScopedRateThrottle
from rest_framework.views import APIView
from rest_framework_simplejwt.tokens import RefreshToken

from accounts.models import ActivationToken, User
from accounts.permissions import IsSuperuser
from accounts.serializers import (
    INVALID_TOKEN_MESSAGE,
    ActivationCompleteSerializer,
    ActivationLinkSerializer,
    ActivationValidateSerializer,
    AvatarUploadSerializer,
    LoginUserSerializer,
    ManagedUserSerializer,
    MeSerializer,
    PersonSerializer,
    TokenPairSerializer,
)
from voting.stats import annotate_voting_stats


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
            for user in User.objects.filter(is_active=True).order_by("username")
            if user.has_usable_password()
        ]
        return Response(LoginUserSerializer(users, many=True).data)


class PeopleListView(generics.ListAPIView):
    """Signed-in directory for pickers (e.g. who takes part in a vote): active, activated users."""

    serializer_class = PersonSerializer
    pagination_class = None

    def get_queryset(self):
        return (
            User.objects.filter(is_active=True)
            .exclude(password__startswith="!")
            .order_by("username")
        )


class UserListCreateView(generics.ListCreateAPIView):
    """Superuser lists accounts or creates one with no usable password (activated via a link)."""

    permission_classes = (IsSuperuser,)
    serializer_class = ManagedUserSerializer
    queryset = annotate_voting_stats(User.objects.order_by("username"))


class UserDetailView(generics.RetrieveUpdateDestroyAPIView):
    permission_classes = (IsSuperuser,)
    serializer_class = ManagedUserSerializer
    queryset = annotate_voting_stats(User.objects.all())
    lookup_url_kwarg = "user_id"
    http_method_names = ("get", "patch", "delete", "head", "options")

    def perform_destroy(self, instance: User) -> None:
        if instance == self.request.user:
            raise serializers.ValidationError("Nie możesz usunąć samego siebie.")
        instance.delete()


class UserAvatarView(APIView):
    """Set (multipart `avatar` file) or remove a user's profile picture."""

    permission_classes = (IsSuperuser,)
    parser_classes = (MultiPartParser,)

    @extend_schema(request=AvatarUploadSerializer, responses={200: ManagedUserSerializer})
    def put(self, request: Request, user_id: int) -> Response:
        user = get_object_or_404(User, pk=user_id)
        serializer = AvatarUploadSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        user.set_avatar(serializer.validated_data["avatar"])
        return Response(ManagedUserSerializer(user, context={"request": request}).data)

    @extend_schema(request=None, responses={200: ManagedUserSerializer})
    def delete(self, request: Request, user_id: int) -> Response:
        user = get_object_or_404(User, pk=user_id)
        user.clear_avatar()
        return Response(ManagedUserSerializer(user, context={"request": request}).data)


class ActivationLinkView(APIView):
    """Issue a one-time activation link; any earlier unused link for the user stops working."""

    permission_classes = (IsSuperuser,)

    @extend_schema(request=None, responses={201: ActivationLinkSerializer})
    def post(self, request: Request, user_id: int) -> Response:
        user = get_object_or_404(User, pk=user_id)
        if not user.is_active:
            raise serializers.ValidationError(
                "Nie można wystawić linku dla nieaktywnego użytkownika."
            )
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
