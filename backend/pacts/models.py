from django.conf import settings
from django.db import models
from django.db.models import Q


class Pact(models.Model):
    """A bet, resolution or prediction between members. Kind-specific rules live in
    `pacts.kinds`, keyed by `kind`; `config` and `outcome` are JSON so new kinds need no schema
    changes.

    A pact is made of participants: the creator (host) plus everyone invited or who asked to
    join. What each participant's row means (a wager against the host, a side in a shared pot)
    depends on the kind.
    """

    class Status(models.TextChoices):
        PROPOSED = "proposed"  # waiting for the first invitee to accept
        ACTIVE = "active"
        AWAITING_RESULT = "awaiting_result"  # the deadline passed, nobody settled it yet
        RESOLVED = "resolved"
        DECLINED = "declined"  # every invitee said no
        CANCELLED = "cancelled"  # called off by the creator before it started
        VOID = "void"

    creator = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="created_pacts"
    )
    kind = models.CharField(max_length=32)
    title = models.CharField(max_length=200)
    condition = models.TextField()  # what has to happen for the host to win
    notes = models.TextField(blank=True)
    due_at = models.DateTimeField(null=True, blank=True)
    is_open = models.BooleanField(default=False)  # anyone may ask to join
    config = models.JSONField(default=dict)
    status = models.CharField(max_length=16, choices=Status, default=Status.PROPOSED)
    outcome = models.JSONField(null=True)  # frozen when the pact is resolved
    created_at = models.DateTimeField(auto_now_add=True)
    resolved_at = models.DateTimeField(null=True)

    objects = models.Manager()

    class Meta:
        indexes = [models.Index(fields=["status", "-created_at"])]  # noqa: RUF012

    def __str__(self) -> str:
        return self.title


class PactParticipant(models.Model):
    """One person in a pact. For a `bet` every non-host row is a separate wager against the host
    with its own stake, and wagers start, end and settle independently."""

    class Role(models.TextChoices):
        HOST = "host"
        OPPONENT = "opponent"

    class State(models.TextChoices):
        INVITED = "invited"
        REQUESTED = "requested"  # asked to join, waiting for the host
        ACTIVE = "active"
        SETTLED = "settled"
        VOID = "void"
        DECLINED = "declined"
        REJECTED = "rejected"  # the host turned a join request down
        WITHDRAWN = "withdrawn"

    pact = models.ForeignKey(Pact, on_delete=models.CASCADE, related_name="participants")
    user = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="pact_participations"
    )
    role = models.CharField(max_length=8, choices=Role, default=Role.OPPONENT)
    side = models.CharField(max_length=32, blank=True)
    stake_amount = models.PositiveIntegerField(null=True, blank=True)  # minor units (grosze)
    stake_note = models.CharField(max_length=200, blank=True)  # non-cash stake, e.g. "kolacja"
    state = models.CharField(max_length=10, choices=State, default=State.INVITED)
    created_at = models.DateTimeField(auto_now_add=True)

    objects = models.Manager()

    class Meta:
        ordering = ("id",)
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(fields=["pact", "user"], name="pact_participant_unique"),
            models.UniqueConstraint(
                fields=["pact"],
                condition=Q(role="host"),
                name="pact_single_host",
            ),
        ]


class OutcomeProposal(models.Model):
    """Someone's claim of how a wager ended. It settles only once the other side confirms."""

    class State(models.TextChoices):
        PENDING = "pending"
        CONFIRMED = "confirmed"
        DISPUTED = "disputed"
        SUPERSEDED = "superseded"

    pact = models.ForeignKey(Pact, on_delete=models.CASCADE, related_name="proposals")
    wager = models.ForeignKey(PactParticipant, on_delete=models.CASCADE, related_name="proposals")
    proposed_by = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    result = models.JSONField()
    state = models.CharField(max_length=10, choices=State, default=State.PENDING)
    created_at = models.DateTimeField(auto_now_add=True)
    decided_at = models.DateTimeField(null=True)

    objects = models.Manager()

    class Meta:
        ordering = ("-created_at", "-id")
