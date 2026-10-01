"""All poll state changes go through here (locked, validated, events sent after commit)."""

from collections.abc import Iterable
from typing import Any

from django.contrib.auth import get_user_model
from django.db import transaction
from django.utils import timezone
from rest_framework.exceptions import APIException, PermissionDenied, ValidationError

from push.sender import send_push
from voting.events import broadcast
from voting.kinds import Entry, get_kind
from voting.models import Poll, PollParticipant

REACTION_EMOJI = ("❤️", "🔥", "😭", "👎", "🤣")


class Conflict(APIException):
    status_code = 409
    default_detail = "This vote has already ended."
    default_code = "conflict"


def _locked_open_poll(poll_id: int) -> Poll:
    poll = Poll.objects.select_for_update().get(pk=poll_id)
    if poll.status != Poll.Status.OPEN:
        raise Conflict()
    return poll


def _participant(poll: Poll, user: Any) -> PollParticipant:
    participant = PollParticipant.objects.filter(poll=poll, user=user).first()
    if participant is None:
        raise PermissionDenied("You're not part of this vote.")
    return participant


def _close(poll: Poll, reason: str) -> None:
    kind = get_kind(poll.kind)
    cast = PollParticipant.objects.filter(poll=poll, voted_at__isnull=False)
    entries = [Entry(ballot=p.ballot, vetoed=p.vetoed) for p in cast]
    poll.result = kind.compute_result(poll.config, entries)
    poll.status = Poll.Status.CLOSED
    poll.close_reason = reason
    poll.closed_at = timezone.now()
    poll.save(update_fields=["result", "status", "close_reason", "closed_at"])
    transaction.on_commit(lambda: broadcast("poll.closed", {"poll_id": poll.pk}))


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
) -> Poll:
    kind = get_kind(kind_key)
    clean_config = kind.validate_config(config)
    wanted = set(participant_ids) | {creator.pk}  # the caller always takes part
    users = list(get_user_model()._default_manager.filter(pk__in=wanted, is_active=True))
    if len(users) != len(wanted):
        raise ValidationError({"participant_ids": "Unknown or inactive user."})

    with transaction.atomic():
        poll = Poll.objects.create(creator=creator, title=title, kind=kind_key, config=clean_config)
        PollParticipant.objects.bulk_create(PollParticipant(poll=poll, user=u) for u in users)
        notify_ids = [u.pk for u in users if u.pk != creator.pk]

        def announce() -> None:
            broadcast("poll.created", {"poll_id": poll.pk})
            send_push(notify_ids, title="New vote", body=poll.title, url=f"/voting/{poll.pk}")

        transaction.on_commit(announce)
    return poll


def cast_ballot(poll_id: int, user: Any, ballot: Any) -> Poll:
    with transaction.atomic():
        poll = _locked_open_poll(poll_id)
        participant = _participant(poll, user)
        if participant.vetoed:
            raise ValidationError("You vetoed this vote; a veto can't be undone.")
        participant.ballot = get_kind(poll.kind).validate_ballot(poll.config, ballot)
        participant.voted_at = timezone.now()
        participant.save(update_fields=["ballot", "voted_at"])
        _after_ballot(poll)
    return poll


def veto(poll_id: int, user: Any) -> Poll:
    with transaction.atomic():
        poll = _locked_open_poll(poll_id)
        participant = _participant(poll, user)
        participant.ballot = get_kind(poll.kind).veto_ballot(poll.config)
        participant.vetoed = True
        participant.voted_at = timezone.now()
        participant.save(update_fields=["ballot", "vetoed", "voted_at"])
        _after_ballot(poll)
    return poll


def close_early(poll_id: int, user: Any) -> Poll:
    with transaction.atomic():
        poll = _locked_open_poll(poll_id)
        if poll.creator_id != user.pk:
            raise PermissionDenied("Only the person who called the vote can end it.")
        _close(poll, Poll.CloseReason.CREATOR)
    return poll


def send_reaction(poll: Poll, user: Any, emoji: str) -> None:
    if poll.status != Poll.Status.CLOSED:
        raise ValidationError("Reactions are for finished votes.")
    broadcast("poll.reaction", {"poll_id": poll.pk, "emoji": emoji, "user_id": user.pk})
