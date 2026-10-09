import pytest
from rest_framework.test import APIClient

from accounts.models import User

LIST_URL = "/api/auth/users/"


def detail_url(user: User) -> str:
    return f"{LIST_URL}{user.pk}/"


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


def test_endpoints_require_superuser(auth_client: APIClient, user: User) -> None:
    assert auth_client.get(LIST_URL).status_code == 403
    assert auth_client.post(LIST_URL, {"username": "x"}).status_code == 403
    assert auth_client.get(detail_url(user)).status_code == 403
    assert auth_client.patch(detail_url(user), {"is_active": False}).status_code == 403
    assert auth_client.delete(detail_url(user)).status_code == 403
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
        "is_active": True,
        "is_superuser": False,
        "has_password": True,
        "avatar_url": None,
        "voting": {"votes_cast": 0, "average_score": None, "veto_count": 0, "veto_percent": 0.0},
        "pacts": {
            "won": 0,
            "lost": 0,
            "draw": 0,
            "money_won": 0,
            "money_lost": 0,
            "by_kind": {},
        },
    }


def test_create_user_is_passwordless(admin_client: APIClient) -> None:
    resp = admin_client.post(LIST_URL, {"username": "bob"})
    assert resp.status_code == 201
    body = resp.json()
    assert body["has_password"] is False
    assert body["is_active"] is True
    assert body["is_superuser"] is False
    assert not User.objects.get(username="bob").has_usable_password()


def test_create_superuser(admin_client: APIClient) -> None:
    resp = admin_client.post(LIST_URL, {"username": "root2", "is_superuser": True})
    assert resp.status_code == 201
    assert User.objects.get(username="root2").is_superuser


def test_create_rejects_duplicate_username(admin_client: APIClient, user: User) -> None:
    assert admin_client.post(LIST_URL, {"username": "alice"}).status_code == 400


def test_retrieve_user(admin_client: APIClient, user: User) -> None:
    resp = admin_client.get(detail_url(user))
    assert resp.status_code == 200
    assert resp.json()["username"] == "alice"


def test_unknown_user_is_404(admin_client: APIClient) -> None:
    assert admin_client.get(f"{LIST_URL}9999/").status_code == 404


# --- update ----------------------------------------------------------------


def test_patch_updates_username(admin_client: APIClient, user: User) -> None:
    resp = admin_client.patch(detail_url(user), {"username": "alicia"})
    assert resp.status_code == 200
    user.refresh_from_db()
    assert user.username == "alicia"
    assert user.check_password("s3cret-pass-123")  # password untouched


def test_removed_profile_fields_are_ignored(admin_client: APIClient, user: User) -> None:
    resp = admin_client.patch(detail_url(user), {"first_name": "X", "email": "x@y.z"})
    assert resp.status_code == 200
    assert "first_name" not in resp.json() and "email" not in resp.json()


def test_patch_cannot_set_password_or_id(admin_client: APIClient, user: User) -> None:
    admin_client.patch(detail_url(user), {"password": "hacked-pass-1", "has_password": False})
    user.refresh_from_db()
    assert user.check_password("s3cret-pass-123")


def test_put_is_not_allowed(admin_client: APIClient, user: User) -> None:
    assert admin_client.put(detail_url(user), {"username": "alice"}).status_code == 405


def test_patch_rejects_duplicate_username(admin_client: APIClient, user: User, boss: User) -> None:
    assert admin_client.patch(detail_url(user), {"username": "boss"}).status_code == 400


def test_promote_and_demote(admin_client: APIClient, user: User) -> None:
    admin_client.patch(detail_url(user), {"is_superuser": True})
    user.refresh_from_db()
    assert user.is_superuser
    admin_client.patch(detail_url(user), {"is_superuser": False})
    user.refresh_from_db()
    assert not user.is_superuser


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
