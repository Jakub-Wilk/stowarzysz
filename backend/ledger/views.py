from django.db.models import Q, QuerySet
from django.shortcuts import get_object_or_404
from drf_spectacular.utils import OpenApiParameter, extend_schema
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from ledger import services
from ledger.models import LedgerEntry
from ledger.serializers import BalanceSerializer, LedgerEntrySerializer


def _my_entries(request: Request) -> QuerySet:
    return LedgerEntry.objects.filter(
        Q(debtor=request.user) | Q(creditor=request.user)
    ).select_related("debtor", "creditor")


class BalancesView(APIView):
    @extend_schema(responses={200: BalanceSerializer(many=True)})
    def get(self, request: Request) -> Response:
        rows = [
            {"user": other, "currency": currency, "amount": amount}
            for other, currency, amount in services.balances_for(request.user)
        ]
        return Response(BalanceSerializer(rows, many=True, context={"request": request}).data)


class EntryListView(APIView):
    @extend_schema(
        parameters=[OpenApiParameter("open", type=bool, required=False)],
        responses={200: LedgerEntrySerializer(many=True)},
    )
    def get(self, request: Request) -> Response:
        entries = _my_entries(request)
        if request.query_params.get("open") in ("1", "true"):
            entries = entries.filter(settled_at__isnull=True)
        return Response(
            LedgerEntrySerializer(entries, many=True, context={"request": request}).data
        )


class EntryPaidView(APIView):
    @extend_schema(request=None, responses={200: LedgerEntrySerializer})
    def post(self, request: Request, entry_id: int) -> Response:
        get_object_or_404(
            _my_entries(request), pk=entry_id
        )  # 404 for entries you're not a party to
        entry = services.mark_paid(entry_id, request.user)
        return Response(LedgerEntrySerializer(entry, context={"request": request}).data)


class EntryConfirmView(APIView):
    @extend_schema(request=None, responses={200: LedgerEntrySerializer})
    def post(self, request: Request, entry_id: int) -> Response:
        get_object_or_404(_my_entries(request), pk=entry_id)
        entry = services.confirm_paid(entry_id, request.user)
        return Response(LedgerEntrySerializer(entry, context={"request": request}).data)
