import json
from types import SimpleNamespace
from typing import Any

import pytest
from pywebpush import WebPushException
from rest_framework.test import APIClient

from accounts.models import User
from push import sender
from push.models import PushSubscription

SUBSCRIBE = "/api/push/subscriptions/"
PUBLIC_KEY = "/api/push/public-key/"


def body(endpoint: str = "https://push.example/abc") -> dict[str, Any]:
    return {"endpoint": endpoint, "keys": {"p256dh": "key-1", "auth": "auth-1"}}


# --- endpoints --------------------------------------------------------------


def test_endpoints_require_auth(api_client: APIClient) -> None:
    assert api_client.get(PUBLIC_KEY).status_code == 401
    assert api_client.post(SUBSCRIBE, body()).status_code == 401
    assert api_client.delete(SUBSCRIBE, {"endpoint": "x"}).status_code == 401


def test_public_key_is_null_until_configured(auth_client: APIClient, settings) -> None:
    settings.VAPID_PUBLIC_KEY = settings.VAPID_PRIVATE_KEY = ""
    assert auth_client.get(PUBLIC_KEY).json() == {"public_key": None}
    settings.VAPID_PUBLIC_KEY, settings.VAPID_PRIVATE_KEY = "pub", "priv"
    assert auth_client.get(PUBLIC_KEY).json() == {"public_key": "pub"}


def test_subscribe_upserts_by_endpoint(auth_client: APIClient, user: User) -> None:
    assert auth_client.post(SUBSCRIBE, body()).status_code == 204
    changed = body()
    changed["keys"]["auth"] = "auth-2"
    assert auth_client.post(SUBSCRIBE, changed).status_code == 204
    (sub,) = PushSubscription.objects.all()
    assert sub.user == user and sub.auth == "auth-2"


def test_endpoint_follows_the_user_who_subscribes_last(auth_client: APIClient, db) -> None:
    other = User.objects.create(username="other")
    PushSubscription.objects.create(user=other, endpoint=body()["endpoint"], p256dh="a", auth="b")
    auth_client.post(SUBSCRIBE, body())
    (sub,) = PushSubscription.objects.all()
    assert sub.user.username == "alice"


@pytest.mark.parametrize("payload", [{}, {"endpoint": "x"}, {"endpoint": "x", "keys": {}}])
def test_subscribe_validation(auth_client: APIClient, payload: dict[str, Any]) -> None:
    assert auth_client.post(SUBSCRIBE, payload).status_code == 400


def test_unsubscribe_only_removes_own(auth_client: APIClient, user: User) -> None:
    other = User.objects.create(username="other")
    PushSubscription.objects.create(user=user, endpoint="mine", p256dh="a", auth="b")
    PushSubscription.objects.create(user=other, endpoint="theirs", p256dh="a", auth="b")
    assert auth_client.delete(SUBSCRIBE, {"endpoint": "theirs"}).status_code == 204
    assert auth_client.delete(SUBSCRIBE, {"endpoint": "mine"}).status_code == 204
    assert list(PushSubscription.objects.values_list("endpoint", flat=True)) == ["theirs"]


# --- sending ----------------------------------------------------------------


class InlineExecutor:
    def submit(self, fn, *args):
        fn(*args)


@pytest.fixture
def configured(monkeypatch: pytest.MonkeyPatch, settings) -> list[dict[str, Any]]:
    settings.VAPID_PUBLIC_KEY, settings.VAPID_PRIVATE_KEY = "pub", "priv"
    settings.VAPID_SUBJECT = "mailto:test@example.com"
    monkeypatch.setattr(sender, "_executor", InlineExecutor())
    calls: list[dict[str, Any]] = []
    monkeypatch.setattr(sender, "webpush", lambda **kw: calls.append(kw))
    return calls


def subscribe(user: User, endpoint: str) -> None:
    PushSubscription.objects.create(user=user, endpoint=endpoint, p256dh="p", auth="a")


def test_sends_to_listed_users_only(configured, db) -> None:
    a, b, c = (User.objects.create(username=n) for n in "abc")
    subscribe(a, "ep-a1")
    subscribe(a, "ep-a2")  # a second device
    subscribe(b, "ep-b")
    subscribe(c, "ep-c")
    sender.send_push([a.pk, b.pk], title="New vote", body="Pizza?", url="/voting/1")
    assert sorted(c["subscription_info"]["endpoint"] for c in configured) == [
        "ep-a1",
        "ep-a2",
        "ep-b",
    ]
    first = configured[0]
    assert json.loads(first["data"]) == {"title": "New vote", "body": "Pizza?", "url": "/voting/1"}
    assert first["vapid_private_key"] == "priv"
    assert first["vapid_claims"] == {"sub": "mailto:test@example.com"}


def test_noop_without_vapid_keys(monkeypatch: pytest.MonkeyPatch, settings, db) -> None:
    settings.VAPID_PUBLIC_KEY = settings.VAPID_PRIVATE_KEY = ""
    calls: list[Any] = []
    monkeypatch.setattr(sender, "webpush", lambda **kw: calls.append(kw))
    user = User.objects.create(username="a")
    subscribe(user, "ep")
    sender.send_push([user.pk], title="t", body="b", url="/")
    assert calls == []


def test_gone_subscriptions_are_pruned_other_failures_kept(
    configured, monkeypatch: pytest.MonkeyPatch, db
) -> None:
    user = User.objects.create(username="a")
    for ep in ("gone", "flaky", "fine"):
        subscribe(user, ep)

    def fake(**kw):
        ep = kw["subscription_info"]["endpoint"]
        if ep == "gone":
            raise WebPushException("gone", response=SimpleNamespace(status_code=410))
        if ep == "flaky":
            raise WebPushException("boom", response=SimpleNamespace(status_code=500))

    monkeypatch.setattr(sender, "webpush", fake)
    sender.send_push([user.pk], title="t", body="b", url="/")
    assert sorted(PushSubscription.objects.values_list("endpoint", flat=True)) == ["fine", "flaky"]


def test_unexpected_errors_do_not_break_the_batch(
    configured, monkeypatch: pytest.MonkeyPatch, db
) -> None:
    user = User.objects.create(username="a")
    subscribe(user, "first")
    subscribe(user, "second")
    seen: list[str] = []

    def fake(**kw):
        seen.append(kw["subscription_info"]["endpoint"])
        if len(seen) == 1:
            raise RuntimeError("network down")

    monkeypatch.setattr(sender, "webpush", fake)
    sender.send_push([user.pk], title="t", body="b", url="/")
    assert len(seen) == 2
