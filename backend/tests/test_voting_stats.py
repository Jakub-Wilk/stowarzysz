from typing import Any

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from voting import services
from voting.models import Poll


@pytest.fixture(autouse=True)
def _quiet(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("django.db.transaction.on_commit", lambda fn, **kw: None)  # no events/push


def make(*names: str) -> list[User]:
    return [User.objects.create(username=n) for n in names]


def run_poll(creator: User, others: list[User], ballots: dict[User, int | str]) -> Poll:
    """Create a poll and apply ballots (an int = score, "veto" = veto), then end it early."""
    poll = services.create_poll(
        creator=creator,
        title="t",
        kind_key="score",
        config={},
        participant_ids=[u.pk for u in others],
    )
    for user, value in ballots.items():
        if value == "veto":
            services.veto(poll.pk, user)
        else:
            services.cast_ballot(poll.pk, user, {"value": value})
    poll.refresh_from_db()
    if poll.status == Poll.Status.OPEN:
        services.close_early(poll.pk, creator)
    return poll


def stats_by_user(client: APIClient) -> dict[str, dict[str, Any]]:
    rows = client.get("/api/auth/users/").json()
    return {r["username"]: r["voting"] for r in rows}


def test_stats_from_closed_polls(admin_client: APIClient) -> None:
    ann, ben, cat = make("ann", "ben", "cat")
    run_poll(ann, [ben, cat], {ann: 4, ben: 2, cat: "veto"})  # ann 4; ben 2; cat -5 (veto)
    run_poll(ann, [ben], {ann: 0, ben: -2})  # ann 0; ben -2

    stats = stats_by_user(admin_client)
    assert stats["ann"] == {
        "votes_cast": 2,
        "average_score": 2.0,
        "veto_count": 0,
        "veto_percent": 0.0,
    }
    assert stats["ben"]["votes_cast"] == 2 and stats["ben"]["average_score"] == 0.0
    assert stats["cat"] == {
        "votes_cast": 1,
        "average_score": -5.0,
        "veto_count": 1,
        "veto_percent": 100.0,
    }


def test_veto_percent_is_per_cast_ballot(admin_client: APIClient) -> None:
    ann, ben = make("ann", "ben")
    for ballot in ("veto", 5, 5):
        run_poll(ann, [ben], {ben: ballot, ann: 0})
    ben_stats = stats_by_user(admin_client)["ben"]
    assert ben_stats["votes_cast"] == 3 and ben_stats["veto_count"] == 1
    assert ben_stats["veto_percent"] == 33.3
    assert ben_stats["average_score"] == pytest.approx(5 / 3, abs=0.01)


def test_open_polls_do_not_leak_into_stats(admin_client: APIClient) -> None:
    ann, ben = make("ann", "ben")
    poll = services.create_poll(
        creator=ann, title="t", kind_key="score", config={}, participant_ids=[ben.pk]
    )
    services.cast_ballot(poll.pk, ben, {"value": 5})
    assert stats_by_user(admin_client)["ben"]["votes_cast"] == 0  # still hidden


def test_nothing_cast_means_no_average(admin_client: APIClient) -> None:
    make("ann")
    assert stats_by_user(admin_client)["ann"] == {
        "votes_cast": 0,
        "average_score": None,
        "veto_count": 0,
        "veto_percent": 0.0,
    }


def test_non_voters_in_a_closed_poll_are_not_counted(admin_client: APIClient) -> None:
    ann, ben = make("ann", "ben")
    run_poll(ann, [ben], {ann: 3})  # ben never voted
    assert stats_by_user(admin_client)["ben"]["votes_cast"] == 0


def test_other_kinds_are_ignored(admin_client: APIClient, monkeypatch: pytest.MonkeyPatch) -> None:
    ann, ben = make("ann", "ben")
    poll = run_poll(ann, [ben], {ann: 1, ben: 1})
    Poll.objects.filter(pk=poll.pk).update(kind="yesno")
    assert stats_by_user(admin_client)["ann"]["votes_cast"] == 0


def test_detail_and_create_include_stats(admin_client: APIClient) -> None:
    ann, ben = make("ann", "ben")
    run_poll(ann, [ben], {ann: 2, ben: 4})
    assert admin_client.get(f"/api/auth/users/{ben.pk}/").json()["voting"]["average_score"] == 4.0
    created = admin_client.post("/api/auth/users/", {"username": "new"}).json()
    assert created["voting"] == {
        "votes_cast": 0,
        "average_score": None,
        "veto_count": 0,
        "veto_percent": 0.0,
    }


def test_stats_use_one_query(admin_client: APIClient, django_assert_max_num_queries) -> None:
    names = [f"u{i}" for i in range(6)]
    users = make(*names)
    run_poll(users[0], users[1:], {u: 1 for u in users})
    with django_assert_max_num_queries(6):  # auth + list; not one per user
        assert admin_client.get("/api/auth/users/").status_code == 200
