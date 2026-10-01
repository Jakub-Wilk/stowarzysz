from typing import Any

from django.contrib.auth import get_user_model
from django_eventstream import send_event

from core.channels import user_channel


def notify_user(user_id: int, event_type: str, data: Any) -> None:
    """Push an SSE event to a single user. Safe to call from sync code (views, tasks, signals)."""
    send_event(user_channel(user_id), event_type, data)


def broadcast(event_type: str, data: dict[str, Any]) -> None:
    """Tell every active user's SSE stream. Payloads carry ids only; clients refetch, so the
    visibility rules stay enforced by the API."""
    user_ids = get_user_model()._default_manager.filter(is_active=True).values_list("pk", flat=True)
    for user_id in user_ids:
        notify_user(user_id, event_type, data)
