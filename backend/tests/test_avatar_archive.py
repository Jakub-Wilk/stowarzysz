"""TEMPORARY, together with the avatar archive endpoints: remove both."""

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from tests.test_avatars import make_image
from tests.test_profile_votes import POLLS, propose_avatar, vote_all
from voting.models import Poll


@pytest.fixture
def alice(db) -> User:
    return User.objects.create(username="alice")


@pytest.fixture
def bob(db) -> User:
    return User.objects.create(username="bob")


@pytest.fixture
def carol(db) -> User:
    return User.objects.create(username="carol")


LIST = f"{POLLS}avatar-archive/"


def closed_avatar_poll(alice: User, bob: User, carol: User) -> int:
    poll_id = propose_avatar(alice, bob).json()["id"]
    boss = User.objects.get(username="boss")  # everyone takes part, the superuser included
    vote_all(poll_id, {alice: 3, bob: 3, carol: 3, boss: 3})
    return poll_id


def test_archive_requires_auth(api_client: APIClient) -> None:
    assert api_client.get(LIST).status_code == 401
    assert api_client.put(f"{POLLS}1/archive-pictures/").status_code == 401


def test_archive_is_superuser_only(auth_client: APIClient) -> None:
    assert auth_client.get(LIST).status_code == 403
    assert auth_client.put(f"{POLLS}1/archive-pictures/").status_code == 403


def test_archive_lists_closed_avatar_polls_and_stores_pictures(
    admin_client: APIClient, alice: User, bob: User, carol: User
) -> None:
    poll_id = closed_avatar_poll(alice, bob, carol)
    Poll.objects.filter(pk=poll_id).update(proposed_avatar="")  # as if closed before the fix
    assert [p["id"] for p in admin_client.get(LIST).json()] == [poll_id]
    resp = admin_client.put(
        f"{POLLS}{poll_id}/archive-pictures/",
        {"previous": make_image(), "proposed": make_image()},
        format="multipart",
    )
    assert resp.status_code == 200, resp.content
    body = resp.json()
    assert body["previous_avatar_url"].endswith(".webp")
    assert body["proposed_avatar_url"].endswith(".webp")


def test_archive_rejects_open_polls(admin_client: APIClient, alice: User, bob: User) -> None:
    poll_id = propose_avatar(alice, bob).json()["id"]
    resp = admin_client.put(
        f"{POLLS}{poll_id}/archive-pictures/", {"previous": make_image()}, format="multipart"
    )
    assert resp.status_code == 404
