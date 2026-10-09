"""All pact state changes go through here (locked, validated, events sent after commit)."""

from collections.abc import Iterable
from datetime import datetime, timedelta
from typing import Any
from uuid import uuid4

from django.contrib.auth import get_user_model
from django.db import transaction
from django.http import Http404
from django.utils import timezone
from rest_framework.exceptions import APIException, PermissionDenied, ValidationError

from accounts.avatars import process_photo
from core import events
from ledger.money import format_amount
from ledger.services import record_debts
from pacts.kinds import Party, Terms, get_kind
from pacts.models import (
    JoinConsent,
    OutcomeConfirmation,
    OutcomeProposal,
    Pact,
    PactAttachment,
    PactParticipant,
)
from push.sender import send_push
from voting import services as voting_services

Role = PactParticipant.Role
State = PactParticipant.State
Proposal = OutcomeProposal.State

BLOCKING_STATES = (State.INVITED, State.ACTIVE)  # a wager that isn't over yet
INVITE_TTL = timedelta(days=7)  # unanswered invites and join requests expire after this
NUDGE_EVERY = timedelta(days=7)  # how often parties are reminded to settle an overdue pact
VOID = {"void": True}  # a claim result meaning "call it off"


class Conflict(APIException):
    status_code = 409
    default_detail = "Ten zakład nie jest już aktualny."
    default_code = "conflict"


def _announce(
    pact: Pact, title: str, body: str, *, push_to: Iterable[int] = (), event: str = "pact.updated"
) -> None:
    """Tell every member's SSE stream (ids only, so clients refetch) and push to `push_to`,
    both after commit. Pacts are readable by everyone, so everyone's view can change."""
    push_ids = list(push_to)

    def announce() -> None:
        events.broadcast(event, {"pact_id": pact.pk})
        if push_ids:
            send_push(push_ids, title=title, body=body, url=f"/pacts/{pact.pk}")

    transaction.on_commit(announce)


def _everyone(pact: Pact) -> list[int]:
    return list(pact.participants.values_list("user_id", flat=True))


def _active_ids(pact: Pact) -> list[int]:
    return list(pact.participants.filter(state=State.ACTIVE).values_list("user_id", flat=True))


def _locked_pact(pact_id: int) -> Pact:
    try:
        return Pact.objects.select_for_update().get(pk=pact_id)
    except Pact.DoesNotExist:
        raise Http404 from None


def _participant(pact: Pact, user: Any) -> PactParticipant:
    participant = PactParticipant.objects.filter(pact=pact, user=user).first()
    if participant is None:
        raise PermissionDenied("Nie bierzesz udziału w tym zakładzie.")
    return participant


def _terms(participant: PactParticipant) -> Terms:
    return Terms(
        participant.user_id, participant.stake_amount, participant.stake_note, participant.side
    )


def _party(participant: PactParticipant) -> Party:
    return Party(
        participant.user_id,
        participant.role == Role.HOST,
        participant.side,
        participant.stake_amount,
        participant.stake_note,
    )


def _no_claim_in_flight(pact: Pact) -> None:
    """Whole-pact kinds can't change who is in them while a claim is being decided (the claim's
    confirmers and payouts would shift under it)."""
    if get_kind(pact.kind).wager_based:
        return
    if (
        pact.proposals.filter(state=Proposal.PENDING).exists()
        or pact.proposals.filter(state=Proposal.DISPUTED, ruling_poll__status="open").exists()
    ):
        raise Conflict("Trwa rozstrzyganie wyniku, poczekaj na jego zakończenie.")


def result_allowed(pact: Pact, now: datetime | None = None) -> bool:
    """With a deadline, nobody can call the result before it has passed (only call it off)."""
    return pact.due_at is None or pact.due_at <= (now or timezone.now())


