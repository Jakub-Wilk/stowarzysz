from django.contrib.auth import get_user_model
from django.db.models import QuerySet
from django.http import Http404
from drf_spectacular.utils import OpenApiParameter, extend_schema
from rest_framework import status
from rest_framework.exceptions import PermissionDenied, ValidationError
from rest_framework.parsers import MultiPartParser
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from pacts import services
from pacts.kinds import Terms
from pacts.models import Pact
from pacts.serializers import (
    AttachmentUploadSerializer,
    JoinDecisionSerializer,
    PactCreateSerializer,
    PactDetailSerializer,
    PactSerializer,
    ProposalSerializer,
    ProposeOutcomeSerializer,
    RespondSerializer,
    TermsSerializer,
)


def visible_pacts(request: Request) -> QuerySet:
    """Every member can read every pact; only participants act on it."""
    return Pact.objects.select_related("creator").prefetch_related(
        "participants__user",
        "participants__consents",
        "attachments__uploaded_by",
        "proposals__proposed_by",
        "proposals__confirmations",
        "proposals__wager",
    )


def _terms(user_id: int, data: dict) -> Terms:
    return Terms(
        user_id, data.get("stake_amount"), data.get("stake_note") or "", data.get("side") or ""
    )


def _detail(request: Request, pact_id: int) -> dict:
    pact = visible_pacts(request).filter(pk=pact_id).first()
    if pact is None:
        raise Http404
    return PactDetailSerializer(pact, context={"request": request}).data


class PactListCreateView(APIView):
    @extend_schema(
        parameters=[
            OpenApiParameter("status", enum=Pact.Status.values, required=False),
            OpenApiParameter("mine", type=bool, required=False),
        ],
        responses={200: PactSerializer(many=True)},
    )
    def get(self, request: Request) -> Response:
        pacts = visible_pacts(request).order_by("-created_at", "-id")
        if request.query_params.get("status") in Pact.Status.values:
            pacts = pacts.filter(status=request.query_params["status"])
        if request.query_params.get("mine") in ("1", "true"):
            pacts = pacts.filter(participants__user=request.user)
        return Response(PactSerializer(pacts, many=True, context={"request": request}).data)

    @extend_schema(request=PactCreateSerializer, responses={201: PactDetailSerializer})
    def post(self, request: Request) -> Response:
        serializer = PactCreateSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        creator = request.user
        if "creator_id" in data:  # TEMPORARY: admins entering old pacts for someone
            if not request.user.is_superuser:
                raise PermissionDenied("Tylko administrator może tworzyć zakłady w czyimś imieniu.")
            creator = get_user_model().objects.members().filter(pk=data["creator_id"]).first()
            if creator is None:
                raise ValidationError({"creator_id": "Nieznany lub nieaktywny poseł."})
        pact = services.create_pact(
            creator,
            kind_key=data["kind"],
            title=data["title"],
            condition=data["condition"],
            notes=data["notes"],
            due_at=data.get("due_at"),
            config=data["config"],
            host=_terms(creator.pk, data["host"]) if "host" in data else None,
            opponents=[_terms(o["user_id"], o) for o in data["opponents"]],
            backfill="creator_id" in data,
        )
        return Response(_detail(request, pact.pk), status=status.HTTP_201_CREATED)


class PactDetailView(APIView):
    @extend_schema(responses={200: PactDetailSerializer})
    def get(self, request: Request, pact_id: int) -> Response:
        return Response(_detail(request, pact_id))


class PactRespondView(APIView):
    @extend_schema(request=RespondSerializer, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        serializer = RespondSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        services.respond_to_invite(
            pact_id,
            request.user,
            accept=data["accept"],
            side=data.get("side") or "",
            stake_amount=data.get("stake_amount"),
            stake_note=data.get("stake_note"),
        )
        return Response(_detail(request, pact_id))


class PactJoinView(APIView):
    @extend_schema(request=TermsSerializer, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        serializer = TermsSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        services.request_to_join(
            pact_id, request.user, _terms(request.user.pk, serializer.validated_data)
        )
        return Response(_detail(request, pact_id))


class PactWithdrawView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        services.withdraw_request(pact_id, request.user)
        return Response(_detail(request, pact_id))


class JoinDecideView(APIView):
    @extend_schema(request=JoinDecisionSerializer, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int, participant_id: int) -> Response:
        serializer = JoinDecisionSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        services.decide_join(
            pact_id, request.user, participant_id, approve=serializer.validated_data["approve"]
        )
        return Response(_detail(request, pact_id))


class AttachmentListView(APIView):
    """Pictures that supplement a pact's notes (multipart `image`, optional `caption`)."""

    parser_classes = (MultiPartParser,)

    @extend_schema(request=AttachmentUploadSerializer, responses={201: PactDetailSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        serializer = AttachmentUploadSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        services.add_attachment(
            pact_id,
            request.user,
            serializer.validated_data["image"],
            serializer.validated_data["caption"],
        )
        return Response(_detail(request, pact_id), status=status.HTTP_201_CREATED)


class AttachmentDetailView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def delete(self, request: Request, pact_id: int, attachment_id: int) -> Response:
        services.delete_attachment(pact_id, attachment_id, request.user)
        return Response(_detail(request, pact_id))


class JudgmentView(APIView):
    """Put a resolution to a vote of all members (its author may do it any time)."""

    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        services.call_judgment(pact_id, request.user)
        return Response(_detail(request, pact_id))


class PactCancelView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        services.cancel_pact(pact_id, request.user)
        return Response(_detail(request, pact_id))


class OutcomeProposeView(APIView):
    @extend_schema(request=ProposeOutcomeSerializer, responses={201: ProposalSerializer})
    def post(self, request: Request, pact_id: int) -> Response:
        serializer = ProposeOutcomeSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        proposal = services.propose_outcome(
            pact_id,
            request.user,
            wager_id=serializer.validated_data.get("wager_id"),
            result=serializer.validated_data["result"],
        )
        return Response(
            ProposalSerializer(proposal, context={"request": request}).data,
            status=status.HTTP_201_CREATED,
        )


class OutcomeConfirmView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int, proposal_id: int) -> Response:
        services.confirm_outcome(proposal_id, request.user)
        return Response(_detail(request, pact_id))


class OutcomeEscalateView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int, proposal_id: int) -> Response:
        services.escalate_dispute(proposal_id, request.user)
        return Response(_detail(request, pact_id))


class OutcomeDisputeView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int, proposal_id: int) -> Response:
        services.dispute_outcome(proposal_id, request.user)
        return Response(_detail(request, pact_id))
