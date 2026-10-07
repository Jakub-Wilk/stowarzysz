"""All poll state changes go through here (locked, validated, events sent after commit)."""

from collections.abc import Iterable
from datetime import datetime, timedelta
from typing import Any

from django.contrib.auth import get_user_model
from django.core.files.base import ContentFile
from django.db import transaction
from django.utils import timezone
from rest_framework.exceptions import APIException, PermissionDenied, ValidationError

from core.events import broadcast
from push.sender import send_push
from voting.kinds import Entry, get_kind
from voting.models import Poll, PollParticipant

REACTION_EMOJI = ("❤️", "🔥", "😭", "👎", "🤣")

POLL_DURATION = timedelta(hours=72)  # every poll ends this long after it was created
REMINDER_OFFSETS = (timedelta(hours=24), timedelta(hours=48), timedelta(hours=69))


class Conflict(APIException):
    status_code = 409
    default_detail = "To głosowanie już się zakończyło."
    default_code = "conflict"


def _locked_open_poll(poll_id: int) -> Poll:
    poll = Poll.objects.select_for_update().get(pk=poll_id)
    if poll.status != Poll.Status.OPEN:
        raise Conflict()
    return poll


def _participant(poll: Poll, user: Any) -> PollParticipant:
    participant = PollParticipant.objects.filter(poll=poll, user=user).first()
    if participant is None:
        raise PermissionDenied("Nie bierzesz udziału w tym głosowaniu.")
    return participant


def _close(poll: Poll, reason: str) -> None:
    kind = get_kind(poll.kind)
    cast = PollParticipant.objects.filter(poll=poll, voted_at__isnull=False)
    entries = [Entry(ballot=p.ballot, vetoed=p.vetoed) for p in cast]
    result = kind.compute_result(poll.config, entries)
    result.update(kind.on_close(poll, result))  # e.g. apply the change a vote was about
    poll.result = result
    poll.status = Poll.Status.CLOSED
    poll.close_reason = reason
    poll.closed_at = timezone.now()
    poll.save(update_fields=["result", "status", "close_reason", "closed_at", "proposed_avatar"])
    participant_ids = list(
        PollParticipant.objects.filter(poll=poll).values_list("user_id", flat=True)
    )

    def announce() -> None:
        broadcast("poll.closed", {"poll_id": poll.pk})
        send_push(
            participant_ids,
            title="Głosowanie zakończone",
            body=_closing_summary(poll.title, result),
            url=f"/voting/{poll.pk}",
        )

    transaction.on_commit(announce)


def _closing_summary(title: str, result: dict[str, Any]) -> str:
    """Push body: the title, plus what came of it for votes that change a profile."""
    if "applied" not in result:
        return title
    if result["applied"]:
        return f"{title}: zmiana zastosowana"
    if result.get("approved"):
        return f"{title}: przyjęto, ale nie udało się zastosować zmiany"
    return f"{title}: nie przyjęto"


def _after_ballot(poll: Poll) -> None:
    everyone_voted = not PollParticipant.objects.filter(poll=poll, voted_at__isnull=True).exists()
    if everyone_voted:
        _close(poll, Poll.CloseReason.AUTO)
    else:
        transaction.on_commit(lambda: broadcast("poll.updated", {"poll_id": poll.pk}))


def create_poll(
    *,
    creator: Any,
    title: str,
    kind_key: str,
    config: Any,
    participant_ids: Iterable[int],
    image: ContentFile | None = None,
) -> Poll:
    kind = get_kind(kind_key)
    if image is not None and not kind.accepts_image:
        raise ValidationError({"image": "To głosowanie nie przyjmuje obrazu."})
    clean_config = kind.validate_config(config)
    kind.validate_proposal(clean_config, creator=creator, has_image=image is not None)
    title = kind.generate_title(clean_config) or title.strip()
    if not title:
        raise ValidationError({"title": "Podaj tytuł głosowania."})
    members = get_user_model().objects.members()
    if kind.everyone_participates:
        users = list(members)
    else:
        wanted = set(participant_ids) | {creator.pk}  # the caller always takes part
        users = list(get_user_model()._default_manager.filter(pk__in=wanted, is_active=True))
        if len(users) != len(wanted):
            raise ValidationError({"participant_ids": "Nieznany lub nieaktywny użytkownik."})

    with transaction.atomic():
        poll = Poll.objects.create(creator=creator, title=title, kind=kind_key, config=clean_config)
        if image is not None:
            poll.attach_proposed_avatar(image)
        PollParticipant.objects.bulk_create(PollParticipant(poll=poll, user=u) for u in users)
        notify_ids = [u.pk for u in users if u.pk != creator.pk]

        def announce() -> None:
            broadcast("poll.created", {"poll_id": poll.pk})
            send_push(
                notify_ids,
                title="Nowe głosowanie w Sejmiku",
                body=poll.title,
                url=f"/voting/{poll.pk}",
            )

        transaction.on_commit(announce)
    return poll


