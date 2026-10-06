"""All Secret Santa state changes go through here (locked, validated, events after commit)."""

import secrets
from collections.abc import Iterable
from datetime import datetime
from typing import Any

from django.contrib.auth import get_user_model
from django.db import IntegrityError, transaction
from django.utils import timezone
from rest_framework.exceptions import APIException, NotFound, PermissionDenied, ValidationError

from core.events import broadcast, notify_user
from push.sender import send_push
from secretsanta import crypto
from secretsanta.models import SantaAssignment, SantaEvent, SantaGift, SantaHelpRequest

MIN_PARTICIPANTS = 3
MAX_TIERS = 10
MAX_AMOUNT = 100_000
MAX_IDEAS = 5
MAX_IDEA_LENGTH = 200


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


def _per_tier_shifts(size: int) -> list[int]:
    """Offsets along the shuffled circle that give a valid tier: never yourself (0) and never
    a swap with the person who draws you (half the circle)."""
    return [shift for shift in range(1, size) if (2 * shift) % size]


def _draw_per_tier(user_ids: list[int], tier_count: int) -> list[dict[int, int]]:
    """One pairing per tier. Each tier is a different offset around the same shuffled circle,
    so a giver never gets the same victim twice, nobody draws themselves and nobody swaps."""
    order = list(user_ids)
    rng = secrets.SystemRandom()
    rng.shuffle(order)
    shifts = rng.sample(_per_tier_shifts(len(order)), tier_count)
    return [
        {giver: order[(i + shift) % len(order)] for i, giver in enumerate(order)}
        for shift in shifts
    ]


