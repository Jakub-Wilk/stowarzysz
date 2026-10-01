from rest_framework import serializers


class PublicKeySerializer(serializers.Serializer):
    public_key = serializers.CharField(allow_null=True)


class SubscriptionKeysSerializer(serializers.Serializer):
    p256dh = serializers.CharField(max_length=255)
    auth = serializers.CharField(max_length=255)


class SubscribeSerializer(serializers.Serializer):
    endpoint = serializers.CharField()
    keys = SubscriptionKeysSerializer()


class UnsubscribeSerializer(serializers.Serializer):
    endpoint = serializers.CharField()