def create_pact(
    creator: Any,
    *,
    kind_key: str,
    title: str,
    condition: str,
    notes: str = "",
    due_at: datetime | None = None,
    config: dict[str, Any] | None = None,
    host: Terms | None = None,
    opponents: list[Terms],
    backfill: bool = False,
) -> Pact:
    """Create a pact for `creator`.

    TEMPORARY `backfill` (admins entering old pacts for someone): the deadline may be in the
    past and the invitees are already in with their terms, so the pact starts at once and nobody
    is notified. Remove together with `creator_id` on the create endpoint.
    """
    kind = get_kind(kind_key)
    if kind.judged_by_everyone and opponents:
        raise ValidationError(
            "Tego rodzaju zakładu oceniają wszyscy posłowie, nie zapraszaj nikogo."
        )
    if kind.needs_due_date and due_at is None:
        raise ValidationError("Podaj termin.")
    if due_at is not None and due_at <= timezone.now() and not backfill:
        raise ValidationError("Termin musi być w przyszłości.")
    clean_config = kind.validate_config(config or {})
    host_terms = kind.validate_terms(host or Terms(creator.pk), clean_config, host=True, final=True)
    cleaned = [kind.validate_terms(t, clean_config, host=False, final=backfill) for t in opponents]
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
            status=Pact.Status.ACTIVE if kind.judged_by_everyone else Pact.Status.PROPOSED,
            config=clean_config,
        )
        PactParticipant.objects.create(
            pact=pact,
            user=creator,
            role=Role.HOST,
            state=State.ACTIVE,
            side=host_terms.side,
            stake_amount=host_terms.stake_amount,
            stake_note=host_terms.stake_note,
        )
        PactParticipant.objects.bulk_create(
            PactParticipant(
                pact=pact,
                user=invitees[terms.user_id],
                stake_amount=terms.stake_amount,
                stake_note=terms.stake_note,
                side=terms.side,
                state=State.ACTIVE if backfill else State.INVITED,
            )
            for terms in cleaned
        )
        if backfill:
            _refresh_status(pact)
        _announce(
            pact,
            "Nowy zakład",
            f"{creator.username} zaprasza Cię: {title}",
            push_to=[] if backfill else ids,
            event="pact.created",
        )
    return pact


def _refresh_status(pact: Pact) -> None:
    """Start a pact that has what it needs, or drop one nobody is left in."""
    if pact.status != Pact.Status.PROPOSED:
        return
    kind = get_kind(pact.kind)
    others = pact.participants.filter(role=Role.OPPONENT)
    has_active = others.filter(state=State.ACTIVE).exists()
    waiting_invites = others.filter(state=State.INVITED).exists()
    if has_active and not (kind.all_must_answer and waiting_invites):
        pact.status = Pact.Status.ACTIVE
        pact.save(update_fields=["status"])


def respond_to_invite(
    pact_id: int,
    user: Any,
    *,
    accept: bool,
    side: str = "",
    stake_amount: int | None = None,
    stake_note: str | None = None,
) -> Pact:
    """Accept or decline an invite. Kinds where invitees choose a side/stake take them here."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        participant = _participant(pact, user)
        kind = get_kind(pact.kind)
        if pact.status not in (Pact.Status.PROPOSED, Pact.Status.ACTIVE):
            raise Conflict()
        if participant.state != State.INVITED:
            raise Conflict("Odpowiedziałeś(-aś) już na to zaproszenie.")
        _no_claim_in_flight(pact)
        if accept:
            if not kind.wager_based:  # in a bet the host already fixed each opponent's stake
                merged = Terms(
                    user.pk,
                    stake_amount if stake_amount is not None else participant.stake_amount,
                    participant.stake_note if stake_note is None else stake_note,
                    side or participant.side,
                )
                final = kind.validate_terms(merged, pact.config, host=False, final=True)
                participant.side = final.side
                participant.stake_amount = final.stake_amount
                participant.stake_note = final.stake_note
            participant.state = State.ACTIVE
        else:
            participant.state = State.DECLINED
        participant.save()
        _refresh_status(pact)
        _announce(
            pact,
            "Zakład: odpowiedź",
            f"{user.username} {'przyjmuje' if accept else 'odrzuca'} zakład: {pact.title}",
            push_to=[pact.creator_id],
        )
    return pact


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
            "Zakład anulowany",
            f"{user.username} anuluje zakład: {pact.title}",
            push_to=[uid for uid in _everyone(pact) if uid != user.pk],
        )
    return pact


# --- attachments ---------------------------------------------------------------

MAX_ATTACHMENTS = 10  # per pact
ATTACH_STATES = (State.INVITED, State.REQUESTED, State.ACTIVE, State.SETTLED, State.VOID)


def add_attachment(pact_id: int, user: Any, upload: Any, caption: str = "") -> PactAttachment:
    """Attach a picture to a pact's notes. Any participant can, at any stage (e.g. as proof)."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        if not pact.participants.filter(user=user, state__in=ATTACH_STATES).exists():
            raise PermissionDenied("Tylko uczestnicy mogą dodawać zdjęcia do zakładu.")
        if pact.attachments.count() >= MAX_ATTACHMENTS:
            raise ValidationError(f"Do zakładu można dodać maksymalnie {MAX_ATTACHMENTS} zdjęć.")
        attachment = PactAttachment(pact=pact, uploaded_by=user, caption=caption.strip())
        attachment.image.save(f"{uuid4().hex}.webp", process_photo(upload), save=False)
        attachment.save()
        _announce(pact, "", "")
    return attachment