def start_event(
    *,
    creator: Any,
    participant_ids: Iterable[int],
    deadline: datetime,
    tiers: Iterable[int],
    mode: str = SantaEvent.Mode.SINGLE,
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
    per_tier = mode == SantaEvent.Mode.PER_TIER
    if per_tier and len(clean_tiers) > len(_per_tier_shifts(len(users))):
        raise ValidationError(
            {"gift_tiers": "Za mało uczestników, by każda kwota miała innego podopiecznego."}
        )

    try:
        with transaction.atomic():
            if get_active_event() is not None:
                raise Conflict()
            event = SantaEvent.objects.create(
                created_by=creator, deadline=deadline, gift_tiers=clean_tiers, mode=mode
            )
            ids = [u.pk for u in users]
            names = {u.pk: u.username for u in users}
            # (giver, tier index, receiver); tier index is None when one victim gets every tier.
            draws: list[tuple[int, int | None, int]]
            if per_tier:
                draws = [
                    (giver, tier, receiver)
                    for tier, pairing in enumerate(_draw_per_tier(ids, len(clean_tiers)))
                    for giver, receiver in pairing.items()
                ]
            else:
                draws = [(giver, None, receiver) for giver, receiver in _draw(ids).items()]
            # Insert in a fresh random order so row ids say nothing about the draw.
            secrets.SystemRandom().shuffle(draws)
            SantaAssignment.objects.bulk_create(
                SantaAssignment(
                    event=event,
                    giver_id=giver,
                    tier_index=tier,
                    payload=crypto.seal(giver, receiver),
                )
                for giver, tier, receiver in draws
            )
            messages = _start_messages(draws, names, clean_tiers)

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


def _start_messages(
    draws: list[tuple[int, int | None, int]], names: dict[int, str], tiers: list[int]
) -> dict[int, str]:
    if all(tier is None for _, tier, _ in draws):
        return {
            giver: f"Obdarowujesz: {names[receiver]}. Kwoty: {_amounts(tiers)}."
            for giver, _, receiver in draws
        }
    by_giver: dict[int, list[tuple[int, int]]] = {}
    for giver, tier, receiver in draws:
        by_giver.setdefault(giver, []).append((tier or 0, receiver))
    return {
        giver: "Twoi podopieczni: "
        + ", ".join(f"{names[receiver]} ({tiers[tier]} zł)" for tier, receiver in sorted(picks))
        + "."
        for giver, picks in by_giver.items()
    }


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
            if event.mode == SantaEvent.Mode.PER_TIER and len(clean) != len(event.gift_tiers):
                raise ValidationError({"gift_tiers": "Po losowaniu nie można zmienić liczby kwot."})
            tiers_changed = clean != event.gift_tiers
            event.gift_tiers = clean
            fields.append("gift_tiers")
        if fields:
            event.save(update_fields=fields)
        giver_ids = list(event.assignments.values_list("giver_id", flat=True).distinct())
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
        tiers = clean_tiers_of(event)
        SantaGift.objects.bulk_create(
            SantaGift(assignment=a, amount=amount)
            for a in assignments
            for amount in (tiers if a.tier_index is None else [tiers[a.tier_index]])
        )
        event.status = SantaEvent.Status.ENDED
        event.ended_at = timezone.now()
        event.save(update_fields=["status", "ended_at"])
        transaction.on_commit(lambda: broadcast("santa.updated", {"event_id": event.pk}))
    return event


def get_victims(event: SantaEvent, user: Any) -> list[tuple[int | None, Any]]:
    """`(tier index, person)` for everyone `user` gives to in an active event; empty if they
    aren't taking part. The tier index is None when the one victim gets every tier."""
    assignments = list(
        SantaAssignment.objects.filter(event=event, giver=user).order_by("tier_index")
    )
    receiver_ids = {a.pk: crypto.open_seal(user.pk, a.payload) for a in assignments}
    people = get_user_model()._default_manager.in_bulk(receiver_ids.values())
    return [(a.tier_index, people[receiver_ids[a.pk]]) for a in assignments]


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


def _help_target(user: Any, victim_id: int) -> tuple[SantaEvent, int | None]:
    """The locked active event and tier index of `user`'s pairing with `victim_id`. A giver
    never draws the same victim twice, so the victim picks the pairing in either mode."""
    event = _locked_active_event()
    for tier, victim in get_victims(event, user):
        if victim.pk == victim_id:
            return event, tier
    raise ValidationError({"victim_id": "To nie jest Twój podopieczny."})


def ask_for_help(user: Any, victim_id: int) -> SantaHelpRequest:
    """Ask a victim, anonymously, for gift ideas. Asking again is allowed once they answered."""
    with transaction.atomic():
        event, tier = _help_target(user, victim_id)
        now = timezone.now()
        request, created = SantaHelpRequest.objects.select_for_update().get_or_create(
            event=event, receiver_id=victim_id, tier_index=tier, defaults={"asked_at": now}
        )
        if not created:
            if request.pending:
                raise ValidationError("Prośba o pomoc już czeka na odpowiedź.")
            request.asked_at = now
            request.answered_at = None
            request.save(update_fields=["asked_at", "answered_at"])
        tiers = clean_tiers_of(event)
        gift = "prezentem" if tier is None else f"prezentem za {tiers[tier]} zł"

        def announce() -> None:
            notify_user(victim_id, "santa.help", {"event_id": event.pk})
            send_push(
                [victim_id],
                title="Secret Santa — prośba o pomoc",
                body=f"Twój Secret Santa nie wie, co Ci kupić, i prosi o pomoc z {gift}. "
                "Podsuń kilka pomysłów!",
                url="/secret-santa",
            )

        transaction.on_commit(announce)
    return request


def _clean_ideas(ideas: Iterable[str]) -> list[str]:
    clean = [idea.strip() for idea in ideas if idea.strip()]
    if not clean:
        raise ValidationError({"ideas": "Podaj przynajmniej jeden pomysł."})
    if len(clean) > MAX_IDEAS:
        raise ValidationError({"ideas": f"Maksymalnie {MAX_IDEAS} pomysłów."})
    if any(len(idea) > MAX_IDEA_LENGTH for idea in clean):
        raise ValidationError({"ideas": f"Pomysł może mieć najwyżej {MAX_IDEA_LENGTH} znaków."})
    return clean


def _giver_of(event: SantaEvent, receiver_id: int, tier: int | None) -> int:
    """Open the seals of one tier to find who gives to `receiver_id`; used only to notify."""
    for assignment in SantaAssignment.objects.filter(event=event, tier_index=tier):
        if crypto.open_seal(assignment.giver_id, assignment.payload) == receiver_id:
            return assignment.giver_id
    raise NotFound()


def answer_help(request_id: int, user: Any, ideas: Iterable[str]) -> SantaHelpRequest:
    """The victim's gift ideas for an open (or already answered) request about them."""
    clean = _clean_ideas(ideas)
    with transaction.atomic():
        event = _locked_active_event()
        request = (
            SantaHelpRequest.objects.select_for_update()
            .filter(pk=request_id, event=event, receiver=user)
            .first()
        )
        if request is None:  # someone else's request looks the same as a missing one
            raise NotFound()
        request.ideas = clean
        request.answered_at = timezone.now()
        request.save(update_fields=["ideas", "answered_at"])
        giver_id = _giver_of(event, user.pk, request.tier_index)
        name = user.username

        def announce() -> None:
            notify_user(giver_id, "santa.help", {"event_id": event.pk})
            send_push(
                [giver_id],
                title="Secret Santa — są podpowiedzi",
                body=f"Masz nowe pomysły na prezent dla: {name}.",
                url="/secret-santa",
            )

        transaction.on_commit(announce)
    return request


def help_for_giver(event: SantaEvent, user: Any) -> list[tuple[Any, SantaHelpRequest]]:
    """`(victim, request)` for each of `user`'s pairings they have asked about."""
    victims = get_victims(event, user)
    if not victims:
        return []
    by_key = {
        (r.receiver_id, r.tier_index): r
        for r in SantaHelpRequest.objects.filter(
            event=event, receiver_id__in=[victim.pk for _, victim in victims]
        )
    }
    return [
        (victim, by_key[(victim.pk, tier)])
        for tier, victim in victims
        if (victim.pk, tier) in by_key
    ]


def help_for_receiver(event: SantaEvent, user: Any) -> list[SantaHelpRequest]:
    return list(SantaHelpRequest.objects.filter(event=event, receiver=user).order_by("tier_index"))
