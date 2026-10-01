import pytest
from rest_framework.test import APIClient

from accounts.models import User


@pytest.fixture
def user(db) -> User:
    return User.objects.create_user("alice", "alice@example.com", "s3cret-pass-123")


@pytest.fixture
def api_client() -> APIClient:
    return APIClient()


@pytest.fixture
def auth_client(api_client: APIClient, user: User) -> APIClient:
    resp = api_client.post("/api/auth/token/", {"username": "alice", "password": "s3cret-pass-123"})
    api_client.credentials(HTTP_AUTHORIZATION=f"Bearer {resp.data['access']}")
    return api_client