def delete_attachment(pact_id: int, attachment_id: int, user: Any) -> None:
    """The uploader or the pact's creator can take a picture down."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        attachment = pact.attachments.filter(pk=attachment_id).first()
        if attachment is None:
            raise Http404
        if user.pk not in (attachment.uploaded_by_id, pact.creator_id):
            raise PermissionDenied("Zdjęcie może usunąć jego autor albo autor zakładu.")
        attachment.delete()
        _announce(pact, "", "")


# --- joining -----------------------------------------------------------------


def request_to_join(pact_id: int, user: Any, terms: Terms) -> PactParticipant:
    """Ask to join a pact. The kind decides who has to approve."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        kind = get_kind(pact.kind)
        if not kind.joinable:
            raise Conflict("Do tego rodzaju zakładu nie można dołączać.")
        if pact.status not in (Pact.Status.PROPOSED, Pact.Status.ACTIVE) or (
            pact.due_at is not None and pact.due_at <= timezone.now()
        ):
            raise Conflict("Do tego zakładu nie można już dołączyć.")
        _no_claim_in_flight(pact)
        final = kind.validate_terms(
            Terms(user.pk, terms.stake_amount, terms.stake_note, terms.side),
            pact.config,
            host=False,
            final=True,
        )
        participant = PactParticipant.objects.filter(pact=pact, user=user).first()
        retry = (State.DECLINED, State.REJECTED, State.WITHDRAWN, State.EXPIRED)
        if participant is not None and participant.state not in retry:
            raise Conflict("Już bierzesz udział w tym zakładzie albo czekasz na decyzję.")
        if participant is None:
            participant = PactParticipant(pact=pact, user=user)
        else:
            participant.consents.all().delete()
        participant.role = Role.OPPONENT
        participant.state = State.REQUESTED
        participant.side = final.side
        participant.stake_amount = final.stake_amount
        participant.stake_note = final.stake_note
        participant.created_at = timezone.now()
        participant.save()
        approvers = kind.join_approvers(pact.creator_id, _active_ids(pact))
        JoinConsent.objects.bulk_create(
            JoinConsent(participant=participant, approver_id=uid) for uid in approvers
        )
        _announce(
            pact,
            "Prośba o dołączenie",
            f"{user.username} chce dołączyć do zakładu: {pact.title}",
            push_to=approvers,
        )
    return participant


