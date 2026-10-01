import hashlib
import secrets
from datetime import timedelta
from typing import Self

from django.conf import settings
from django.contrib.auth.models import AbstractUser
from django.db import models
from django.utils import timezone


class User(AbstractUser):
    """Custom user model from day one; extend it here instead of migrating away from auth.User."""


def hash_token(raw: str) -> str:
    return hashlib.sha256(raw.encode()).hexdigest()


class ActivationToken(models.Model):
    """One-time link that lets an admin-created user set their own password.

    Only the SHA-256 of the token is stored; the raw value is shown once, at creation.
    """

    user = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="activation_tokens"
    )
    token_hash = models.CharField(max_length=64, unique=True)
    created_by = models.ForeignKey(
        settings.AUTH_USER_MODEL,
        null=True,
        on_delete=models.SET_NULL,
        related_name="+",
    )
    created_at = models.DateTimeField(auto_now_add=True)
    expires_at = models.DateTimeField()
    used_at = models.DateTimeField(null=True)

    objects = models.Manager()

    @classmethod
    def issue(cls, user: User, created_by: User | None = None) -> tuple[Self, str]:
        """Create a fresh token, invalidating any earlier unused ones for this user."""
        cls.objects.filter(user=user, used_at__isnull=True).delete()
        raw = secrets.token_urlsafe(32)
        obj = cls.objects.create(
            user=user,
            token_hash=hash_token(raw),
            created_by=created_by,
            expires_at=timezone.now() + timedelta(hours=settings.ACTIVATION_TOKEN_TTL_HOURS),
        )
        return obj, raw

    @property
    def is_valid(self) -> bool:
        return self.used_at is None and self.expires_at > timezone.now() and self.user.is_active
