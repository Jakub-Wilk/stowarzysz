from django.db.models import Q, QuerySet
from django.http import Http404
from drf_spectacular.utils import OpenApiParameter, extend_schema
from rest_framework import status
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from pacts import services
from pacts.kinds import Terms
from pacts.models import Pact
from pacts.serializers import (
    PactCreateSerializer,
    PactDetailSerializer,
    PactSerializer,
    ProposalSerializer,
    ProposeOutcomeSerializer,
    RespondSerializer,
)


def visible_pacts(request: Request) -> QuerySet:
    """Open pacts are readable by everyone; the rest only by the people in them."""
    return (
        Pact.objects.filter(Q(is_open=True) | Q(participants__user=request.user))
        .select_related("creator")
        .prefetch_related("participants__user", "proposals__proposed_by")
        .distinct()
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
            OpenApiParameter("open", type=bool, required=False),
            OpenApiParameter("mine", type=bool, required=False),
        ],
        responses={200: PactSerializer(many=True)},
    )
    def get(self, request: Request) -> Response:
        pacts = visible_pacts(request).order_by("-created_at", "-id")
        if request.query_params.get("status") in Pact.Status.values:
            pacts = pacts.filter(status=request.query_params["status"])
        if request.query_params.get("open") in ("1", "true"):
            pacts = pacts.filter(is_open=True)
        if request.query_params.get("mine") in ("1", "true"):
            pacts = pacts.filter(participants__user=request.user)
        return Response(PactSerializer(pacts, many=True, context={"request": request}).data)

    @extend_schema(request=PactCreateSerializer, responses={201: PactDetailSerializer})
    def post(self, request: Request) -> Response:
        serializer = PactCreateSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        pact = services.create_pact(
            request.user,
            kind_key=data["kind"],
            title=data["title"],
            condition=data["condition"],
            notes=data["notes"],
            due_at=data.get("due_at"),
            is_open=data["is_open"],
            config=data["config"],
            opponents=[
                Terms(o["user_id"], o.get("stake_amount"), o.get("stake_note", ""))
                for o in data["opponents"]
            ],
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
        services.respond_to_invite(
            pact_id, request.user, accept=serializer.validated_data["accept"]
        )
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
            wager_id=serializer.validated_data["wager_id"],
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


class OutcomeDisputeView(APIView):
    @extend_schema(request=None, responses={200: PactDetailSerializer})
    def post(self, request: Request, pact_id: int, proposal_id: int) -> Response:
        services.dispute_outcome(proposal_id, request.user)
        return Response(_detail(request, pact_id))