def decide_join(pact_id: int, user: Any, participant_id: int, *, approve: bool) -> PactParticipant:
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        requester = PactParticipant.objects.filter(
            pk=participant_id, pact=pact, state=State.REQUESTED
        ).first()
        if requester is None:
            raise Conflict("Ta prośba nie czeka już na decyzję.")
        consent = JoinConsent.objects.filter(participant=requester, approver=user).first()
        if consent is None:
            raise PermissionDenied("Nie decydujesz o tej prośbie.")
        if consent.approved is not None:
            raise Conflict("Podjąłeś(-ęłaś) już decyzję w tej sprawie.")
        _no_claim_in_flight(pact)
        consent.approved = approve
        consent.save(update_fields=["approved"])
        if not approve:
            requester.state = State.REJECTED
            requester.save(update_fields=["state"])
            _announce(
                pact,
                "Prośba odrzucona",
                f"{user.username} odrzuca Twoją prośbę o dołączenie: {pact.title}",
                push_to=[requester.user_id],
            )
        elif not requester.consents.filter(approved__isnull=True).exists():
            requester.state = State.ACTIVE
            requester.save(update_fields=["state"])
            _refresh_status(pact)
            _announce(
                pact,
                "Dołączono do zakładu",
                f"{requester.user.username} dołącza do zakładu: {pact.title}",
                push_to=[requester.user_id],
            )
        else:
            _announce(pact, "", "")
    return requester


def withdraw_request(pact_id: int, user: Any) -> PactParticipant:
    """Take back a join request that hasn't been decided."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        participant = _participant(pact, user)
        if participant.state != State.REQUESTED:
            raise Conflict("Nie masz oczekującej prośby w tym zakładzie.")
        participant.state = State.WITHDRAWN
        participant.save(update_fields=["state"])
        participant.consents.all().delete()
        _announce(pact, "", "")
    return participant


# --- outcomes ----------------------------------------------------------------


def _confirmers(pact: Pact, proposal: OutcomeProposal) -> set[int]:
    """Whose agreement a claim needs: the other side of a wager, or everyone else in the pact."""
    if proposal.wager is not None:
        parties = {pact.creator_id, proposal.wager.user_id}
    else:
        parties = set(_active_ids(pact))
    return parties - {proposal.proposed_by_id}


def _ruling_open(proposals: Any) -> bool:
    return proposals.filter(state=Proposal.DISPUTED, ruling_poll__status="open").exists()


def propose_outcome(
    pact_id: int,
    user: Any,
    *,
    wager_id: int | None = None,
    result: dict[str, Any],
) -> OutcomeProposal:
    """Someone says how a wager (or the whole pact) ended; the other side(s) then confirm.

    A result of `{"void": true}` proposes calling it off instead."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        kind = get_kind(pact.kind)
        if pact.status not in (Pact.Status.ACTIVE, Pact.Status.AWAITING_RESULT):
            raise Conflict()
        if kind.judged_by_everyone:
            raise Conflict("To postanowienie oceniają wszyscy posłowie w głosowaniu w Sejmiku.")
        wager = None
        if kind.wager_based:
            wager = PactParticipant.objects.filter(
                pk=wager_id, pact=pact, role=Role.OPPONENT
            ).first()
            if wager is None:
                raise ValidationError("Wskaż zakład, którego dotyczy wynik (wager_id).")
            if user.pk not in (pact.creator_id, wager.user_id):
                raise PermissionDenied("Tylko strony zakładu mogą podać wynik.")
            if wager.state != State.ACTIVE:
                raise Conflict("Ten zakład nie jest aktywny.")
        elif user.pk not in _active_ids(pact):
            raise PermissionDenied("Tylko uczestnicy mogą podać wynik.")
        scope = pact.proposals.filter(wager=wager)
        if _ruling_open(scope):
            raise Conflict("Trwa głosowanie w sprawie spornego wyniku.")
        if result.get("void") is True:
            cleaned = dict(VOID)  # calling it off is always possible
        elif not result_allowed(pact):
            raise Conflict(
                "Wynik można podać dopiero po upływie terminu. Wcześniej można tylko unieważnić."
            )
        else:
            cleaned = kind.validate_outcome(result, pact.config)
        scope.filter(state__in=(Proposal.PENDING, Proposal.DISPUTED)).update(
            state=Proposal.SUPERSEDED
        )
        proposal = OutcomeProposal.objects.create(
            pact=pact, wager=wager, proposed_by=user, result=cleaned
        )
        confirmers = _confirmers(pact, proposal)
        _announce(
            pact,
            "Potwierdź wynik zakładu",
            f"{user.username} podaje wynik: {pact.title}",
            push_to=confirmers,
        )
    return proposal


