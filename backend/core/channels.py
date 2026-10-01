from django_eventstream.channelmanager import ChannelManagerBase


def user_channel(user_id: int | str) -> str:
    return f"user-{user_id}"


class UserChannelManager(ChannelManagerBase):
    """Every authenticated user can only listen to their own `user-<id>` channel."""

    def get_channels_for_request(self, request, view_kwargs):
        user = getattr(request, "user", None)
        if user is None or not user.is_authenticated:
            return set()
        return {user_channel(user.pk)}

    def can_read_channel(self, user, channel):
        return user is not None and channel == user_channel(user.pk)

    def is_channel_reliable(self, channel):
        return False
