from io import BytesIO
from pathlib import Path

import pytest
from django.core.files.uploadedfile import SimpleUploadedFile
from PIL import Image
from rest_framework.test import APIClient

from accounts import avatars
from accounts.models import User


@pytest.fixture(autouse=True)
def _media_root(settings, tmp_path: Path) -> None:
    settings.MEDIA_ROOT = tmp_path


def make_image(size: tuple[int, int] = (300, 200), fmt: str = "PNG", name: str = "pic.png"):
    buffer = BytesIO()
    Image.new("RGB", size, (200, 30, 60)).save(buffer, format=fmt)
    return SimpleUploadedFile(name, buffer.getvalue(), content_type=f"image/{fmt.lower()}")


def avatar_url(url_user: User) -> str:
    return f"/api/auth/users/{url_user.pk}/avatar/"


def upload(client: APIClient, user: User, file=None):
    return client.put(avatar_url(user), {"avatar": file or make_image()}, format="multipart")


def stored_files(settings) -> list[Path]:
    return [p for p in Path(settings.MEDIA_ROOT).rglob("*") if p.is_file()]


def test_avatar_endpoints_require_auth(api_client: APIClient, user: User) -> None:
    assert api_client.put(avatar_url(user), {}).status_code == 401
    assert api_client.delete(avatar_url(user)).status_code == 401


def test_avatar_endpoints_require_superuser(auth_client: APIClient, user: User, settings) -> None:
    assert upload(auth_client, user).status_code == 403
    assert auth_client.delete(avatar_url(user)).status_code == 403
    assert stored_files(settings) == []


def test_upload_normalises_image(admin_client: APIClient, user: User, settings) -> None:
    resp = upload(admin_client, user)
    assert resp.status_code == 200
    url = resp.json()["avatar_url"]
    assert url.startswith("/api/media/avatars/") and url.endswith(".webp")

    (path,) = stored_files(settings)
    with Image.open(path) as stored:
        assert stored.format == "WEBP"
        assert stored.size == (avatars.AVATAR_SIZE, avatars.AVATAR_SIZE)  # cropped to a square


def test_avatar_is_exposed_to_me_login_list_and_management(
    admin_client: APIClient, api_client: APIClient, user: User
) -> None:
    url = upload(admin_client, user).json()["avatar_url"]
    assert api_client.get("/api/auth/login-users/").json() == [
        {"username": "alice", "avatar_url": url},
        # superuser "boss" has no picture
        {"username": "boss", "avatar_url": None},
    ]
    assert admin_client.get(f"/api/auth/users/{user.pk}/").json()["avatar_url"] == url
    tokens = api_client.post(
        "/api/auth/token/", {"username": "alice", "password": "s3cret-pass-123"}
    ).data
    api_client.credentials(HTTP_AUTHORIZATION=f"Bearer {tokens['access']}")
    assert api_client.get("/api/auth/me/").json()["avatar_url"] == url


def test_replacing_removes_the_old_file(admin_client: APIClient, user: User, settings) -> None:
    first = upload(admin_client, user).json()["avatar_url"]
    second = upload(admin_client, user, make_image(fmt="JPEG", name="p.jpg")).json()["avatar_url"]
    assert first != second
    assert [p.name for p in stored_files(settings)] == [second.rsplit("/", 1)[1]]


def test_delete_removes_picture_and_file(admin_client: APIClient, user: User, settings) -> None:
    upload(admin_client, user)
    resp = admin_client.delete(avatar_url(user))
    assert resp.status_code == 200
    assert resp.json()["avatar_url"] is None
    assert stored_files(settings) == []
    assert admin_client.delete(avatar_url(user)).status_code == 200  # idempotent


def test_deleting_the_user_removes_the_file(admin_client: APIClient, user: User, settings) -> None:
    upload(admin_client, user)
    assert admin_client.delete(f"/api/auth/users/{user.pk}/").status_code == 204
    assert stored_files(settings) == []


def test_rejects_non_images(admin_client: APIClient, user: User, settings) -> None:
    junk = SimpleUploadedFile("x.png", b"definitely not an image", content_type="image/png")
    resp = upload(admin_client, user, junk)
    assert resp.status_code == 400
    assert "avatar" in resp.json()
    assert stored_files(settings) == []


def test_rejects_missing_file(admin_client: APIClient, user: User) -> None:
    assert admin_client.put(avatar_url(user), {}, format="multipart").status_code == 400


def test_rejects_oversized_upload(
    admin_client: APIClient, user: User, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(avatars, "MAX_UPLOAD_BYTES", 10)
    resp = upload(admin_client, user)
    assert resp.status_code == 400
    assert "too large" in str(resp.json())


def test_unknown_user_is_404(admin_client: APIClient) -> None:
    resp = admin_client.put(
        "/api/auth/users/9999/avatar/", {"avatar": make_image()}, format="multipart"
    )
    assert resp.status_code == 404