def _locked_pending_proposal(proposal_id: int, user: Any) -> tuple[Pact, OutcomeProposal, set[int]]:
    proposal = OutcomeProposal.objects.select_related("wager").filter(pk=proposal_id).first()
    if proposal is None:
        raise Http404
    pact = _locked_pact(proposal.pact_id)
    proposal.refresh_from_db()
    if proposal.state != Proposal.PENDING:
        raise Conflict("Ten wynik nie czeka już na potwierdzenie.")
    if user.pk == proposal.proposed_by_id:
        raise PermissionDenied("Wynik musi potwierdzić druga strona.")
    confirmers = _confirmers(pact, proposal)
    if user.pk not in confirmers:
        raise PermissionDenied("Tylko strony zakładu mogą potwierdzić wynik.")
    return pact, proposal, confirmers


def confirm_outcome(proposal_id: int, user: Any) -> OutcomeProposal:
    """A required party agrees. Once all of them have, the claim is settled."""
    with transaction.atomic():
        pact, proposal, confirmers = _locked_pending_proposal(proposal_id, user)
        _, created = OutcomeConfirmation.objects.get_or_create(proposal=proposal, user=user)
        if not created:
            raise Conflict("Potwierdziłeś(-aś) już ten wynik.")
        agreed = set(proposal.confirmations.values_list("user_id", flat=True))
        if confirmers <= agreed:
            _settle(pact, proposal)
        else:
            _announce(pact, "", "")
    return proposal


def dispute_outcome(proposal_id: int, user: Any) -> OutcomeProposal:
    """A required party disagrees. The claim is parked: propose again or escalate it."""
    with transaction.atomic():
        pact, proposal, _ = _locked_pending_proposal(proposal_id, user)
        proposal.state = Proposal.DISPUTED
        proposal.decided_at = timezone.now()
        proposal.save(update_fields=["state", "decided_at"])
        _announce(
            pact,
            "Wynik zakładu sporny",
            f"{user.username} nie zgadza się z wynikiem: {pact.title}",
            push_to=[proposal.proposed_by_id],
        )
    return proposal


def _settle(pact: Pact, proposal: OutcomeProposal) -> None:
    """Apply a claim everyone agreed to (or a ruling upheld): states, debts, resolution."""
    kind = get_kind(pact.kind)
    now = timezone.now()
    wager = proposal.wager
    rows = list(PactParticipant.objects.select_for_update().filter(pact=pact))
    if wager is not None:
        affected = [r for r in rows if r.pk == wager.pk or r.role == Role.HOST]
        ended = [r for r in rows if r.pk == wager.pk]
    else:
        affected = ended = [r for r in rows if r.state == State.ACTIVE]
    if any(r.state != State.ACTIVE for r in ended):
        raise Conflict("Ten zakład nie jest już aktywny.")

    void = proposal.result.get("void") is True
    verdicts: dict[int, str] = {}
    debts = []
    if void:
        verdicts = {r.user_id: "void" for r in affected}
    else:
        settlement = kind.settle([_party(r) for r in affected], proposal.result, pact.config)
        verdicts, debts = settlement.verdicts, settlement.debts
    record_debts(debts, title=pact.title, source_type="pact", source_id=pact.pk)
    proposal.state = Proposal.CONFIRMED
    proposal.decided_at = now
    proposal.verdicts = {str(uid): v for uid, v in verdicts.items()}
    proposal.save(update_fields=["state", "decided_at", "verdicts"])
    end_state = State.VOID if void else State.SETTLED
    for row in ended:
        row.state = end_state
        row.save(update_fields=["state"])
    _resolve_if_finished(pact, void=void)
    summary = ", ".join(format_amount(d.amount, item=d.item) for d in debts) or "bez rozliczenia"
    _announce(
        pact,
        "Zakład rozstrzygnięty" if not void else "Zakład unieważniony",
        f"{pact.title}: {summary}",
        push_to=[proposal.proposed_by_id],
    )


