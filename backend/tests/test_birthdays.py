from datetime import UTC, date, datetime
from unittest.mock import patch

import pytest
from rest_framework.test import APIClient

from accounts.birthdays import birthday_users, is_birthday, send_birthday_reminders
from accounts.models import User

URL = "/api/auth/birthdays/"


def warsaw(day: int, hour: int, minute: int = 0, month: int = 6) -> datetime:
    """A Warsaw summer time (UTC+2) moment, as an aware datetime."""
    return datetime(2026, month, day, hour - 2, minute, tzinfo=UTC)


@pytest.fixture
def bday(db) -> User:
    return User.objects.create_user("bday", "s3cret-pass-123", birthday=date(1990, 6, 15))


def test_is_birthday_ignores_the_year() -> None:
    assert is_birthday(date(1990, 6, 15), date(2026, 6, 15))
    assert not is_birthday(date(1990, 6, 15), date(2026, 6, 16))
    assert not is_birthday(None, date(2026, 6, 15))


def test_leap_day_is_kept_on_28_february_in_other_years() -> None:
    leap = date(2000, 2, 29)
    assert is_birthday(leap, date(2026, 2, 28))
    assert not is_birthday(leap, date(2026, 3, 1))
    assert is_birthday(leap, date(2028, 2, 29))
    assert not is_birthday(leap, date(2028, 2, 28))


def test_birthday_users_matches_leap_day(db) -> None:
    User.objects.create_user("leaper", "s3cret-pass-123", birthday=date(2000, 2, 29))
    assert [u.username for u in birthday_users(date(2026, 2, 28))] == ["leaper"]
    assert list(birthday_users(date(2028, 2, 28))) == []


def test_endpoint_requires_auth(api_client: APIClient) -> None:
    assert api_client.get(URL).status_code == 401


def test_endpoint_lists_todays_birthdays(auth_client: APIClient, bday: User) -> None:
    with patch("accounts.views.timezone.localdate", return_value=date(2026, 6, 15)):
        resp = auth_client.get(URL)
    assert [p["username"] for p in resp.json()] == ["bday"]
    with patch("accounts.views.timezone.localdate", return_value=date(2026, 6, 16)):
        assert auth_client.get(URL).json() == []


def test_no_reminder_before_8pm(bday: User, user: User) -> None:
    with patch("accounts.birthdays.send_push") as push:
        assert send_birthday_reminders(warsaw(15, 19, 55)) == 0
    push.assert_not_called()


def test_reminder_goes_to_everyone_else_once(bday: User, user: User) -> None:
    with patch("accounts.birthdays.send_push") as push:
        assert send_birthday_reminders(warsaw(15, 20, 1)) == 1
        assert send_birthday_reminders(warsaw(15, 20, 6)) == 0  # already sent today
    push.assert_called_once()
    recipients = list(push.call_args.args[0])
    assert recipients == [user.pk]  # not the birthday person
    assert "bday" in push.call_args.kwargs["body"]


def test_reminder_repeats_next_year(bday: User, user: User) -> None:
    with patch("accounts.birthdays.send_push"):
        assert send_birthday_reminders(warsaw(15, 21)) == 1
        later = datetime(2027, 6, 15, 19, 0, tzinfo=UTC)  # 21:00 Warsaw
        assert send_birthday_reminders(later) == 1


def test_admin_sets_birthday(admin_client: APIClient, user: User) -> None:
    url = f"/api/auth/users/{user.pk}/"
    resp = admin_client.patch(url, {"birthday": "1995-03-04"})
    assert resp.status_code == 200
    assert resp.json()["birthday"] == "1995-03-04"
    user.refresh_from_db()
    assert user.birthday == date(1995, 3, 4)
    assert admin_client.patch(url, {"birthday": None}, format="json").status_code == 200
    user.refresh_from_db()
    assert user.birthday is None


def test_birthday_cannot_be_in_the_future(admin_client: APIClient, user: User) -> None:
    resp = admin_client.patch(f"/api/auth/users/{user.pk}/", {"birthday": "2999-01-01"})
    assert resp.status_code == 400
