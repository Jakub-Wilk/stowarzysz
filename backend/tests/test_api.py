from rest_framework.test import APIClient

from accounts.models import User


def test_health_is_public(api_client: APIClient) -> None:
    assert api_client.get("/api/health/").json() == {"status": "ok"}


def test_me_requires_jwt(api_client: APIClient) -> None:
    assert api_client.get("/api/auth/me/").status_code == 401


def test_me_with_jwt(auth_client: APIClient) -> None:
    assert auth_client.get("/api/auth/me/").json()["username"] == "alice"


def test_refresh_rotates_and_blacklists(api_client: APIClient, user) -> None:
    tokens = api_client.post(
        "/api/auth/token/", {"username": "alice", "password": "s3cret-pass-123"}
    ).data
    first = api_client.post("/api/auth/token/refresh/", {"refresh": tokens["refresh"]})
    assert first.status_code == 200
    reuse = api_client.post("/api/auth/token/refresh/", {"refresh": tokens["refresh"]})
    assert reuse.status_code == 401


def test_sse_requires_jwt(api_client: APIClient) -> None:
    assert api_client.get("/api/events/").status_code == 401


def test_me_includes_display_name(auth_client: APIClient, user) -> None:
    assert auth_client.get("/api/auth/me/").json()["display_name"] == "alice"
    user.first_name, user.last_name = "Alice", "Smith"
    user.save()
    assert auth_client.get("/api/auth/me/").json()["display_name"] == "Alice Smith"


def test_login_users_is_public_and_minimal(api_client: APIClient, user) -> None:
    resp = api_client.get("/api/auth/login-users/")
    assert resp.status_code == 200
    assert resp.json() == [{"username": "alice", "display_name": "alice"}]


def test_login_users_hides_inactive_and_unactivated(api_client: APIClient, user) -> None:
    User.objects.create_user("gone", password="x-pass-123", is_active=False)
    pending = User(username="pending")
    pending.set_unusable_password()
    pending.save()
    names = [u["username"] for u in api_client.get("/api/auth/login-users/").json()]
    assert names == ["alice"]
