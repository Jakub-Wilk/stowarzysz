from django.conf import settings
from django.db import models


class SantaEvent(models.Model):
    """One round of Secret Santa. Only one can be active; ended rounds stay as public history."""

    class Status(models.TextChoices):
        ACTIVE = "active"
        ENDED = "ended"

    status = models.CharField(max_length=8, choices=Status, default=Status.ACTIVE)
    deadline = models.DateTimeField()  # informational only; a superuser ends the event by hand
    gift_tiers = models.JSONField(default=list)  # PLN amounts, one gift per tier per giver
    created_by = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    created_at = models.DateTimeField(auto_now_add=True)
    ended_at = models.DateTimeField(null=True)

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012 (Django's own pattern)
            models.UniqueConstraint(
                fields=["status"],
                condition=models.Q(status="active"),
                name="single_active_santa_event",
            )
        ]


class SantaAssignment(models.Model):
    """Who gives to whom.

    While the event is active the pairing exists only as `payload`, a Fernet token keyed by
    `settings.SECRET_SANTA_KEY`, and `receiver` is null. Ending the event decrypts it into
    `receiver` and makes the pairing public history.
    """

    event = models.ForeignKey(SantaEvent, on_delete=models.CASCADE, related_name="assignments")
    giver = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="santa_assignments"
    )
    payload = models.TextField(blank=True)
    receiver = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, null=True, related_name="+"
    )

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(fields=["event", "giver"], name="unique_santa_giver")
        ]


class SantaGift(models.Model):
    """What was gifted for one tier of one pairing. Snapshotted when the event ends."""

    assignment = models.ForeignKey(SantaAssignment, on_delete=models.CASCADE, related_name="gifts")
    amount = models.PositiveIntegerField()
    note = models.CharField(max_length=300, blank=True)

    objects = models.Manager()

    class Meta:
        ordering = ("-amount",)
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(fields=["assignment", "amount"], name="unique_santa_gift")
        ]