def _resolve_if_finished(pact: Pact, *, void: bool) -> None:
    kind = get_kind(pact.kind)
    if (
        kind.wager_based
        and pact.participants.filter(role=Role.OPPONENT, state__in=BLOCKING_STATES).exists()
    ):
        return  # other wagers are still running
    # whoever never got an answer is out
    pact.participants.filter(state__in=(State.INVITED, State.REQUESTED)).update(
        state=State.WITHDRAWN
    )
    JoinConsent.objects.filter(participant__pact=pact, approved__isnull=True).delete()
    confirmed = list(pact.proposals.filter(state=Proposal.CONFIRMED).order_by("id"))
    if kind.wager_based:
        outcome: dict[str, Any] = {
            "wagers": [{"participant_id": p.wager_id, "result": p.result} for p in confirmed]
        }
        void = all(p.result.get("void") for p in confirmed)
    else:
        outcome = {"result": confirmed[-1].result}
    pact.status = Pact.Status.VOID if void else Pact.Status.RESOLVED
    pact.resolved_at = timezone.now()
    pact.outcome = outcome
    pact.save(update_fields=["status", "resolved_at", "outcome"])


# --- disputes: Sejmik rulings --------------------------------------------------


def escalate_dispute(proposal_id: int, user: Any) -> OutcomeProposal:
    """Put a disputed claim to a vote of the members who aren't part of the pact."""
    with transaction.atomic():
        proposal = OutcomeProposal.objects.select_related("wager").filter(pk=proposal_id).first()
        if proposal is None:
            raise Http404
        pact = _locked_pact(proposal.pact_id)
        proposal.refresh_from_db()
        parties = _confirmers(pact, proposal) | {proposal.proposed_by_id}
        if user.pk not in parties:
            raise PermissionDenied("Tylko strony sporu mogą oddać go pod głosowanie.")
        if proposal.state != Proposal.DISPUTED or proposal.ruling_poll_id is not None:
            raise Conflict("Ten wynik nie jest sporny albo już trafił pod głosowanie.")
        everyone_in_pact = set(_everyone(pact))
        jurors = get_user_model().objects.members().exclude(pk__in=everyone_in_pact)
        if not jurors.exists():
            raise ValidationError("Nie ma bezstronnych posłów, którzy mogliby rozstrzygnąć spór.")
        poll = voting_services.create_poll(
            creator=user,
            title="",
            kind_key="pact_ruling",
            config={"proposal_id": proposal.pk},
            participant_ids=[],
        )
        proposal.ruling_poll = poll
        proposal.save(update_fields=["ruling_poll"])
        _announce(pact, "", "")
    return proposal


def apply_ruling(proposal_id: int, *, upheld: bool) -> dict[str, Any]:
    """Called when a `pact_ruling` vote closes: settle the claim if upheld, else overrule it."""
    with transaction.atomic():
        proposal = OutcomeProposal.objects.select_related("wager").get(pk=proposal_id)
        pact = _locked_pact(proposal.pact_id)
        proposal.refresh_from_db()
        if proposal.state != Proposal.DISPUTED:
            return {"applied": False, "apply_error": "Ten wynik nie jest już sporny."}
        if upheld:
            if pact.status not in (Pact.Status.ACTIVE, Pact.Status.AWAITING_RESULT):
                return {"applied": False, "apply_error": "Zakład jest już zamknięty."}
            _settle(pact, proposal)
        else:
            proposal.state = Proposal.OVERRULED
            proposal.save(update_fields=["state"])
            _announce(
                pact,
                "Wynik odrzucony",
                f"Sejmik odrzucił wynik zakładu: {pact.title}",
                push_to=_active_ids(pact),
            )
    return {"applied": upheld}


# --- resolutions: judged by everyone ---------------------------------------------


