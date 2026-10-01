"""All Secret Santa state changes go through here (locked, validated, events after commit)."""

import secrets
from collections.abc import Iterable
from datetime import datetime
from typing import Any

from django.contrib.auth import get_user_model
from django.db import IntegrityError, transaction
from django.utils import timezone
from rest_framework.exceptions import APIException, NotFound, PermissionDenied, ValidationError

from core.events import broadcast
from push.sender import send_push
from secretsanta import crypto
from secretsanta.models import SantaAssignment, SantaEvent, SantaGift

MIN_PARTICIPANTS = 3
MAX_TIERS = 10
MAX_AMOUNT = 100_000


class Conflict(APIException):
    status_code = 409
    default_detail = "Secret Santa już trwa."
    default_code = "conflict"


def get_active_event() -> SantaEvent | None:
    return SantaEvent.objects.filter(status=SantaEvent.Status.ACTIVE).first()


def _locked_active_event() -> SantaEvent:
    event = SantaEvent.objects.select_for_update().filter(status=SantaEvent.Status.ACTIVE).first()
    if event is None:
        raise NotFound("Secret Santa nie jest aktywny.")
    return event


def _clean_tiers(tiers: Iterable[int]) -> list[int]:
    clean = sorted(set(tiers), reverse=True)
    if not clean:
        raise ValidationError({"gift_tiers": "Dodaj przynajmniej jedną kwotę."})
    if len(clean) > MAX_TIERS:
        raise ValidationError({"gift_tiers": f"Maksymalnie {MAX_TIERS} kwot."})
    if any(not 0 < amount <= MAX_AMOUNT for amount in clean):
        raise ValidationError({"gift_tiers": "Kwoty muszą być dodatnie i rozsądne."})
    return clean


def _clean_deadline(deadline: datetime) -> datetime:
    if deadline <= timezone.now():
        raise ValidationError({"deadline": "Termin musi być w przyszłości."})
    return deadline


def clean_tiers_of(event: SantaEvent) -> list[int]:
    return list(event.gift_tiers)


def _amounts(tiers: Iterable[int]) -> str:
    return ", ".join(f"{amount} zł" for amount in tiers)


def _draw(user_ids: list[int]) -> dict[int, int]:
    """One big cycle over a shuffled list: nobody draws themselves and, with three or more
    people, nobody draws the person who drew them."""
    order = list(user_ids)
    secrets.SystemRandom().shuffle(order)
    return {giver: order[(i + 1) % len(order)] for i, giver in enumerate(order)}


def start_event(
    *, creator: Any, participant_ids: Iterable[int], deadline: datetime, tiers: Iterable[int]
) -> SantaEvent:
    wanted = set(participant_ids)
    clean_tiers = _clean_tiers(tiers)
    _clean_deadline(deadline)
    users = list(get_user_model()._default_manager.filter(pk__in=wanted, is_active=True))
    if len(users) != len(wanted):
        raise ValidationError({"participant_ids": "Nieznany lub nieaktywny użytkownik."})
    if len(users) < MIN_PARTICIPANTS:
        raise ValidationError(
            {"participant_ids": f"Potrzeba co najmniej {MIN_PARTICIPANTS} uczestników."}
        )

    try:
        with transaction.atomic():
            if get_active_event() is not None:
                raise Conflict()
            event = SantaEvent.objects.create(
                created_by=creator, deadline=deadline, gift_tiers=clean_tiers
            )
            pairing = _draw([u.pk for u in users])
            names = {u.pk: u.username for u in users}
            # Insert in a fresh random order so row ids say nothing about the draw.
            givers = list(pairing)
            secrets.SystemRandom().shuffle(givers)
            SantaAssignment.objects.bulk_create(
                SantaAssignment(
                    event=event, giver_id=giver, payload=crypto.seal(giver, pairing[giver])
                )
                for giver in givers
            )
            messages = {
                giver: f"Obdarowujesz: {names[receiver]}. Kwoty: {_amounts(clean_tiers)}."
                for giver, receiver in pairing.items()
            }

            def announce() -> None:
                broadcast("santa.updated", {"event_id": event.pk})
                for user_id, body in messages.items():
                    send_push(
                        [user_id],
                        title="Secret Santa — wylosowano!",
                        body=body,
                        url="/secret-santa",
                    )

            transaction.on_commit(announce)
    except IntegrityError as exc:  # lost a race with another start
        raise Conflict() from exc
    return event


def update_event(*, deadline: datetime | None, tiers: Iterable[int] | None) -> SantaEvent:
    with transaction.atomic():
        event = _locked_active_event()
        fields = []
        tiers_changed = False
        if deadline is not None:
            event.deadline = deadline
            fields.append("deadline")
        if tiers is not None:
            clean = _clean_tiers(tiers)
            tiers_changed = clean != event.gift_tiers
            event.gift_tiers = clean
            fields.append("gift_tiers")
        if fields:
            event.save(update_fields=fields)
        giver_ids = list(event.assignments.values_list("giver_id", flat=True))
        tier_text = _amounts(event.gift_tiers)

        def announce() -> None:
            broadcast("santa.updated", {"event_id": event.pk})
            if tiers_changed:
                send_push(
                    giver_ids,
                    title="Secret Santa — zmiana kwot",
                    body=f"Nowe kwoty: {tier_text}.",
                    url="/secret-santa",
                )

        transaction.on_commit(announce)
    return event


def end_event() -> SantaEvent:
    """Reveal the pairings and keep the event as history."""
    with transaction.atomic():
        event = _locked_active_event()
        assignments = list(event.assignments.all())
        for assignment in assignments:
            assignment.receiver_id = crypto.open_seal(assignment.giver_id, assignment.payload)
            assignment.payload = ""
        SantaAssignment.objects.bulk_update(assignments, ["receiver", "payload"])
        SantaGift.objects.bulk_create(
            SantaGift(assignment=a, amount=amount)
            for a in assignments
            for amount in clean_tiers_of(event)
        )
        event.status = SantaEvent.Status.ENDED
        event.ended_at = timezone.now()
        event.save(update_fields=["status", "ended_at"])
        transaction.on_commit(lambda: broadcast("santa.updated", {"event_id": event.pk}))
    return event


def get_victim(event: SantaEvent, user: Any) -> Any | None:
    """The person `user` gives to in an active event, or None if they aren't taking part."""
    assignment = SantaAssignment.objects.filter(event=event, giver=user).first()
    if assignment is None:
        return None
    receiver_id = crypto.open_seal(user.pk, assignment.payload)
    return get_user_model()._default_manager.get(pk=receiver_id)


def set_gift_note(gift_id: int, user: Any, note: str) -> SantaGift:
    with transaction.atomic():
        gift = (
            SantaGift.objects.select_for_update(of=("self",))
            .select_related("assignment__event")
            .filter(pk=gift_id)
            .first()
        )
        if gift is None:
            raise NotFound()
        if gift.assignment.event.status != SantaEvent.Status.ENDED:
            raise ValidationError("Notatki można dodawać po zakończeniu Secret Santa.")
        if gift.assignment.giver_id != user.pk and not user.is_superuser:
            raise PermissionDenied("Możesz edytować tylko własne prezenty.")
        gift.note = note
        gift.save(update_fields=["note"])
        event_id = gift.assignment.event_id
        transaction.on_commit(lambda: broadcast("santa.updated", {"event_id": event_id}))
    return gift
