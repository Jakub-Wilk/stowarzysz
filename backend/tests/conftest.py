import pytest
from rest_framework.test import APIClient

from accounts.models import User


@pytest.fixture
def user(db) -> User:
    return User.objects.create_user("alice", "s3cret-pass-123")


@pytest.fixture
def api_client() -> APIClient:
    return APIClient()


@pytest.fixture
def admin_client(db) -> APIClient:
    User.objects.create_superuser("boss", "admin-pass-123")
    client = APIClient()
    resp = client.post("/api/auth/token/", {"username": "boss", "password": "admin-pass-123"})
    client.credentials(HTTP_AUTHORIZATION=f"Bearer {resp.data['access']}")
    return client


@pytest.fixture
def auth_client(api_client: APIClient, user: User) -> APIClient:
    resp = api_client.post("/api/auth/token/", {"username": "alice", "password": "s3cret-pass-123"})
    api_client.credentials(HTTP_AUTHORIZATION=f"Bearer {resp.data['access']}")
    return api_client


@pytest.fixture(autouse=True)
def _clear_cache() -> None:
    from django.core.cache import cache

    cache.clear()  # throttle counters live in the cache
