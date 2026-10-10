from typing import Any

from django.db.models import QuerySet
from django.shortcuts import get_object_or_404
from drf_spectacular.utils import OpenApiParameter, PolymorphicProxySerializer, extend_schema
from google.genai import errors as genai_errors
from rest_framework import status
from rest_framework.exceptions import APIException
from rest_framework.pagination import CursorPagination
from rest_framework.parsers import MultiPartParser
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from accounts.avatars import process_photo
from core import events
from ledger import rates, receipt_ocr, services, stats
from ledger.balances import balances
from ledger.kinds import Conflict, get_kind
from ledger.models import LedgerEntry
from ledger.money import BASE_CURRENCY, CURRENCIES
from ledger.serializers import (
    INPUTS,
    UPDATES,
    BalancesSerializer,
    EntryAttachmentUploadSerializer,
    EntrySerializer,
    MetaSerializer,
    RateQuerySerializer,
    RateSerializer,
    ReceiptOcrUploadSerializer,
    StatsQuerySerializer,
    StatsSerializer,
    people_for,
)

ENTRY_INPUT = PolymorphicProxySerializer(
    component_name="EntryInput",
    serializers=list(INPUTS.values()),
    resource_type_field_name="kind",
)
ENTRY_UPDATE = PolymorphicProxySerializer(
    component_name="EntryUpdate",
    serializers=list(UPDATES.values()),
    resource_type_field_name="kind",
)


def entries() -> QuerySet:
    return LedgerEntry.objects.select_related("created_by").prefetch_related(
        "obligations__debtor", "obligations__creditor", "attachments__uploaded_by"
    )


def serialize(request: Request, entry_list: list[LedgerEntry]) -> Any:
    context = {"request": request, "people": people_for(entry_list)}
    return EntrySerializer(entry_list, many=True, context=context).data


def detail(request: Request, entry_id: int) -> Any:
    return serialize(request, [get_object_or_404(entries(), pk=entry_id)])[0]


class FeedPagination(CursorPagination):
    page_size = 30
    ordering = ("-occurred_on", "-id")

    def paginate_queryset(self, queryset, request, view=None):  # type: ignore[override]
        if request.query_params.get("source_type"):
            return None  # one source's entries (a pact's settlement) are few; return them all
        return super().paginate_queryset(queryset, request, view)


class EntryListView(APIView):
    """The group's feed, newest first: every member reads every entry, like Tricount."""

    pagination_class = FeedPagination

    @extend_schema(
        parameters=[
            OpenApiParameter("kind", enum=["expense", "income", "debt", "payment"], required=False),
            OpenApiParameter("source_type", type=str, required=False),
            OpenApiParameter("source_id", type=int, required=False),
            OpenApiParameter("cursor", type=str, required=False),
        ],
        responses={200: EntrySerializer(many=True)},
    )
    def get(self, request: Request) -> Response:
        queryset = entries()
        params = request.query_params
        if params.get("kind"):
            queryset = queryset.filter(kind=params["kind"])
        if params.get("source_type"):
            queryset = queryset.filter(source_type=params["source_type"])
            if params.get("source_id", "").isdigit():
                queryset = queryset.filter(source_id=int(params["source_id"]))
        paginator = self.pagination_class()
        page = paginator.paginate_queryset(queryset, request, view=self)
        if page is None:
            return Response(serialize(request, list(queryset)))
        return paginator.get_paginated_response(serialize(request, page))

    @extend_schema(request=ENTRY_INPUT, responses={201: EntrySerializer})
    def post(self, request: Request) -> Response:
        kind = get_kind(str(request.data.get("kind", "")))
        serializer = INPUTS[kind.key](data=request.data)
        serializer.is_valid(raise_exception=True)
        entry = services.create_entry(kind.key, request.user, serializer.validated_data)
        return Response(detail(request, entry.pk), status=status.HTTP_201_CREATED)


class EntryDetailView(APIView):
    @extend_schema(responses={200: EntrySerializer})
    def get(self, request: Request, entry_id: int) -> Response:
        return Response(detail(request, entry_id))

    @extend_schema(request=ENTRY_UPDATE, responses={200: EntrySerializer})
    def put(self, request: Request, entry_id: int) -> Response:
        """Replace an entry's content (expenses, incomes and manual debts). Send the `version`
        you edited; if somebody saved in between, you get a 409 and should reload."""
        entry = get_object_or_404(LedgerEntry, pk=entry_id)
        if entry.kind not in UPDATES:
            raise Conflict("Tego wpisu nie można edytować.")
        get_kind(entry.kind).check(entry, request.user, "edit")  # 409/403 before a 400
        serializer = UPDATES[entry.kind](data={**request.data, "kind": entry.kind})
        serializer.is_valid(raise_exception=True)
        data = dict(serializer.validated_data)
        services.update_entry(entry_id, request.user, data, data.pop("version"))
        return Response(detail(request, entry_id))


