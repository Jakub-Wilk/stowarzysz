from django.db.models import QuerySet
from django.shortcuts import get_object_or_404
from drf_spectacular.utils import OpenApiParameter, extend_schema
from rest_framework import status
from rest_framework.pagination import CursorPagination
from rest_framework.parsers import JSONParser, MultiPartParser
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from voting import services
from voting.models import Poll
from voting.serializers import (
    BallotRequestSerializer,
    PollCreateSerializer,
    PollDetailSerializer,
    PollSerializer,
    ReactionRequestSerializer,
)


def polls_queryset() -> QuerySet:
    return Poll.objects.select_related("creator").prefetch_related("participants__user")


class HistoryPagination(CursorPagination):
    page_size = 20
    ordering = ("-created_at", "-id")

    def paginate_queryset(self, queryset, request, view=None):  # type: ignore[override]
        if request.query_params.get("status") == Poll.Status.OPEN:
            return None  # active polls are few; return them all, unpaged
        return super().paginate_queryset(queryset, request, view)


class PollListCreateView(APIView):
    """Every signed-in user can read every poll; ballots stay hidden until a poll ends."""

    pagination_class = HistoryPagination
    parser_classes = (JSONParser, MultiPartParser)  # multipart: profile-picture votes

    @extend_schema(
        parameters=[
            OpenApiParameter("status", enum=["open", "closed"], required=False),
            OpenApiParameter("participating", type=bool, required=False),
            OpenApiParameter("cursor", type=str, required=False),
        ],
        responses={200: PollSerializer(many=True)},
    )
    def get(self, request: Request) -> Response:
        queryset = polls_queryset()
        poll_status = request.query_params.get("status")
        if poll_status in Poll.Status.values:
            queryset = queryset.filter(status=poll_status)
        if request.query_params.get("participating") in ("1", "true"):
            queryset = queryset.filter(participants__user=request.user)
        paginator = self.pagination_class()
        page = paginator.paginate_queryset(queryset, request, view=self)
        if page is None:
            data = PollSerializer(queryset, many=True, context={"request": request}).data
            return Response(data)
        data = PollSerializer(page, many=True, context={"request": request}).data
        return paginator.get_paginated_response(data)

    @extend_schema(request=PollCreateSerializer, responses={201: PollDetailSerializer})
    def post(self, request: Request) -> Response:
        serializer = PollCreateSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        poll = services.create_poll(
            creator=request.user,
            title=data["title"],
            kind_key=data["kind"],
            config=data["config"],
            participant_ids=data["participant_ids"],
            image=data.get("image"),
        )
        return Response(_detail(poll.pk, request), status=status.HTTP_201_CREATED)


def _detail(poll_id: int, request: Request) -> dict:
    poll = get_object_or_404(polls_queryset(), pk=poll_id)
    return PollDetailSerializer(poll, context={"request": request}).data


class PollDetailView(APIView):
    @extend_schema(responses={200: PollDetailSerializer})
    def get(self, request: Request, poll_id: int) -> Response:
        return Response(_detail(poll_id, request))


class PollBallotView(APIView):
    @extend_schema(request=BallotRequestSerializer, responses={200: PollDetailSerializer})
    def put(self, request: Request, poll_id: int) -> Response:
        serializer = BallotRequestSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        get_object_or_404(Poll, pk=poll_id)
        services.cast_ballot(poll_id, request.user, serializer.validated_data["ballot"])
        return Response(_detail(poll_id, request))


class PollVetoView(APIView):
    @extend_schema(request=None, responses={200: PollDetailSerializer})
    def post(self, request: Request, poll_id: int) -> Response:
        get_object_or_404(Poll, pk=poll_id)
        services.veto(poll_id, request.user)
        return Response(_detail(poll_id, request))


class PollCloseView(APIView):
    @extend_schema(request=None, responses={200: PollDetailSerializer})
    def post(self, request: Request, poll_id: int) -> Response:
        get_object_or_404(Poll, pk=poll_id)
        services.close_early(poll_id, request.user)
        return Response(_detail(poll_id, request))


class PollReactionView(APIView):
    """Deliberately unthrottled: reactions are meant to be spammed."""

    @extend_schema(request=ReactionRequestSerializer, responses={204: None})
    def post(self, request: Request, poll_id: int) -> Response:
        serializer = ReactionRequestSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        poll = get_object_or_404(Poll, pk=poll_id)
        services.send_reaction(poll, request.user, serializer.validated_data["emoji"])
        return Response(status=status.HTTP_204_NO_CONTENT)
