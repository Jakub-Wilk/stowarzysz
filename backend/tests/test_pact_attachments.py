from io import BytesIO
from pathlib import Path

import pytest
from django.core.files.uploadedfile import SimpleUploadedFile
from PIL import Image

from accounts import avatars
from pacts.models import Pact, PactAttachment
from tests.test_avatars import make_image
from tests.test_pact_flows import (
    accept,
    alice,
    bob,
    carol,
    client_for,
    dave,
    events,
    make_bet,
    pushes,
    url,
)

pytestmark = pytest.mark.usefixtures("run_on_commit")


@pytest.fixture(autouse=True)
def _media_root(settings, tmp_path: Path) -> None:
    settings.MEDIA_ROOT = tmp_path / "media"


def files(settings) -> list[Path]:
    return [p for p in Path(settings.MEDIA_ROOT).rglob("*") if p.is_file()]


def attach(user, pact, file=None, **extra):
    return client_for(user).post(
        url(pact, "attachments/"), {"image": file or make_image(), **extra}, format="multipart"
    )


def test_photos_keep_their_aspect_ratio_and_are_capped() -> None:
    wide = avatars.process_photo(make_image((3000, 1000)))
    with Image.open(BytesIO(wide.read())) as stored:
        assert stored.format == "WEBP" and stored.size == (1600, 533)
    small = avatars.process_photo(make_image((300, 200)))
    with Image.open(BytesIO(small.read())) as stored:
        assert stored.size == (300, 200)  # never enlarged, never cropped


def test_endpoints_require_auth(api_client, alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert api_client.post(url(pact, "attachments/")).status_code == 401
    assert api_client.delete(url(pact, "attachments/1/")).status_code == 401


def test_a_participant_can_attach_a_picture_shown_in_the_detail(alice, bob, settings) -> None:
    pact = make_bet(alice, (bob, 1000))
    resp = attach(alice, pact, caption=" umowa na serwetce ")
    assert resp.status_code == 201, resp.json()
    [shown] = resp.json()["attachments"]
    assert shown["caption"] == "umowa na serwetce"
    assert shown["url"].startswith("/api/media/pact_attachments/") and shown["url"].endswith(
        ".webp"
    )
    assert shown["uploaded_by"]["username"] == "alice"
    assert len(files(settings)) == 1
    assert client_for(bob).get(url(pact)).json()["actions"]["can_attach"] is True


def test_invited_opponents_can_attach_too_and_it_works_after_resolution(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert attach(bob, pact).status_code == 201
    accept(bob, pact)
    Pact.objects.filter(pk=pact.pk).update(status=Pact.Status.RESOLVED)
    assert attach(alice, pact).status_code == 201
    assert PactAttachment.objects.filter(pact=pact).count() == 2


def test_outsiders_cannot_attach(alice, bob, dave) -> None:
    pact = make_bet(alice, (bob, 1000))
    assert attach(dave, pact).status_code == 403
    assert client_for(dave).get(url(pact)).json()["actions"]["can_attach"] is False
    assert PactAttachment.objects.count() == 0


def test_declined_participants_lose_the_right(alice, bob) -> None:
    pact = make_bet(alice, (bob, 1000))
    client_for(bob).post(url(pact, "respond/"), {"accept": False}, format="json")
    assert attach(bob, pact).status_code == 403


def test_bad_uploads_are_rejected_and_leave_nothing_behind(alice, bob, settings) -> None:
    pact = make_bet(alice, (bob, 1000))
    junk = SimpleUploadedFile("x.png", b"not an image", content_type="image/png")
    assert attach(alice, pact, junk).status_code == 400
    huge = SimpleUploadedFile(
        "x.png", b"0" * (avatars.MAX_UPLOAD_BYTES + 1), content_type="image/png"
    )
    assert attach(alice, pact, huge).status_code == 400
    assert (
        client_for(alice).post(url(pact, "attachments/"), {}, format="multipart").status_code == 400
    )
    assert PactAttachment.objects.count() == 0 and files(settings) == []


def test_the_number_of_pictures_is_capped(alice, bob, monkeypatch) -> None:
    monkeypatch.setattr("pacts.services.MAX_ATTACHMENTS", 2)
    pact = make_bet(alice, (bob, 1000))
    assert attach(alice, pact).status_code == 201
    assert attach(alice, pact).status_code == 201
    assert attach(alice, pact).status_code == 400


def test_the_uploader_or_the_creator_can_delete(alice, bob, carol, settings) -> None:
    pact = make_bet(alice, (bob, 1000), (carol, 1500))
    mine = attach(bob, pact).json()["attachments"][0]["id"]
    assert client_for(carol).delete(url(pact, f"attachments/{mine}/")).status_code == 403
    assert client_for(bob).delete(url(pact, f"attachments/{mine}/")).status_code == 200
    assert files(settings) == []  # the file goes with the row

    theirs = attach(carol, pact).json()["attachments"][0]["id"]
    resp = client_for(alice).delete(url(pact, f"attachments/{theirs}/"))  # the host may
    assert resp.status_code == 200 and resp.json()["attachments"] == []
    assert client_for(alice).delete(url(pact, f"attachments/{theirs}/")).status_code == 404


def test_deleting_a_pact_removes_its_files(alice, bob, settings) -> None:
    pact = make_bet(alice, (bob, 1000))
    attach(alice, pact)
    pact.delete()
    assert files(settings) == []


def test_pictures_are_visible_to_every_member(alice, bob, dave) -> None:
    pact = make_bet(alice, (bob, 1000))
    attach(alice, pact)
    assert len(client_for(dave).get(url(pact)).json()["attachments"]) == 1
