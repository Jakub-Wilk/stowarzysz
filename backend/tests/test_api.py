from rest_framework.test import APIClient


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