def cast_ballot(poll_id: int, user: Any, ballot: Any) -> Poll:
    with transaction.atomic():
        poll = _locked_open_poll(poll_id)
        participant = _participant(poll, user)
        if participant.vetoed:
            raise ValidationError("Zawetowano już to głosowanie; weta nie można cofnąć.")
        participant.ballot = get_kind(poll.kind).validate_ballot(poll.config, ballot)
        participant.voted_at = timezone.now()
        participant.save(update_fields=["ballot", "voted_at"])
        _after_ballot(poll)
    return poll


def veto(poll_id: int, user: Any) -> Poll:
    with transaction.atomic():
        poll = _locked_open_poll(poll_id)
        kind = get_kind(poll.kind)
        if not kind.allows_veto:
            raise ValidationError("To głosowanie nie podlega wetu.")
        participant = _participant(poll, user)
        participant.ballot = kind.veto_ballot(poll.config)
        participant.vetoed = True
        participant.voted_at = timezone.now()
        participant.save(update_fields=["ballot", "vetoed", "voted_at"])
        _after_ballot(poll)
    return poll


def close_early(poll_id: int, user: Any) -> Poll:
    with transaction.atomic():
        poll = _locked_open_poll(poll_id)
        if poll.creator_id != user.pk:
            raise PermissionDenied("Tylko osoba, która rozpoczęła głosowanie, może je zakończyć.")
        if not get_kind(poll.kind).allows_early_close:
            raise ValidationError("Tego głosowania nie można zakończyć wcześniej.")
        _close(poll, Poll.CloseReason.CREATOR)
    return poll


def send_reaction(poll: Poll, user: Any, emoji: str) -> None:
    if poll.status != Poll.Status.CLOSED:
        raise ValidationError("Reakcje są dostępne dla zakończonych głosowań.")
    broadcast("poll.reaction", {"poll_id": poll.pk, "emoji": emoji, "user_id": user.pk})


def _due_reminder_stage(poll: Poll, now: datetime) -> int:
    """How many reminders should have been sent by `now` (0 to len(REMINDER_OFFSETS))."""
    age = now - poll.created_at
    return sum(1 for offset in REMINDER_OFFSETS if age >= offset)


def _remind(poll: Poll, stage: int) -> None:
    hours_left = round((POLL_DURATION - REMINDER_OFFSETS[stage - 1]).total_seconds() / 3600)
    waiting = list(
        PollParticipant.objects.filter(poll=poll, voted_at__isnull=True).values_list(
            "user_id", flat=True
        )
    )
    transaction.on_commit(
        lambda: send_push(
            waiting,
            title="Głosowanie czeka na Twój głos",
            body=f"„{poll.title}” kończy się za {hours_left} godz.",
            url=f"/voting/{poll.pk}",
        )
    )


def process_deadlines(now: datetime | None = None) -> tuple[int, int]:
    """Close polls past `POLL_DURATION` and send the reminders that are due.

    Safe to run as often as you like. After downtime only the latest due reminder is sent.
    Returns `(closed, reminded)` poll counts.
    """
    now = now or timezone.now()
    closed = reminded = 0
    for poll_id in Poll.objects.filter(status=Poll.Status.OPEN).values_list("pk", flat=True):
        with transaction.atomic():
            poll = Poll.objects.select_for_update().get(pk=poll_id)
            if poll.status != Poll.Status.OPEN:
                continue  # closed while we were looking
            if now >= poll.created_at + POLL_DURATION:
                _close(poll, Poll.CloseReason.EXPIRED)
                closed += 1
                continue
            stage = _due_reminder_stage(poll, now)
            if stage > poll.reminders_sent:
                poll.reminders_sent = stage
                poll.save(update_fields=["reminders_sent"])
                _remind(poll, stage)
                reminded += 1
    return closed, reminded
