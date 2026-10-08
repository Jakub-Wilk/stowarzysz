"""All pact state changes go through here (locked, validated, events sent after commit)."""

from collections.abc import Iterable
from datetime import datetime
from typing import Any

from django.contrib.auth import get_user_model
from django.db import transaction
from django.http import Http404
from django.utils import timezone
from rest_framework.exceptions import APIException, PermissionDenied, ValidationError

from core import events
from ledger.services import format_amount, record_debt
from pacts.kinds import Terms, get_kind
from pacts.models import OutcomeProposal, Pact, PactParticipant
from push.sender import send_push

Role = PactParticipant.Role
State = PactParticipant.State
OPEN_STATES = (State.INVITED, State.REQUESTED, State.ACTIVE)  # a wager that isn't over yet


class Conflict(APIException):
    status_code = 409
    default_detail = "Ten zakład nie jest już aktualny."
    default_code = "conflict"


def _announce(
    pact: Pact, user_ids: Iterable[int], title: str, body: str, *, push_to: Iterable[int] = ()
) -> None:
    """SSE (ids only) to `user_ids` plus a push to `push_to`, both after commit."""
    sse_ids = set(user_ids)
    push_ids = list(push_to)

    def announce() -> None:
        for user_id in sse_ids:
            events.notify_user(user_id, "pact.updated", {"pact_id": pact.pk})
        if push_ids:
            send_push(push_ids, title=title, body=body, url=f"/pacts/{pact.pk}")

    transaction.on_commit(announce)


def _everyone(pact: Pact) -> list[int]:
    return list(pact.participants.values_list("user_id", flat=True))


def _locked_pact(pact_id: int) -> Pact:
    try:
        return Pact.objects.select_for_update().get(pk=pact_id)
    except Pact.DoesNotExist:
        raise Http404 from None


def _participant(pact: Pact, user: Any) -> PactParticipant:
    participant = PactParticipant.objects.filter(pact=pact, user=user).first()
    if participant is None:
        if not pact.is_open:
            raise Http404  # private pacts don't reveal themselves to outsiders
        raise PermissionDenied("Nie bierzesz udziału w tym zakładzie.")
    return participant


def _host(pact: Pact) -> PactParticipant:
    return PactParticipant.objects.get(pact=pact, role=Role.HOST)


def _terms(participant: PactParticipant) -> Terms:
    return Terms(participant.user_id, participant.stake_amount, participant.stake_note)


def create_pact(
    creator: Any,
    *,
    kind_key: str,
    title: str,
    condition: str,
    notes: str = "",
    due_at: datetime | None = None,
    is_open: bool = False,
    config: dict[str, Any] | None = None,
    opponents: list[Terms],
) -> Pact:
    kind = get_kind(kind_key)
    if is_open and not kind.allows_open_join:
        raise ValidationError("Ten rodzaj zakładu nie może być otwarty.")
    if kind.needs_due_date and due_at is None:
        raise ValidationError("Podaj termin.")
    if not opponents and not is_open:
        raise ValidationError("Zaproś przynajmniej jedną osobę albo otwórz zakład dla wszystkich.")
    cleaned = [kind.validate_terms(terms) for terms in opponents]
    ids = [terms.user_id for terms in cleaned]
    if len(set(ids)) != len(ids):
        raise ValidationError("Ta sama osoba jest zaproszona więcej niż raz.")
    if creator.pk in ids:
        raise ValidationError("Nie możesz zaprosić samego siebie.")
    invitees = {u.pk: u for u in get_user_model().objects.members().filter(pk__in=ids)}
    if len(invitees) != len(ids):
        raise ValidationError("Któraś z zaproszonych osób nie istnieje albo jest nieaktywna.")

    with transaction.atomic():
        pact = Pact.objects.create(
            creator=creator,
            kind=kind_key,
            title=title,
            condition=condition,
            notes=notes,
            due_at=due_at,
            is_open=is_open,
            config=kind.validate_config(config or {}),
        )
        PactParticipant.objects.create(pact=pact, user=creator, role=Role.HOST, state=State.ACTIVE)
        PactParticipant.objects.bulk_create(
            PactParticipant(
                pact=pact,
                user=invitees[terms.user_id],
                stake_amount=terms.stake_amount,
                stake_note=terms.stake_note,
            )
            for terms in cleaned
        )
        invited = [terms.user_id for terms in cleaned]
        _announce(
            pact,
            [creator.pk, *invited],
            "Nowy zakład",
            f"{creator.username} zaprasza Cię: {title}",
            push_to=invited,
        )
        if is_open:
            transaction.on_commit(lambda: events.broadcast("pact.created", {"pact_id": pact.pk}))
    return pact


