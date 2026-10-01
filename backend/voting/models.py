from django.conf import settings
from django.db import models


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

    objects = models.Manager()

    class Meta:
        indexes = [models.Index(fields=["status", "-created_at"])]  # noqa: RUF012

    def __str__(self) -> str:
        return self.title


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
