from drf_spectacular.utils import OpenApiParameter, extend_schema
from rest_framework import status
from rest_framework.pagination import CursorPagination
from rest_framework.permissions import IsAuthenticated
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from accounts.permissions import IsSuperuser
from secretsanta import services
from secretsanta.models import SantaEvent
from secretsanta.serializers import (
    GiftNoteRequestSerializer,
    GiftSerializer,
    HistoryEventSerializer,
    SantaStateSerializer,
    StartRequestSerializer,
    UpdateRequestSerializer,
)


def _state(request: Request) -> dict:
    return SantaStateSerializer(
        {"event": services.get_active_event()}, context={"request": request}
    ).data


class SantaView(APIView):
    """Everyone can see whether Secret Santa is on; superusers start, adjust and end it."""

    def get_permissions(self):  # type: ignore[override]
        if self.request.method == "GET":
            return [IsAuthenticated()]
        return [IsSuperuser()]

    @extend_schema(responses={200: SantaStateSerializer})
    def get(self, request: Request) -> Response:
        return Response(_state(request))

    @extend_schema(request=StartRequestSerializer, responses={201: SantaStateSerializer})
    def post(self, request: Request) -> Response:
        serializer = StartRequestSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        services.start_event(
            creator=request.user,
            participant_ids=data["participant_ids"],
            deadline=data["deadline"],
            tiers=data["gift_tiers"],
        )
        return Response(_state(request), status=status.HTTP_201_CREATED)

    @extend_schema(request=UpdateRequestSerializer, responses={200: SantaStateSerializer})
    def patch(self, request: Request) -> Response:
        serializer = UpdateRequestSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        services.update_event(deadline=data.get("deadline"), tiers=data.get("gift_tiers"))
        return Response(_state(request))

    @extend_schema(request=None, responses={200: SantaStateSerializer})
    def delete(self, request: Request) -> Response:
        services.end_event()
        return Response(_state(request))


class HistoryPagination(CursorPagination):
    page_size = 10
    ordering = ("-ended_at", "-id")


class SantaHistoryView(APIView):
    """Ended events with their pairings, which are public once an event is over."""

    @extend_schema(
        parameters=[OpenApiParameter("cursor", type=str, required=False)],
        responses={200: HistoryEventSerializer(many=True)},
    )
    def get(self, request: Request) -> Response:
        queryset = SantaEvent.objects.filter(status=SantaEvent.Status.ENDED).prefetch_related(
            "assignments__giver", "assignments__receiver", "assignments__gifts"
        )
        paginator = HistoryPagination()
        page = paginator.paginate_queryset(queryset, request, view=self)
        return paginator.get_paginated_response(HistoryEventSerializer(page, many=True).data)


class SantaGiftView(APIView):
    @extend_schema(request=GiftNoteRequestSerializer, responses={200: GiftSerializer})
    def patch(self, request: Request, gift_id: int) -> Response:
        serializer = GiftNoteRequestSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        gift = services.set_gift_note(gift_id, request.user, serializer.validated_data["note"])
        return Response(GiftSerializer(gift).data)