def respond_to_invite(pact_id: int, user: Any, *, accept: bool) -> Pact:
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        participant = _participant(pact, user)
        if pact.status not in (Pact.Status.PROPOSED, Pact.Status.ACTIVE):
            raise Conflict()
        if participant.state != State.INVITED:
            raise Conflict("Odpowiedziałeś(-aś) już na to zaproszenie.")
        participant.state = State.ACTIVE if accept else State.DECLINED
        participant.save(update_fields=["state"])
        if accept and pact.status == Pact.Status.PROPOSED:
            pact.status = Pact.Status.ACTIVE
            pact.save(update_fields=["status"])
        elif not accept:
            _decline_if_abandoned(pact)
        _announce(
            pact,
            _everyone(pact),
            "Zakład: odpowiedź",
            f"{user.username} {'przyjmuje' if accept else 'odrzuca'} zakład: {pact.title}",
            push_to=[pact.creator_id],
        )
    return pact


def _decline_if_abandoned(pact: Pact) -> None:
    """A pact nobody joined (everyone declined, none open to join) is over."""
    if pact.status != Pact.Status.PROPOSED or pact.is_open:
        return
    waiting = pact.participants.filter(role=Role.OPPONENT, state__in=OPEN_STATES).exists()
    if not waiting:
        pact.status = Pact.Status.DECLINED
        pact.save(update_fields=["status"])


def cancel_pact(pact_id: int, user: Any) -> Pact:
    """The creator calls off a pact that hasn't started."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        if pact.creator_id != user.pk:
            raise PermissionDenied("Tylko autor może anulować zakład.")
        if pact.status != Pact.Status.PROPOSED:
            raise Conflict("Zakład już się rozpoczął.")
        pact.status = Pact.Status.CANCELLED
        pact.save(update_fields=["status"])
        _announce(
            pact,
            _everyone(pact),
            "Zakład anulowany",
            f"{user.username} anuluje zakład: {pact.title}",
            push_to=[uid for uid in _everyone(pact) if uid != user.pk],
        )
    return pact


def _wager_for(pact: Pact, wager_id: int) -> PactParticipant:
    wager = PactParticipant.objects.filter(pk=wager_id, pact=pact, role=Role.OPPONENT).first()
    if wager is None:
        raise ValidationError("Nie ma takiego zakładu w tym pakcie.")
    return wager


def propose_outcome(
    pact_id: int, user: Any, *, wager_id: int, result: dict[str, Any]
) -> OutcomeProposal:
    """Either side of a wager says how it ended; the other side then confirms or disputes."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        if pact.status not in (Pact.Status.ACTIVE, Pact.Status.AWAITING_RESULT):
            raise Conflict()
        wager = _wager_for(pact, wager_id)
        if user.pk not in (pact.creator_id, wager.user_id):
            raise PermissionDenied("Tylko strony zakładu mogą podać wynik.")
        if wager.state != State.ACTIVE:
            raise Conflict("Ten zakład nie jest aktywny.")
        cleaned = get_kind(pact.kind).validate_outcome(result)
        wager.proposals.filter(state=OutcomeProposal.State.PENDING).update(
            state=OutcomeProposal.State.SUPERSEDED
        )
        proposal = OutcomeProposal.objects.create(
            pact=pact, wager=wager, proposed_by=user, result=cleaned
        )
        other = wager.user_id if user.pk == pact.creator_id else pact.creator_id
        _announce(
            pact,
            [pact.creator_id, wager.user_id],
            "Potwierdź wynik zakładu",
            f"{user.username} podaje wynik: {pact.title}",
            push_to=[other],
        )
    return proposal


