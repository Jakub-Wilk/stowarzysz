from django.conf import settings
from django.db import models


class SantaEvent(models.Model):
    """One round of Secret Santa. Only one can be active; ended rounds stay as public history."""

    class Status(models.TextChoices):
        ACTIVE = "active"
        ENDED = "ended"

    class Mode(models.TextChoices):
        SINGLE = "single"  # one victim per giver, who gets every tier
        PER_TIER = "per_tier"  # a different victim for each tier

    status = models.CharField(max_length=8, choices=Status, default=Status.ACTIVE)
    mode = models.CharField(max_length=8, choices=Mode, default=Mode.SINGLE)
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
    """Who gives to whom: one row per giver, or per giver and tier in `per_tier` mode.

    While the event is active the pairing exists only as `payload`, a Fernet token keyed by
    `settings.SECRET_SANTA_KEY`, and `receiver` is null. Ending the event decrypts it into
    `receiver` and makes the pairing public history.
    """

    event = models.ForeignKey(SantaEvent, on_delete=models.CASCADE, related_name="assignments")
    giver = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="santa_assignments"
    )
    # Position in `event.gift_tiers` (0 = highest); null when the giver covers every tier.
    # An index rather than an amount, so amounts can be edited without touching the draw.
    tier_index = models.PositiveSmallIntegerField(null=True)
    payload = models.TextField(blank=True)
    receiver = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, null=True, related_name="+"
    )

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(
                fields=["event", "giver"],
                condition=models.Q(tier_index__isnull=True),
                name="unique_santa_giver",
            ),
            models.UniqueConstraint(
                fields=["event", "giver", "tier_index"],
                condition=models.Q(tier_index__isnull=False),
                name="unique_santa_giver_tier",
            ),
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


class SantaHelpRequest(models.Model):
    """A giver asking their victim, anonymously, for gift ideas while an event is active.

    It names the receiver and tier, never the giver: each receiver has exactly one giver per
    tier (or one overall in `single` mode), so that pair identifies the pairing without
    putting it in plain text next to the sealed draw.
    """

    event = models.ForeignKey(SantaEvent, on_delete=models.CASCADE, related_name="help_requests")
    receiver = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    tier_index = models.PositiveSmallIntegerField(null=True)  # as on `SantaAssignment`
    ideas = models.JSONField(default=list)  # the receiver's latest answer
    asked_at = models.DateTimeField()
    answered_at = models.DateTimeField(null=True)  # cleared when the giver asks again

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(
                fields=["event", "receiver"],
                condition=models.Q(tier_index__isnull=True),
                name="unique_santa_help",
            ),
            models.UniqueConstraint(
                fields=["event", "receiver", "tier_index"],
                condition=models.Q(tier_index__isnull=False),
                name="unique_santa_help_tier",
            ),
        ]

    @property
    def pending(self) -> bool:
        return self.answered_at is None
