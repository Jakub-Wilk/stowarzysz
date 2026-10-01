import json
import logging
import threading
from collections.abc import Iterable
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass

from django.conf import settings
from django.db import connections
from pywebpush import WebPushException, webpush

from push.models import PushSubscription

logger = logging.getLogger(__name__)

_worker = threading.local()


def _mark_worker() -> None:
    _worker.active = True


# Network calls must never run inside a request: deliver from a small thread pool.
_executor = ThreadPoolExecutor(max_workers=4, thread_name_prefix="push", initializer=_mark_worker)


@dataclass(frozen=True)
class _Target:
    endpoint: str
    p256dh: str
    auth: str


def push_enabled() -> bool:
    return bool(settings.VAPID_PRIVATE_KEY and settings.VAPID_PUBLIC_KEY)


def send_push(user_ids: Iterable[int], *, title: str, body: str, url: str) -> None:
    """Queue a notification to every subscription of the given users (no-op without VAPID keys)."""
    if not push_enabled():
        logger.info("Web Push is not configured (VAPID keys missing); skipping notification.")
        return
    targets = [
        _Target(s.endpoint, s.p256dh, s.auth)
        for s in PushSubscription.objects.filter(user_id__in=list(user_ids))
    ]
    if targets:
        payload = json.dumps({"title": title, "body": body, "url": url})
        _executor.submit(_deliver, targets, payload)


def _deliver(targets: list[_Target], payload: str) -> None:
    dead: list[str] = []
    try:
        for target in targets:
            try:
                webpush(
                    subscription_info={
                        "endpoint": target.endpoint,
                        "keys": {"p256dh": target.p256dh, "auth": target.auth},
                    },
                    data=payload,
                    vapid_private_key=settings.VAPID_PRIVATE_KEY,
                    vapid_claims={"sub": settings.VAPID_SUBJECT},
                    ttl=3600,
                )
            except WebPushException as exc:
                status = getattr(exc.response, "status_code", None)
                if status in (404, 410):  # subscription is gone for good
                    dead.append(target.endpoint)
                else:
                    logger.warning("Web Push to %s failed: %s", target.endpoint[:60], exc)
            except Exception:
                logger.exception("Unexpected Web Push failure")
        if dead:
            PushSubscription.objects.filter(endpoint__in=dead).delete()
    finally:
        if getattr(_worker, "active", False):
            connections.close_all()  # this thread's DB connections are not managed by a request