def _locked_pending_proposal(proposal_id: int, user: Any) -> tuple[Pact, OutcomeProposal]:
    proposal = OutcomeProposal.objects.select_related("wager").filter(pk=proposal_id).first()
    if proposal is None:
        raise Http404
    pact = _locked_pact(proposal.pact_id)
    proposal.refresh_from_db()
    if proposal.state != OutcomeProposal.State.PENDING:
        raise Conflict("Ten wynik nie czeka już na potwierdzenie.")
    if user.pk not in (pact.creator_id, proposal.wager.user_id):
        raise PermissionDenied("Tylko strony zakładu mogą potwierdzić wynik.")
    if user.pk == proposal.proposed_by_id:
        raise PermissionDenied("Wynik musi potwierdzić druga strona.")
    return pact, proposal


def confirm_outcome(proposal_id: int, user: Any) -> OutcomeProposal:
    """The other side agrees: settle this wager and record any money owed in the ledger."""
    with transaction.atomic():
        pact, proposal = _locked_pending_proposal(proposal_id, user)
        wager = PactParticipant.objects.select_for_update().get(pk=proposal.wager_id)
        if wager.state != State.ACTIVE:
            raise Conflict("Ten zakład nie jest aktywny.")
        now = timezone.now()
        proposal.state = OutcomeProposal.State.CONFIRMED
        proposal.decided_at = now
        proposal.save(update_fields=["state", "decided_at"])
        wager.state = State.SETTLED
        wager.save(update_fields=["state"])

        kind = get_kind(pact.kind)
        debts = kind.settle(
            host_id=pact.creator_id,
            wager_user_id=wager.user_id,
            terms=_terms(wager),
            result=proposal.result,
        )
        users = get_user_model().objects.in_bulk(
            [d.debtor_id for d in debts] + [d.creditor_id for d in debts]
        )
        for debt in debts:
            record_debt(
                users[debt.debtor_id],
                users[debt.creditor_id],
                debt.amount,
                pact.title,
                source_type="pact",
                source_id=pact.pk,
            )
        _resolve_if_finished(pact)
        summary = ", ".join(format_amount(d.amount) for d in debts) or "bez rozliczenia"
        _announce(
            pact,
            [pact.creator_id, wager.user_id],
            "Zakład rozstrzygnięty",
            f"{pact.title}: {summary}",
            push_to=[proposal.proposed_by_id],
        )
    return proposal


def _resolve_if_finished(pact: Pact) -> None:
    if pact.participants.filter(role=Role.OPPONENT, state__in=OPEN_STATES).exists():
        return
    settled = PactParticipant.objects.filter(pact=pact, state=State.SETTLED)
    pact.status = Pact.Status.RESOLVED
    pact.resolved_at = timezone.now()
    pact.outcome = {
        "wagers": [
            {"participant_id": p.pk, "result": p.proposals.get(state="confirmed").result}
            for p in settled
        ]
    }
    pact.save(update_fields=["status", "resolved_at", "outcome"])


def dispute_outcome(proposal_id: int, user: Any) -> OutcomeProposal:
    """The other side disagrees. The wager stays active; either side can propose again."""
    with transaction.atomic():
        pact, proposal = _locked_pending_proposal(proposal_id, user)
        proposal.state = OutcomeProposal.State.DISPUTED
        proposal.decided_at = timezone.now()
        proposal.save(update_fields=["state", "decided_at"])
        _announce(
            pact,
            [pact.creator_id, proposal.wager.user_id],
            "Wynik zakładu sporny",
            f"{user.username} nie zgadza się z wynikiem: {pact.title}",
            push_to=[proposal.proposed_by_id],
        )
    return proposal
