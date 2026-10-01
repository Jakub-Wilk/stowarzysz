from typing import Any

from django.contrib.auth import get_user_model
from django.contrib.auth.password_validation import validate_password
from django.core.exceptions import ValidationError as DjangoValidationError
from rest_framework import serializers

from accounts.models import ActivationToken, hash_token

User = get_user_model()

INVALID_TOKEN_MESSAGE = "This activation link is invalid or has expired."


def display_name(user: Any) -> str:
    return user.get_full_name() or user.get_username()


class UserCreateSerializer(serializers.ModelSerializer):
    class Meta:
        model = User
        fields = ("id", "username", "email", "first_name", "last_name")
        read_only_fields = ("id",)

    def create(self, validated_data: dict[str, Any]) -> Any:
        user = User(**validated_data)
        user.set_unusable_password()
        user.save()
        return user


class MeSerializer(serializers.ModelSerializer):
    display_name = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = ("id", "username", "email", "display_name")
        read_only_fields = fields

    def get_display_name(self, obj: Any) -> str:
        return display_name(obj)


class LoginUserSerializer(serializers.ModelSerializer):
    """What the unauthenticated login screen may know about each account."""

    display_name = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = ("username", "display_name")
        read_only_fields = fields

    def get_display_name(self, obj: Any) -> str:
        return display_name(obj)


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