class _ActionView(APIView):
    transition = staticmethod(services.confirm)

    @extend_schema(request=None, responses={200: EntrySerializer})
    def post(self, request: Request, entry_id: int) -> Response:
        self.transition(entry_id, request.user)
        return Response(detail(request, entry_id))


class ConfirmView(_ActionView):
    transition = staticmethod(services.confirm)  # the receiver of a payment


class RejectView(_ActionView):
    transition = staticmethod(services.reject)  # the receiver of a payment


class CancelView(_ActionView):
    transition = staticmethod(services.cancel)  # whoever the kind allows


class AttachmentListView(APIView):
    """Pictures on an entry (multipart `image`): a receipt, proof of a transfer."""

    parser_classes = (MultiPartParser,)

    @extend_schema(request=EntryAttachmentUploadSerializer, responses={201: EntrySerializer})
    def post(self, request: Request, entry_id: int) -> Response:
        serializer = EntryAttachmentUploadSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        services.add_attachment(entry_id, request.user, serializer.validated_data["image"])
        return Response(detail(request, entry_id), status=status.HTTP_201_CREATED)


class AttachmentDetailView(APIView):
    @extend_schema(request=None, responses={200: EntrySerializer})
    def delete(self, request: Request, entry_id: int, attachment_id: int) -> Response:
        services.delete_attachment(entry_id, attachment_id, request.user)
        return Response(detail(request, entry_id))


class BalancesView(APIView):
    """The whole group's Bilans (public, like Tricount's)."""

    @extend_schema(responses={200: BalancesSerializer})
    def get(self, request: Request) -> Response:
        result = balances()
        data = {
            "currency": BASE_CURRENCY,
            "members": result.members,
            "settlements": result.settlements,
            "pending": result.pending,
        }
        return Response(BalancesSerializer(data, context={"request": request}).data)


class RateView(APIView):
    """The rate an expense in `currency` on `date` will use. The form asks as soon as either
    changes, which also warms the cache so saving doesn't wait on the network."""

    @extend_schema(parameters=[RateQuerySerializer], responses={200: RateSerializer})
    def get(self, request: Request) -> Response:
        query = RateQuerySerializer(data=request.query_params)
        query.is_valid(raise_exception=True)
        currency = query.validated_data["currency"]
        rate = rates.rate_for(currency, query.validated_data["date"])
        data = {"currency": currency, "rate": rate.value, "rate_date": rate.day}
        return Response(RateSerializer(data).data)


class StatsView(APIView):
    """Spending in a period (public, like the entries): totals, by category, by person and over
    time. Without dates it covers everything."""

    @extend_schema(parameters=[StatsQuerySerializer], responses={200: StatsSerializer})
    def get(self, request: Request) -> Response:
        query = StatsQuerySerializer(data=request.query_params)
        query.is_valid(raise_exception=True)
        data = stats.spending(query.validated_data.get("start"), query.validated_data.get("end"))
        return Response(StatsSerializer(data, context={"request": request}).data)


class MetaView(APIView):
    """What the forms offer: currencies (with their minor-unit digits) and categories."""

    @extend_schema(responses={200: MetaSerializer})
    def get(self, request: Request) -> Response:
        data = {
            "base_currency": BASE_CURRENCY,
            "currencies": [{"code": c, "exponent": e} for c, e in CURRENCIES.items()],
            "categories": [
                {"key": key, "label": label, "emoji": LedgerEntry.CATEGORY_EMOJI[key]}
                for key, label in LedgerEntry.Category.choices
            ],
        }
        return Response(MetaSerializer(data).data)


class OcrUnavailable(APIException):
    status_code = status.HTTP_503_SERVICE_UNAVAILABLE
    default_detail = "Nie udało się odczytać paragonu, spróbuj ponownie."
    default_code = "ocr_unavailable"


class ReceiptOcrView(APIView):
    """Read a receipt photo (multipart `image` + `job_id`) with Gemini.

    Blocks until the result is ready. Each request sent to Gemini (a 503 is retried) is announced
    to the caller over SSE as `ledger.ocr.request {job_id, attempt}` so the form can show
    progress; the HTTP response is what carries the result.
    """

    parser_classes = (MultiPartParser,)

    @extend_schema(request=ReceiptOcrUploadSerializer, responses={200: receipt_ocr.Receipt})
    def post(self, request: Request) -> Response:
        serializer = ReceiptOcrUploadSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        job_id = str(serializer.validated_data["job_id"])
        photo = process_photo(serializer.validated_data["image"])

        def announce(attempt: int) -> None:
            data = {"job_id": job_id, "attempt": attempt}
            events.notify_user(request.user.id, "ledger.ocr.request", data)

        try:
            result = receipt_ocr.ocr_receipt(photo.read(), "image/webp", on_request_sent=announce)
        except (RuntimeError, genai_errors.APIError) as exc:
            raise OcrUnavailable from exc
        return Response(result.model_dump(mode="json"))
