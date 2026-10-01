from typing import Any

from django_eventstream import send_event

from core.channels import user_channel


def notify_user(user_id: int, event_type: str, data: Any) -> None:
    """Push an SSE event to a single user. Safe to call from sync code (views, tasks, signals)."""
    send_event(user_channel(user_id), event_type, data)
