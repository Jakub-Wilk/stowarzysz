from django.db.models import QuerySet
from django.shortcuts import get_object_or_404
from drf_spectacular.utils import extend_schema
from rest_framework import status
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from ledger import services
from ledger.models import LedgerEntry
from ledger.serializers import BalancesSerializer, LedgerEntrySerializer, PaymentCreateSerializer


def _entries() -> QuerySet:
    return LedgerEntry.objects.select_related("debtor", "creditor")


class BalancesView(APIView):
    """The whole group's totals (public, like Tricount's balance)."""

    @extend_schema(responses={200: BalancesSerializer})
    def get(self, request: Request) -> Response:
        members, pairs = services.group_balances()
        data = {"members": members, "pairs": pairs}
        return Response(BalancesSerializer(data, context={"request": request}).data)


class EntryListView(APIView):
    """Every entry in the group (debts and payments), newest first. Everyone can read them all."""

    @extend_schema(responses={200: LedgerEntrySerializer(many=True)})
    def get(self, request: Request) -> Response:
        return Response(
            LedgerEntrySerializer(_entries(), many=True, context={"request": request}).data
        )


class PaymentCreateView(APIView):
    """Pay somebody off: the payment counts once the receiver confirms it."""

    @extend_schema(request=PaymentCreateSerializer, responses={201: LedgerEntrySerializer})
    def post(self, request: Request) -> Response:
        serializer = PaymentCreateSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        entry = services.create_payment(
            request.user, data["to_user_id"], data["amount"], data["note"]
        )
        return Response(
            LedgerEntrySerializer(entry, context={"request": request}).data,
            status=status.HTTP_201_CREATED,
        )


class _PaymentActionView(APIView):
    action = staticmethod(services.confirm_payment)

    @extend_schema(request=None, responses={200: LedgerEntrySerializer})
    def post(self, request: Request, entry_id: int) -> Response:
        get_object_or_404(LedgerEntry, pk=entry_id)
        entry = self.action(entry_id, request.user)
        return Response(LedgerEntrySerializer(entry, context={"request": request}).data)


class PaymentConfirmView(_PaymentActionView):
    action = staticmethod(services.confirm_payment)  # the receiver


class PaymentRejectView(_PaymentActionView):
    action = staticmethod(services.reject_payment)  # the receiver


class PaymentCancelView(_PaymentActionView):
    action = staticmethod(services.cancel_payment)  # the payer
