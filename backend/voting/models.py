from typing import Any, cast
from uuid import uuid4

from django.conf import settings
from django.core.files.base import ContentFile
from django.db import models, transaction
from django.db.models.fields.files import FieldFile
from django.db.models.signals import post_delete
from django.dispatch import receiver


class Poll(models.Model):
    """A vote called by a user. Type-specific behaviour lives in `voting.kinds`, keyed by `kind`.

    `config`, each participant's `ballot` and the frozen `result` are JSON so that new kinds
    (yes/no, pick-one-of-options, ...) need no schema changes.
    """

    class Status(models.TextChoices):
        OPEN = "open"
        CLOSED = "closed"

    class CloseReason(models.TextChoices):
        AUTO = "auto"  # everyone voted
        CREATOR = "creator"  # ended early by the caller
        EXPIRED = "expired"  # ran out of time (see `services.POLL_DURATION`)

    creator = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="created_polls"
    )
    title = models.CharField(max_length=200)
    kind = models.CharField(max_length=32)
    config = models.JSONField(default=dict)
    status = models.CharField(max_length=8, choices=Status, default=Status.OPEN)
    close_reason = models.CharField(max_length=8, choices=CloseReason, null=True)
    result = models.JSONField(null=True)  # computed once at close, then frozen
    created_at = models.DateTimeField(auto_now_add=True)
    closed_at = models.DateTimeField(null=True)
    reminders_sent = models.PositiveSmallIntegerField(default=0)  # deadline reminder stages sent
    proposed_avatar = models.ImageField(upload_to="poll_proposals/", blank=True)

    objects = models.Manager()

    class Meta:
        indexes = [models.Index(fields=["status", "-created_at"])]  # noqa: RUF012

    def __str__(self) -> str:
        return self.title

    @property
    def proposed_avatar_file(self) -> FieldFile:
        return cast(FieldFile, self.proposed_avatar)

    def attach_proposed_avatar(self, content: ContentFile) -> None:
        """Store an already-processed image for a profile-picture vote."""
        self.proposed_avatar_file.save(f"{uuid4().hex}.webp", content, save=True)

    def discard_proposed_avatar(self) -> None:
        """Forget the pending image (the file goes once the transaction commits)."""
        name = self.proposed_avatar_file.name
        if not name:
            return
        storage = self.proposed_avatar_file.storage
        self.proposed_avatar = ""
        transaction.on_commit(lambda: storage.delete(name))


@receiver(post_delete, sender=Poll)
def delete_proposed_avatar_file(sender: type[Poll], instance: Poll, **kwargs: Any) -> None:
    if instance.proposed_avatar:
        instance.proposed_avatar_file.storage.delete(instance.proposed_avatar_file.name)


class PollParticipant(models.Model):
    poll = models.ForeignKey(Poll, on_delete=models.CASCADE, related_name="participants")
    user = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="poll_participations"
    )
    ballot = models.JSONField(null=True)  # shape defined by the poll's kind
    vetoed = models.BooleanField(default=False)
    voted_at = models.DateTimeField(null=True)

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012 (Django's own pattern)
            models.UniqueConstraint(fields=["poll", "user"], name="unique_participant")
        ]

    @property
    def has_ballot(self) -> bool:
        return self.voted_at is not None
