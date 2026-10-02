from datetime import timedelta

import pytest
from django.core.cache import cache
from django.utils import timezone
from rest_framework.test import APIClient

from accounts.models import ActivationToken, User

STRONG_PASSWORD = "correct-horse-battery-9"


@pytest.fixture(autouse=True)
def _clear_throttle_cache() -> None:
    cache.clear()


@pytest.fixture
def pending_user(admin_client: APIClient) -> User:
    resp = admin_client.post("/api/auth/users/", {"username": "bob"})
    assert resp.status_code == 201
    return User.objects.get(pk=resp.json()["id"])


def issue_link(admin_client: APIClient, user: User) -> str:
    resp = admin_client.post(f"/api/auth/users/{user.pk}/activation-link/")
    assert resp.status_code == 201
    return resp.json()["token"]


# --- admin endpoints ---


def test_create_user_requires_auth(api_client: APIClient) -> None:
    assert api_client.post("/api/auth/users/", {"username": "x"}).status_code == 401


def test_create_user_requires_admin(auth_client: APIClient) -> None:
    assert auth_client.post("/api/auth/users/", {"username": "x"}).status_code == 403


def test_create_user_has_unusable_password(api_client: APIClient, pending_user: User) -> None:
    assert not pending_user.has_usable_password()
    resp = api_client.post("/api/auth/token/", {"username": "bob", "password": "anything-123"})
    assert resp.status_code == 401


def test_activation_link_requires_auth(api_client: APIClient, user: User) -> None:
    assert api_client.post(f"/api/auth/users/{user.pk}/activation-link/").status_code == 401


def test_activation_link_requires_admin(auth_client: APIClient, user: User) -> None:
    assert auth_client.post(f"/api/auth/users/{user.pk}/activation-link/").status_code == 403


def test_activation_link_unknown_user(admin_client: APIClient) -> None:
    assert admin_client.post("/api/auth/users/9999/activation-link/").status_code == 404


def test_activation_link_returns_url(admin_client: APIClient, pending_user: User) -> None:
    resp = admin_client.post(f"/api/auth/users/{pending_user.pk}/activation-link/")
    body = resp.json()
    assert body["url"] == f"http://localhost:5173/activate/{body['token']}"
    assert ActivationToken.objects.get(user=pending_user).token_hash != body["token"]


def test_new_link_invalidates_previous(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    first = issue_link(admin_client, pending_user)
    second = issue_link(admin_client, pending_user)
    assert api_client.post("/api/auth/activation/validate/", {"token": first}).status_code == 400
    assert api_client.post("/api/auth/activation/validate/", {"token": second}).status_code == 200


# --- public endpoints ---


def test_validate_returns_username(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    resp = api_client.post("/api/auth/activation/validate/", {"token": raw})
    assert resp.json() == {"username": "bob"}


def test_complete_sets_password_without_logging_in(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    resp = api_client.post(
        "/api/auth/activation/complete/", {"token": raw, "password": STRONG_PASSWORD}
    )
    assert resp.status_code == 204
    assert not resp.content
    pending_user.refresh_from_db()
    assert pending_user.check_password(STRONG_PASSWORD)
    assert ActivationToken.objects.get(user=pending_user).used_at is not None

    login = APIClient().post("/api/auth/token/", {"username": "bob", "password": STRONG_PASSWORD})
    assert login.status_code == 200


def test_complete_rejects_reuse(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    payload = {"token": raw, "password": STRONG_PASSWORD}
    assert api_client.post("/api/auth/activation/complete/", payload).status_code == 204
    assert api_client.post("/api/auth/activation/complete/", payload).status_code == 400


def test_complete_rejects_expired(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    ActivationToken.objects.update(expires_at=timezone.now() - timedelta(seconds=1))
    resp = api_client.post(
        "/api/auth/activation/complete/", {"token": raw, "password": STRONG_PASSWORD}
    )
    assert resp.status_code == 400


def test_complete_rejects_weak_password(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    resp = api_client.post("/api/auth/activation/complete/", {"token": raw, "password": "123"})
    assert resp.status_code == 400
    assert "password" in resp.json()
    pending_user.refresh_from_db()
    assert not pending_user.has_usable_password()


def test_complete_rejects_inactive_user(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    User.objects.filter(pk=pending_user.pk).update(is_active=False)
    resp = api_client.post(
        "/api/auth/activation/complete/", {"token": raw, "password": STRONG_PASSWORD}
    )
    assert resp.status_code == 400


def test_bad_token_matches_expired_response(
    admin_client: APIClient, api_client: APIClient, pending_user: User
) -> None:
    raw = issue_link(admin_client, pending_user)
    ActivationToken.objects.update(expires_at=timezone.now() - timedelta(seconds=1))
    expired = api_client.post("/api/auth/activation/validate/", {"token": raw})
    bogus = api_client.post("/api/auth/activation/validate/", {"token": "nope"})
    assert expired.status_code == bogus.status_code == 400
    assert expired.json() == bogus.json()


def test_activation_endpoints_are_throttled(api_client: APIClient, db) -> None:
    codes = [
        api_client.post("/api/auth/activation/validate/", {"token": "nope"}).status_code
        for _ in range(11)
    ]
    assert codes[:10] == [400] * 10
    assert codes[10] == 429
