from typing import Any

from django.contrib.auth import get_user_model

from core.events import notify_user


def broadcast(event_type: str, data: dict[str, Any]) -> None:
    """Tell every active user's SSE stream. Payloads carry ids only; clients refetch, so the
    hidden-until-the-end rules stay enforced by the API."""
    user_ids = get_user_model()._default_manager.filter(is_active=True).values_list("pk", flat=True)
    for user_id in user_ids:
        notify_user(user_id, event_type, data)
