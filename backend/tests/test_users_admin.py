import pytest
from rest_framework.test import APIClient

from accounts.models import User

LIST_URL = "/api/auth/users/"


def detail_url(user: User) -> str:
    return f"{LIST_URL}{user.pk}/"


@pytest.fixture
def staff_client(db) -> APIClient:
    """is_staff but not superuser: must not get management access."""
    User.objects.create_user("staffer", password="staff-pass-123", is_staff=True)
    client = APIClient()
    resp = client.post("/api/auth/token/", {"username": "staffer", "password": "staff-pass-123"})
    client.credentials(HTTP_AUTHORIZATION=f"Bearer {resp.data['access']}")
    return client


@pytest.fixture
def boss(admin_client: APIClient) -> User:
    return User.objects.get(username="boss")


# --- permissions -----------------------------------------------------------


def test_endpoints_require_auth(api_client: APIClient, user: User) -> None:
    assert api_client.get(LIST_URL).status_code == 401
    assert api_client.post(LIST_URL, {"username": "x"}).status_code == 401
    assert api_client.get(detail_url(user)).status_code == 401
    assert api_client.patch(detail_url(user), {}).status_code == 401
    assert api_client.delete(detail_url(user)).status_code == 401


@pytest.mark.parametrize("client_fixture", ["auth_client", "staff_client"])
def test_endpoints_require_superuser(
    client_fixture: str, request: pytest.FixtureRequest, user: User
) -> None:
    client: APIClient = request.getfixturevalue(client_fixture)
    assert client.get(LIST_URL).status_code == 403
    assert client.post(LIST_URL, {"username": "x"}).status_code == 403
    assert client.get(detail_url(user)).status_code == 403
    assert client.patch(detail_url(user), {"first_name": "X"}).status_code == 403
    assert client.delete(detail_url(user)).status_code == 403
    assert User.objects.filter(pk=user.pk).exists()


# --- list / create / retrieve ----------------------------------------------


def test_list_users(admin_client: APIClient, user: User) -> None:
    resp = admin_client.get(LIST_URL)
    assert resp.status_code == 200
    rows = {row["username"]: row for row in resp.json()}
    assert set(rows) == {"alice", "boss"}
    assert rows["alice"] == {
        "id": user.pk,
        "username": "alice",
        "first_name": "",
        "last_name": "",
        "email": "alice@example.com",
        "is_active": True,
        "is_superuser": False,
        "has_password": True,
    }


def test_create_user_is_passwordless(admin_client: APIClient) -> None:
    resp = admin_client.post(LIST_URL, {"username": "bob", "first_name": "Bob"})
    assert resp.status_code == 201
    body = resp.json()
    assert body["has_password"] is False
    assert body["is_active"] is True
    assert body["is_superuser"] is False
    assert not User.objects.get(username="bob").has_usable_password()


def test_create_superuser_sets_staff(admin_client: APIClient) -> None:
    resp = admin_client.post(LIST_URL, {"username": "root2", "is_superuser": True})
    assert resp.status_code == 201
    created = User.objects.get(username="root2")
    assert created.is_superuser and created.is_staff


def test_create_rejects_duplicate_username(admin_client: APIClient, user: User) -> None:
    assert admin_client.post(LIST_URL, {"username": "alice"}).status_code == 400


def test_retrieve_user(admin_client: APIClient, user: User) -> None:
    resp = admin_client.get(detail_url(user))
    assert resp.status_code == 200
    assert resp.json()["username"] == "alice"


def test_unknown_user_is_404(admin_client: APIClient) -> None:
    assert admin_client.get(f"{LIST_URL}9999/").status_code == 404


# --- update ----------------------------------------------------------------


def test_patch_updates_fields(admin_client: APIClient, user: User) -> None:
    resp = admin_client.patch(
        detail_url(user), {"first_name": "Alice", "last_name": "Smith", "email": "a@b.co"}
    )
    assert resp.status_code == 200
    user.refresh_from_db()
    assert (user.first_name, user.last_name, user.email) == ("Alice", "Smith", "a@b.co")
    assert user.check_password("s3cret-pass-123")  # password untouched


def test_patch_cannot_set_password_or_id(admin_client: APIClient, user: User) -> None:
    admin_client.patch(detail_url(user), {"password": "hacked-pass-1", "has_password": False})
    user.refresh_from_db()
    assert user.check_password("s3cret-pass-123")


def test_put_is_not_allowed(admin_client: APIClient, user: User) -> None:
    assert admin_client.put(detail_url(user), {"username": "alice"}).status_code == 405


def test_patch_rejects_duplicate_username(admin_client: APIClient, user: User, boss: User) -> None:
    assert admin_client.patch(detail_url(user), {"username": "boss"}).status_code == 400


def test_promote_and_demote_syncs_staff(admin_client: APIClient, user: User) -> None:
    admin_client.patch(detail_url(user), {"is_superuser": True})
    user.refresh_from_db()
    assert user.is_superuser and user.is_staff
    admin_client.patch(detail_url(user), {"is_superuser": False})
    user.refresh_from_db()
    assert not user.is_superuser and not user.is_staff


def test_cannot_deactivate_or_demote_self(admin_client: APIClient, boss: User) -> None:
    assert admin_client.patch(detail_url(boss), {"is_active": False}).status_code == 400
    assert admin_client.patch(detail_url(boss), {"is_superuser": False}).status_code == 400
    boss.refresh_from_db()
    assert boss.is_active and boss.is_superuser


def test_deactivating_blacklists_refresh_tokens(
    admin_client: APIClient, api_client: APIClient, user: User
) -> None:
    refresh = api_client.post(
        "/api/auth/token/", {"username": "alice", "password": "s3cret-pass-123"}
    ).data["refresh"]
    assert admin_client.patch(detail_url(user), {"is_active": False}).status_code == 200
    assert api_client.post("/api/auth/token/refresh/", {"refresh": refresh}).status_code == 401


# --- delete ----------------------------------------------------------------


def test_delete_user(admin_client: APIClient, user: User) -> None:
    assert admin_client.delete(detail_url(user)).status_code == 204
    assert not User.objects.filter(pk=user.pk).exists()


def test_cannot_delete_self(admin_client: APIClient, boss: User) -> None:
    assert admin_client.delete(detail_url(boss)).status_code == 400
    assert User.objects.filter(pk=boss.pk).exists()


def test_me_reports_superuser(admin_client: APIClient, auth_client: APIClient) -> None:
    assert admin_client.get("/api/auth/me/").json()["is_superuser"] is True
    assert auth_client.get("/api/auth/me/").json()["is_superuser"] is False
