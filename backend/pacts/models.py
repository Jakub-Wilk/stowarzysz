from django.conf import settings
from django.db import models
from django.db.models import Q
from django.db.models.signals import post_delete
from django.dispatch import receiver


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
    last_nudged_at = models.DateTimeField(null=True)  # last "please settle this" push

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
        EXPIRED = "expired"  # an invite or join request nobody answered in time

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
        OVERRULED = "overruled"  # a Sejmik ruling rejected a disputed claim

    pact = models.ForeignKey(Pact, on_delete=models.CASCADE, related_name="proposals")
    # the wager this claim settles (`bet`); null when it settles the whole pact
    wager = models.ForeignKey(
        PactParticipant, on_delete=models.CASCADE, related_name="proposals", null=True
    )
    proposed_by = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    result = models.JSONField()
    verdicts = models.JSONField(default=dict)  # {user_id: won|lost|draw|void}, set on settling
    state = models.CharField(max_length=10, choices=State, default=State.PENDING)
    ruling_poll = models.ForeignKey(
        "voting.Poll", on_delete=models.SET_NULL, null=True, related_name="+"
    )
    created_at = models.DateTimeField(auto_now_add=True)
    decided_at = models.DateTimeField(null=True)

    objects = models.Manager()

    class Meta:
        ordering = ("-created_at", "-id")


class OutcomeConfirmation(models.Model):
    """One required party agreeing to a claim. The claim settles once all of them have."""

    proposal = models.ForeignKey(
        OutcomeProposal, on_delete=models.CASCADE, related_name="confirmations"
    )
    user = models.ForeignKey(settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+")
    created_at = models.DateTimeField(auto_now_add=True)

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(fields=["proposal", "user"], name="pact_confirmation_unique")
        ]


class JoinConsent(models.Model):
    """One approver's say on a join request. Who has to approve depends on the kind."""

    participant = models.ForeignKey(
        PactParticipant, on_delete=models.CASCADE, related_name="consents"
    )
    approver = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    approved = models.BooleanField(null=True)  # null until they decide

    objects = models.Manager()

    class Meta:
        constraints = [  # noqa: RUF012
            models.UniqueConstraint(
                fields=["participant", "approver"], name="pact_join_consent_unique"
            )
        ]


class PactAttachment(models.Model):
    """A picture that supplements a pact's notes (the original agreement, proof of the result).

    Stored via `accounts.avatars.process_photo` and served from `MEDIA_URL`."""

    pact = models.ForeignKey(Pact, on_delete=models.CASCADE, related_name="attachments")
    uploaded_by = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="+"
    )
    image = models.ImageField(upload_to="pact_attachments/")
    caption = models.CharField(max_length=200, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)

    objects = models.Manager()

    class Meta:
        ordering = ("created_at", "id")


@receiver(post_delete, sender=PactAttachment)
def delete_attachment_file(
    sender: type[PactAttachment], instance: PactAttachment, **kwargs
) -> None:
    if instance.image:
        instance.image.storage.delete(instance.image.name)
