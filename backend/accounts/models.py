import hashlib
import secrets
from datetime import timedelta
from typing import Any, Self, cast
from uuid import uuid4

from django.conf import settings
from django.contrib.auth.base_user import AbstractBaseUser, BaseUserManager
from django.contrib.auth.models import PermissionsMixin
from django.contrib.auth.validators import UnicodeUsernameValidator
from django.core.files.base import ContentFile
from django.db import models
from django.db.models.fields.files import FieldFile
from django.db.models.signals import post_delete
from django.dispatch import receiver
from django.utils import timezone


class UserManager(BaseUserManager["User"]):
    def create_user(self, username: str, password: str | None = None, **extra: Any) -> User:
        """`password=None` creates an account with no usable password (activated via a link)."""
        if not username:
            raise ValueError("Nazwa użytkownika jest wymagana")
        user = self.model(username=self.model.normalize_username(username), **extra)
        user.set_password(password)
        user.save(using=self._db)
        return user

    def create_superuser(self, username: str, password: str | None = None, **extra: Any) -> User:
        extra.setdefault("is_superuser", True)
        return self.create_user(username, password, **extra)


class User(AbstractBaseUser, PermissionsMixin):
    """Custom user model from day one; extend it here instead of migrating away from auth.User.

    The app identifies people by username only: no name or email fields.
    """

    username = models.CharField(
        max_length=150,
        unique=True,
        validators=[UnicodeUsernameValidator()],
        error_messages={"unique": "Użytkownik o takiej nazwie już istnieje."},
    )
    is_active = models.BooleanField(default=True)
    date_joined = models.DateTimeField(default=timezone.now)
    avatar = models.ImageField(upload_to="avatars/", blank=True)

    objects = UserManager()

    USERNAME_FIELD = "username"
    REQUIRED_FIELDS: list[str] = []  # noqa: RUF012 (Django's own pattern; the base is a list)

    @property
    def avatar_file(self) -> FieldFile:
        """The avatar as a FieldFile (type checkers see the raw field here)."""
        return cast(FieldFile, self.avatar)

    def set_avatar(self, content: ContentFile) -> None:
        """Store an already-processed image under a fresh name (no stale caches)."""
        old = self.avatar_file.name
        self.avatar_file.save(f"{uuid4().hex}.webp", content, save=False)
        self.save(update_fields=["avatar"])
        if old:
            self.avatar_file.storage.delete(old)

    def clear_avatar(self) -> None:
        self.avatar_file.delete(save=False)  # removes the file and blanks the field
        self.save(update_fields=["avatar"])


@receiver(post_delete, sender=User)
def delete_avatar_file(sender: type[User], instance: User, **kwargs: Any) -> None:
    if instance.avatar:
        instance.avatar_file.storage.delete(instance.avatar_file.name)


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
