import uuid
from typing import Any

import pytest
from google.genai import errors
from rest_framework.test import APIClient

from ledger import receipt_ocr
from ledger.receipt_ocr import Receipt
from tests.test_avatars import make_image

URL = "/api/ledger/receipt-ocr/"


def server_error(code: int) -> errors.ServerError:
    return errors.ServerError(code, {"error": {"message": "overloaded"}})


@pytest.fixture
def sent(monkeypatch: pytest.MonkeyPatch) -> list[tuple[int, str, Any]]:
    events: list[tuple[int, str, Any]] = []
    monkeypatch.setattr("core.events.notify_user", lambda uid, t, d: events.append((uid, t, d)))
    return events


class FakeGemini:
    """Outcomes for successive Gemini requests: an exception is raised, anything else returned."""

    def __init__(self) -> None:
        self.outcomes: list[Any] = []
        self.calls = 0

    def __call__(self, client: Any, image: bytes, mime_type: str) -> Receipt:
        self.calls += 1
        outcome = self.outcomes.pop(0)
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


@pytest.fixture
def gemini(monkeypatch: pytest.MonkeyPatch, settings) -> FakeGemini:
    settings.GEMINI_API_KEY = "test-key"
    monkeypatch.setattr(receipt_ocr, "RETRY_DELAY", 0)
    fake = FakeGemini()
    monkeypatch.setattr(receipt_ocr, "_generate", fake)
    return fake


def scan(client: APIClient, job_id: str | None = None) -> Any:
    return client.post(
        URL,
        {"image": make_image(), "job_id": job_id or str(uuid.uuid4())},
        format="multipart",
    )


def test_success_announces_one_request(auth_client, user, sent, gemini) -> None:
    gemini.outcomes.append(Receipt(total=12.5))
    job = str(uuid.uuid4())
    resp = scan(auth_client, job)
    assert resp.status_code == 200
    assert resp.json()["total"] == 12.5
    assert sent == [(user.id, "ledger.ocr.request", {"job_id": job, "attempt": 1})]


def test_503_is_retried(auth_client, sent, gemini) -> None:
    gemini.outcomes.extend([server_error(503), Receipt(total=1.0)])
    resp = scan(auth_client)
    assert resp.status_code == 200
    assert [d["attempt"] for _, _, d in sent] == [1, 2]


def test_gives_up_after_two_retries(auth_client, sent, gemini) -> None:
    gemini.outcomes.extend([server_error(503)] * 3)
    resp = scan(auth_client)
    assert resp.status_code == 503
    assert [d["attempt"] for _, _, d in sent] == [1, 2, 3]
    assert gemini.calls == 3


def test_other_errors_are_not_retried(auth_client, sent, gemini) -> None:
    gemini.outcomes.append(server_error(500))
    resp = scan(auth_client)
    assert resp.status_code == 503
    assert [d["attempt"] for _, _, d in sent] == [1]


def test_missing_key(auth_client, sent, settings) -> None:
    settings.GEMINI_API_KEY = ""
    assert scan(auth_client).status_code == 503
    assert sent == []


def test_rejects_bad_input(auth_client, gemini) -> None:
    resp = auth_client.post(URL, {"image": make_image()}, format="multipart")
    assert resp.status_code == 400
    assert gemini.calls == 0


def test_requires_auth(api_client) -> None:
    assert scan(api_client).status_code == 401
