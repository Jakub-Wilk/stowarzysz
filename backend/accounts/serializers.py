from typing import Any

from django.contrib.auth import get_user_model
from django.contrib.auth.password_validation import validate_password
from django.core.exceptions import ValidationError as DjangoValidationError
from drf_spectacular.utils import extend_schema_field
from rest_framework import serializers
from rest_framework_simplejwt.token_blacklist.models import BlacklistedToken, OutstandingToken

from accounts.avatars import process_avatar
from accounts.models import ActivationToken, hash_token
from pacts.stats import EMPTY, PactStatsSerializer, all_pact_stats
from voting.stats import VotingStatsSerializer, voting_stats

User = get_user_model()

INVALID_TOKEN_MESSAGE = "Ten link aktywacyjny jest nieprawidłowy lub wygasł."


def avatar_url(user: Any) -> str | None:
    return user.avatar.url if user.avatar else None


class PersonSerializer(serializers.ModelSerializer):
    """Minimal public-to-members view of an account: who they are and their picture."""

    avatar_url = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = ("id", "username", "avatar_url")
        read_only_fields = fields

    def get_avatar_url(self, obj: Any) -> str | None:
        return avatar_url(obj)


class ManagedUserSerializer(serializers.ModelSerializer):
    """Full account view for superusers. New accounts get no password; they activate via a link."""

    has_password = serializers.SerializerMethodField()
    avatar_url = serializers.SerializerMethodField()
    voting = serializers.SerializerMethodField()
    pacts = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = (
            "id",
            "username",
            "is_active",
            "is_superuser",
            "has_password",
            "avatar_url",
            "voting",
            "pacts",
        )
        read_only_fields = ("id", "has_password", "avatar_url", "voting", "pacts")

    @extend_schema_field(VotingStatsSerializer)
    def get_voting(self, obj: Any) -> dict[str, Any]:
        return voting_stats(obj)

    @extend_schema_field(PactStatsSerializer)
    def get_pacts(self, obj: Any) -> dict[str, Any]:
        # everyone's record is computed once per request, not once per listed user
        if "pact_stats" not in self.context:
            self.context["pact_stats"] = all_pact_stats()
        return self.context["pact_stats"].get(obj.pk, EMPTY)

    def get_has_password(self, obj: Any) -> bool:
        return obj.has_usable_password()

    def get_avatar_url(self, obj: Any) -> str | None:
        return avatar_url(obj)

    def validate(self, attrs: dict[str, Any]) -> dict[str, Any]:
        # Guard against an admin locking themselves out. Since the requester is always an active
        # superuser, any *other* target leaves at least one active superuser behind.
        request = self.context.get("request")
        if self.instance is not None and request is not None and self.instance == request.user:
            if attrs.get("is_active") is False:
                raise serializers.ValidationError(
                    {"is_active": "Nie możesz dezaktywować samego siebie."}
                )
            if attrs.get("is_superuser") is False:
                raise serializers.ValidationError(
                    {"is_superuser": "Nie możesz odebrać sobie uprawnień superużytkownika."}
                )
        return attrs

    def create(self, validated_data: dict[str, Any]) -> Any:
        return User.objects.create_user(password=None, **validated_data)

    def update(self, instance: Any, validated_data: dict[str, Any]) -> Any:
        was_active = instance.is_active
        user = super().update(instance, validated_data)
        if was_active and not user.is_active:
            # Deactivation also ends existing sessions (refresh tokens).
            for token in OutstandingToken._default_manager.filter(user=user):
                BlacklistedToken._default_manager.get_or_create(token=token)
        return user


class AvatarUploadSerializer(serializers.Serializer):
    avatar = serializers.ImageField(write_only=True)

    def validate_avatar(self, value: Any) -> Any:
        return process_avatar(value)  # the validated value is the normalised image, ready to store


class MeSerializer(serializers.ModelSerializer):
    avatar_url = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = ("id", "username", "is_superuser", "avatar_url")
        read_only_fields = fields

    def get_avatar_url(self, obj: Any) -> str | None:
        return avatar_url(obj)


class LoginUserSerializer(serializers.ModelSerializer):
    """What the unauthenticated login screen may know about each account."""

    avatar_url = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = ("username", "avatar_url")
        read_only_fields = fields

    def get_avatar_url(self, obj: Any) -> str | None:
        return avatar_url(obj)


class ActivationLinkSerializer(serializers.Serializer):
    token = serializers.CharField(read_only=True)
    url = serializers.URLField(read_only=True)
    expires_at = serializers.DateTimeField(read_only=True)


class ActivationTokenField(serializers.CharField):
    """Resolves the raw token to an ActivationToken; every failure looks identical."""

    def to_internal_value(self, data: Any) -> ActivationToken:
        raw = super().to_internal_value(data)
        token = (
            ActivationToken.objects.select_related("user")
            .filter(token_hash=hash_token(raw))
            .first()
        )
        if token is None or not token.is_valid:
            raise serializers.ValidationError(INVALID_TOKEN_MESSAGE)
        return token


class ActivationValidateSerializer(serializers.Serializer):
    token = ActivationTokenField(write_only=True)
    username = serializers.CharField(read_only=True)


class ActivationCompleteSerializer(serializers.Serializer):
    token = ActivationTokenField(write_only=True)
    password = serializers.CharField(write_only=True, trim_whitespace=False)

    def validate(self, attrs: dict[str, Any]) -> dict[str, Any]:
        try:
            validate_password(attrs["password"], attrs["token"].user)
        except DjangoValidationError as exc:
            raise serializers.ValidationError({"password": list(exc.messages)}) from exc
        return attrs


class TokenPairSerializer(serializers.Serializer):
    access = serializers.CharField()
    refresh = serializers.CharField()