def call_judgment(pact_id: int, user: Any) -> Pact:
    """Put a resolution to a Sejmik vote of every member except its author.

    Only once the deadline has passed, for the author and everyone else alike.
    """
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        if not get_kind(pact.kind).judged_by_everyone:
            raise Conflict("Ten zakład nie jest oceniany w głosowaniu.")
        if pact.status not in (Pact.Status.ACTIVE, Pact.Status.AWAITING_RESULT):
            raise Conflict()
        if not result_allowed(pact):
            raise PermissionDenied("Postanowienie można ocenić dopiero po upływie terminu.")
        if pact.judgment_poll_id is not None:
            raise Conflict("Posłowie już oceniają to postanowienie.")
        poll = voting_services.create_poll(
            creator=user,
            title="",
            kind_key="resolution_judgment",
            config={"pact_id": pact.pk},
            participant_ids=[],
        )
        pact.judgment_poll = poll
        pact.save(update_fields=["judgment_poll"])
        _announce(pact, "", "")
    return pact


def apply_judgment(pact_id: int, *, kept: bool, decided: bool) -> dict[str, Any]:
    """Called when the judging vote closes: settle the resolution, or (tie, no votes) leave it
    open so somebody can call for judgment again."""
    with transaction.atomic():
        pact = _locked_pact(pact_id)
        pact.judgment_poll = None
        pact.save(update_fields=["judgment_poll"])
        if not decided:
            _announce(
                pact,
                "Brak rozstrzygnięcia",
                f"Posłowie nie rozstrzygnęli: {pact.title}",
                push_to=[pact.creator_id],
            )
            return {"applied": False}
        if pact.status not in (Pact.Status.ACTIVE, Pact.Status.AWAITING_RESULT):
            return {"applied": False, "apply_error": "Zakład jest już zamknięty."}
        proposal = OutcomeProposal.objects.create(
            pact=pact, proposed_by=pact.creator, result={"kept": kept}
        )
        _settle(pact, proposal)
    return {"applied": True}


# --- deadlines -----------------------------------------------------------------


def process_deadlines(now: datetime | None = None) -> dict[str, int]:
    """Move overdue pacts along, remind the parties, expire stale invites.

    Safe to run as often as you like (every few minutes). Returns counts of what it did.
    """
    now = now or timezone.now()
    counts = {"overdue": 0, "nudged": 0, "expired": 0, "lapsed": 0}
    stale = now - INVITE_TTL
    for pact_id in Pact.objects.filter(
        status__in=(Pact.Status.PROPOSED, Pact.Status.ACTIVE, Pact.Status.AWAITING_RESULT)
    ).values_list("pk", flat=True):
        with transaction.atomic():
            pact = Pact.objects.select_for_update().get(pk=pact_id)
            n = pact.participants.filter(
                state__in=(State.INVITED, State.REQUESTED), created_at__lt=stale
            ).update(state=State.EXPIRED)
            if n:
                counts["expired"] += n
                JoinConsent.objects.filter(participant__state=State.EXPIRED).delete()
                _refresh_status(pact)
            if pact.status == Pact.Status.PROPOSED and pact.due_at and pact.due_at <= now:
                pact.status = Pact.Status.CANCELLED  # it never started and the date has passed
                pact.save(update_fields=["status"])
                counts["lapsed"] += 1
                _announce(pact, "Zakład wygasł", pact.title)
                continue
            if pact.status == Pact.Status.ACTIVE and pact.due_at and pact.due_at <= now:
                pact.status = Pact.Status.AWAITING_RESULT
                pact.last_nudged_at = now
                pact.save(update_fields=["status", "last_nudged_at"])
                counts["overdue"] += 1
                _nudge(pact)
            elif pact.status == Pact.Status.AWAITING_RESULT and (
                pact.last_nudged_at is None or pact.last_nudged_at <= now - NUDGE_EVERY
            ):
                pact.last_nudged_at = now
                pact.save(update_fields=["last_nudged_at"])
                counts["nudged"] += 1
                _nudge(pact)
    return counts


def _nudge(pact: Pact) -> None:
    _announce(
        pact,
        "Termin zakładu minął",
        f"„{pact.title}”: ustalcie wynik",
        push_to=_active_ids(pact),
    )
