from django.conf import settings
from drf_spectacular.utils import extend_schema
from rest_framework import status
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView

from push.models import PushSubscription
from push.sender import push_enabled
from push.serializers import PublicKeySerializer, SubscribeSerializer, UnsubscribeSerializer


class PublicKeyView(APIView):
    @extend_schema(responses={200: PublicKeySerializer})
    def get(self, request: Request) -> Response:
        return Response({"public_key": settings.VAPID_PUBLIC_KEY if push_enabled() else None})


class SubscriptionView(APIView):
    @extend_schema(request=SubscribeSerializer, responses={204: None})
    def post(self, request: Request) -> Response:
        serializer = SubscribeSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        data = serializer.validated_data
        # An endpoint identifies a browser; if it changes hands (re-login), it follows the user.
        PushSubscription.objects.update_or_create(
            endpoint=data["endpoint"],
            defaults={
                "user": request.user,
                "p256dh": data["keys"]["p256dh"],
                "auth": data["keys"]["auth"],
                "user_agent": request.headers.get("User-Agent", "")[:255],
            },
        )
        return Response(status=status.HTTP_204_NO_CONTENT)

    @extend_schema(request=UnsubscribeSerializer, responses={204: None})
    def delete(self, request: Request) -> Response:
        serializer = UnsubscribeSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        PushSubscription.objects.filter(
            endpoint=serializer.validated_data["endpoint"], user=request.user
        ).delete()
        return Response(status=status.HTTP_204_NO_CONTENT)
