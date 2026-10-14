"""Single-choice, multi-choice and ranked-choice votes: config, ballots, results, the open option
pool (a participant adds an option with their vote) and the tie-break follow-up."""

from typing import Any

import pytest
from rest_framework.test import APIClient

from accounts.models import User
from voting.models import Poll

POLLS = "/api/polls/"
PIZZA = ["Margherita", "Pepperoni", "Hawajska"]


@pytest.fixture(autouse=True)
def _run_on_commit_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("django.db.transaction.on_commit", lambda fn, **kw: fn())


@pytest.fixture(autouse=True)
def _no_events(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("core.events.notify_user", lambda uid, t, d: None)


@pytest.fixture
def pushes(monkeypatch: pytest.MonkeyPatch) -> list[dict[str, Any]]:
    sent: list[dict[str, Any]] = []

    def fake_send_push(user_ids, **kwargs):
        sent.append({"user_ids": sorted(user_ids), **kwargs})

    monkeypatch.setattr("voting.services.send_push", fake_send_push)
    return sent


@pytest.fixture
def alice(db) -> User:
    return User.objects.create(username="alice")


@pytest.fixture
def bob(db) -> User:
    return User.objects.create(username="bob")


@pytest.fixture
def carol(db) -> User:
    return User.objects.create(username="carol")


@pytest.fixture
def dave(db) -> User:
    return User.objects.create(username="dave")


def client_for(user: User) -> APIClient:
    client = APIClient()
    client.force_authenticate(user)
    return client


def url(poll: Poll | int, suffix: str = "") -> str:
    pk = poll.pk if isinstance(poll, Poll) else poll
    return f"{POLLS}{pk}/{suffix}"


def create(
    creator: User,
    kind: str,
    others: list[User],
    options: list[str] | None = None,
    **config: Any,
) -> Poll:
    body = {
        "title": "Co zamawiamy?",
        "kind": kind,
        "config": {"options": options or PIZZA, **config},
        "participant_ids": [u.pk for u in others],
    }
    resp = client_for(creator).post(POLLS, body, format="json")
    assert resp.status_code == 201, resp.json()
    return Poll.objects.get(pk=resp.json()["id"])


def cast(user: User, poll: Poll, ballot: dict[str, Any]) -> Any:
    return client_for(user).put(url(poll, "ballot/"), {"ballot": ballot}, format="json")


def detail(user: User, poll: Poll) -> dict[str, Any]:
    return client_for(user).get(url(poll)).json()


def fresh(poll: Poll) -> Poll:
    return Poll.objects.get(pk=poll.pk)


def option_id(poll: Poll, label: str) -> str:
    return next(o["id"] for o in fresh(poll).config["options"] if o["label"] == label)


# --- auth -------------------------------------------------------------------


def test_new_endpoints_require_auth(api_client: APIClient, alice, bob) -> None:
    poll = create(alice, "single_choice", [bob])
    assert api_client.post(POLLS, {"kind": "single_choice"}, format="json").status_code == 401
    assert api_client.put(url(poll, "ballot/"), {"ballot": {}}, format="json").status_code == 401
    assert api_client.post(url(poll, "tie-break/")).status_code == 401


# --- config -----------------------------------------------------------------


def test_create_assigns_option_ids_and_trims_labels(alice, bob) -> None:
    poll = create(alice, "single_choice", [bob], options=["  Pizza ", "Sushi   rolls"])
    assert poll.config == {
        "options": [{"id": "o1", "label": "Pizza"}, {"id": "o2", "label": "Sushi rolls"}]
    }


def test_create_keeps_open_options_and_max_choices(alice, bob) -> None:
    poll = create(alice, "multi_choice", [bob], max_choices=2, open_options=True)
    assert poll.config["max_choices"] == 2 and poll.config["open_options"] is True
    assert [o["id"] for o in poll.config["options"]] == ["o1", "o2", "o3"]


@pytest.mark.parametrize(
    ("kind", "config"),
    [
        ("single_choice", {}),
        ("single_choice", {"options": ["Jedna"]}),
        ("single_choice", {"options": [f"Opcja {n}" for n in range(11)]}),
        ("single_choice", {"options": ["Pizza", "pizza"]}),
        ("single_choice", {"options": ["Pizza", "  "]}),
        ("single_choice", {"options": ["Pizza", "x" * 61]}),
        ("single_choice", {"options": ["Pizza", 5]}),
        ("single_choice", {"options": "Pizza, Sushi"}),
        ("single_choice", {"options": ["A", "B"], "max_choices": 1}),
        ("single_choice", {"options": ["A", "B"], "open_options": "tak"}),
        ("single_choice", {"options": ["A", "B"], "tie_break_of": 1}),
        ("multi_choice", {"options": ["A", "B", "C"], "max_choices": 3}),
        ("multi_choice", {"options": ["A", "B", "C"], "max_choices": 0}),
        ("multi_choice", {"options": ["A", "B", "C"], "max_choices": True}),
        ("ranked_choice", {"options": ["A", "B"], "open_options": True}),
    ],
)
def test_invalid_configs_are_rejected(alice, bob, kind: str, config: dict[str, Any]) -> None:
    resp = client_for(alice).post(
        POLLS,
        {"title": "x", "kind": kind, "config": config, "participant_ids": [bob.pk]},
        format="json",
    )
    assert resp.status_code == 400, resp.json()


def test_a_title_is_required(alice, bob) -> None:
    resp = client_for(alice).post(
        POLLS,
        {"kind": "single_choice", "config": {"options": ["A", "B"]}, "participant_ids": [bob.pk]},
        format="json",
    )
    assert resp.status_code == 400


# --- single choice ----------------------------------------------------------


def test_single_choice_counts_and_picks_the_winner(alice, bob, carol) -> None:
    poll = create(alice, "single_choice", [bob, carol])
    assert cast(alice, poll, {"choice": "o2"}).status_code == 200
    assert cast(bob, poll, {"choice": "o2"}).status_code == 200
    assert cast(carol, poll, {"choice": "o1"}).status_code == 200
    closed = fresh(poll)
    assert closed.status == Poll.Status.CLOSED
    assert closed.result["winners"] == ["o2"]
    assert closed.result["tie"] is False and closed.result["tone"] == "positive"
    assert closed.result["votes_cast"] == 3
    assert closed.result["options"] == [
        {"id": "o2", "label": "Pepperoni", "count": 2},
        {"id": "o1", "label": "Margherita", "count": 1},
        {"id": "o3", "label": "Hawajska", "count": 0},
    ]


@pytest.mark.parametrize(
    "ballot",
    [
        {},
        {"choice": "o9"},
        {"choice": 1},
        {"choice": ["o1"]},
        {"choices": ["o1"]},
        {"choice": None},
    ],
)
def test_single_choice_rejects_bad_ballots(alice, bob, ballot: dict[str, Any]) -> None:
    poll = create(alice, "single_choice", [bob])
    assert cast(alice, poll, ballot).status_code == 400


def test_ballots_stay_hidden_until_the_end(alice, bob, carol) -> None:
    poll = create(alice, "single_choice", [bob, carol])
    cast(bob, poll, {"choice": "o1"})
    body = detail(carol, poll)
    assert all("ballot" not in row for row in body["participants"])
    assert detail(bob, poll)["my_ballot"] == {"choice": "o1"}
    client_for(alice).post(url(poll, "close/"))
    revealed = {r["user"]["username"]: r["ballot"] for r in detail(carol, poll)["participants"]}
    assert revealed["bob"] == {"choice": "o1"}


def test_voting_again_replaces_the_ballot(alice, bob) -> None:
    poll = create(alice, "single_choice", [bob])
    cast(alice, poll, {"choice": "o1"})
    cast(alice, poll, {"choice": "o3"})
    cast(bob, poll, {"choice": "o3"})
    assert fresh(poll).result["winners"] == ["o3"]


def test_there_is_no_veto_but_abstaining_works(alice, bob, carol) -> None:
    poll = create(alice, "single_choice", [bob, carol])
    assert client_for(bob).post(url(poll, "veto/")).status_code == 400
    body = detail(bob, poll)
    assert body["can_veto"] is False and body["can_abstain"] is True
    assert client_for(bob).post(url(poll, "abstain/"), {"reason": "Dieta"}).status_code == 200
    cast(alice, poll, {"choice": "o1"})
    cast(carol, poll, {"choice": "o1"})
    result = fresh(poll).result
    assert result["votes_cast"] == 2 and result["abstain_count"] == 1


def test_a_poll_without_votes_has_no_winner(alice, bob) -> None:
    poll = create(alice, "single_choice", [bob])
    assert client_for(alice).post(url(poll, "close/")).status_code == 200
    result = fresh(poll).result
    assert result["winners"] == [] and result["tie"] is False and result["tone"] == "neutral"
    assert result["abstain_count"] == 2


# --- multi choice -----------------------------------------------------------


def test_multi_choice_counts_approvals(alice, bob, carol) -> None:
    poll = create(alice, "multi_choice", [bob, carol])
    cast(alice, poll, {"choices": ["o1", "o2"]})
    cast(bob, poll, {"choices": ["o2"]})
    cast(carol, poll, {"choices": ["o3", "o2"]})
    result = fresh(poll).result
    assert result["winners"] == ["o2"]
    assert {o["id"]: o["count"] for o in result["options"]} == {"o1": 1, "o2": 3, "o3": 1}
    assert result["options"][0]["id"] == "o2"
    assert fresh(poll).status == Poll.Status.CLOSED


def test_multi_choice_stores_choices_in_option_order(alice, bob) -> None:
    poll = create(alice, "multi_choice", [bob])
    cast(alice, poll, {"choices": ["o3", "o1"]})
    assert detail(alice, poll)["my_ballot"] == {"choices": ["o1", "o3"]}


@pytest.mark.parametrize(
    "ballot",
    [{}, {"choices": []}, {"choices": ["o1", "o1"]}, {"choices": ["o7"]}, {"choices": "o1"}],
)
def test_multi_choice_rejects_bad_ballots(alice, bob, ballot: dict[str, Any]) -> None:
    poll = create(alice, "multi_choice", [bob])
    assert cast(alice, poll, ballot).status_code == 400


def test_multi_choice_respects_max_choices(alice, bob) -> None:
    poll = create(alice, "multi_choice", [bob], max_choices=2)
    assert cast(alice, poll, {"choices": ["o1", "o2", "o3"]}).status_code == 400
    assert cast(alice, poll, {"choices": ["o1", "o2"]}).status_code == 200


# --- ranked choice ----------------------------------------------------------


def test_ranked_choice_uses_the_schulze_method(alice, bob, carol, dave) -> None:
    poll = create(alice, "ranked_choice", [bob, carol, dave])
    cast(alice, poll, {"ranking": ["o1", "o2", "o3"]})
    cast(bob, poll, {"ranking": ["o1", "o3", "o2"]})
    cast(carol, poll, {"ranking": ["o2", "o1", "o3"]})
    cast(dave, poll, {"ranking": ["o3", "o1", "o2"]})
    result = fresh(poll).result
    assert result["winners"] == ["o1"] and result["condorcet_winner"] == "o1"
    assert result["pairwise"]["o1"]["o2"] == 3 and result["pairwise"]["o2"]["o1"] == 1
    first = {o["id"]: o["count"] for o in result["options"]}
    assert first == {"o1": 2, "o2": 1, "o3": 1}
    assert result["options"][0]["id"] == "o1" and result["options"][0]["wins"] == 2


def test_ranked_choice_accepts_partial_rankings(alice, bob) -> None:
    poll = create(alice, "ranked_choice", [bob])
    assert cast(alice, poll, {"ranking": ["o2"]}).status_code == 200
    assert cast(bob, poll, {"ranking": ["o2", "o3"]}).status_code == 200
    assert fresh(poll).result["winners"] == ["o2"]


@pytest.mark.parametrize(
    "ballot",
    [{}, {"ranking": []}, {"ranking": ["o1", "o1"]}, {"ranking": ["o4"]}, {"choice": "o1"}],
)
def test_ranked_choice_rejects_bad_ballots(alice, bob, ballot: dict[str, Any]) -> None:
    poll = create(alice, "ranked_choice", [bob])
    assert cast(alice, poll, ballot).status_code == 400


# --- open option pool -------------------------------------------------------


def test_a_participant_can_vote_for_a_new_option(alice, bob, carol) -> None:
    poll = create(alice, "single_choice", [bob, carol], open_options=True)
    resp = cast(bob, poll, {"new_option": "  Capricciosa "})
    assert resp.status_code == 200, resp.json()
    labels = [o["label"] for o in fresh(poll).config["options"]]
    assert labels == [*PIZZA, "Capricciosa"]
    assert detail(bob, poll)["my_ballot"] == {"choice": "o4"}  # ids only, no new_option
    assert "added_by" not in str(fresh(poll).config)
    cast(alice, poll, {"choice": "o4"})
    cast(carol, poll, {"choice": "o1"})
    result = fresh(poll).result
    assert result["winners"] == ["o4"]
    assert {o["id"]: o["label"] for o in result["options"]}["o4"] == "Capricciosa"


def test_a_multi_vote_adds_the_new_option_to_the_picks(alice, bob) -> None:
    poll = create(alice, "multi_choice", [bob], open_options=True, max_choices=2)
    cast(alice, poll, {"choices": ["o1"], "new_option": "Funghi"})
    assert detail(alice, poll)["my_ballot"] == {"choices": ["o1", "o4"]}
    # the new option counts towards the limit
    resp = cast(bob, poll, {"choices": ["o1", "o2"], "new_option": "Diavola"})
    assert resp.status_code == 400
    assert [o["label"] for o in fresh(poll).config["options"]][-1] == "Funghi"  # nothing added


def test_a_duplicate_label_votes_for_the_existing_option(alice, bob, carol, pushes) -> None:
    poll = create(alice, "single_choice", [bob, carol], open_options=True)
    cast(alice, poll, {"choice": "o1"})
    pushes.clear()
    assert cast(bob, poll, {"new_option": "pepperoni"}).status_code == 200
    assert len(fresh(poll).config["options"]) == 3
    assert detail(bob, poll)["my_ballot"] == {"choice": "o2"}
    assert pushes == []


def test_changing_your_vote_can_add_another_option_and_the_first_stays(alice, bob, carol) -> None:
    poll = create(alice, "single_choice", [bob, carol], open_options=True)
    cast(bob, poll, {"new_option": "Capricciosa"})
    cast(bob, poll, {"new_option": "Funghi"})
    options = {o["id"]: o["label"] for o in fresh(poll).config["options"]}
    assert options["o4"] == "Capricciosa" and options["o5"] == "Funghi"
    assert detail(bob, poll)["my_ballot"] == {"choice": "o5"}
    client_for(alice).post(url(poll, "close/"))
    counts = {o["id"]: o["count"] for o in fresh(poll).result["options"]}
    assert counts["o4"] == 0 and counts["o5"] == 1


@pytest.mark.parametrize(
    "ballot",
    [
        {"new_option": ["Funghi", "Diavola"]},
        {"new_option": ""},
        {"new_option": "x" * 61},
        {"new_option": 5},
        {"new_option": "Funghi", "choice": "o1"},
    ],
)
def test_new_option_must_be_one_valid_label(alice, bob, ballot: dict[str, Any]) -> None:
    poll = create(alice, "single_choice", [bob], open_options=True)
    assert cast(alice, poll, ballot).status_code == 400
    assert len(fresh(poll).config["options"]) == 3


@pytest.mark.parametrize("kind", ["single_choice", "multi_choice"])
def test_a_closed_pool_rejects_new_options(alice, bob, kind: str) -> None:
    poll = create(alice, kind, [bob])
    assert cast(alice, poll, {"new_option": "Funghi"}).status_code == 400
    assert len(fresh(poll).config["options"]) == 3


def test_ranked_choice_rejects_new_options(alice, bob) -> None:
    poll = create(alice, "ranked_choice", [bob])
    assert cast(alice, poll, {"new_option": "Funghi"}).status_code == 400


def test_the_pool_stops_at_twenty_options(alice, bob) -> None:
    poll = create(
        alice, "single_choice", [bob], open_options=True, options=[f"O{n}" for n in range(10)]
    )
    for n in range(10):
        assert cast(alice, poll, {"new_option": f"Nowa {n}"}).status_code == 200
    assert len(fresh(poll).config["options"]) == 20
    assert cast(alice, poll, {"new_option": "Jedna za dużo"}).status_code == 400
    assert cast(alice, poll, {"new_option": "nowa 3"}).status_code == 200  # an existing one is fine
    ids = [o["id"] for o in fresh(poll).config["options"]]
    assert ids == [f"o{n}" for n in range(1, 21)]


def test_only_participants_add_options_and_only_while_open(alice, bob, dave) -> None:
    poll = create(alice, "single_choice", [bob], open_options=True)
    assert cast(dave, poll, {"new_option": "Funghi"}).status_code == 403
    client_for(alice).post(url(poll, "close/"))
    assert cast(bob, poll, {"new_option": "Funghi"}).status_code == 409
    assert len(fresh(poll).config["options"]) == 3


def test_a_new_option_pushes_only_to_people_who_already_voted(
    alice, bob, carol, dave, pushes
) -> None:
    poll = create(alice, "single_choice", [bob, carol, dave], open_options=True)
    cast(alice, poll, {"choice": "o1"})
    client_for(bob).post(url(poll, "abstain/"))
    pushes.clear()
    assert cast(carol, poll, {"new_option": "Capricciosa"}).status_code == 200
    assert len(pushes) == 1
    push = pushes[0]
    assert push["user_ids"] == [alice.pk]  # not carol (the adder), bob (abstained) or dave
    assert push["title"] == "Nowa opcja w głosowaniu"
    assert "Capricciosa" in push["body"] and poll.title in push["body"]
    assert push["url"] == f"/voting/{poll.pk}"
    assert carol.username not in push["body"]


def test_the_adder_is_not_pushed_when_they_change_their_own_vote(alice, bob, carol, pushes) -> None:
    poll = create(alice, "single_choice", [bob, carol], open_options=True)
    cast(alice, poll, {"choice": "o1"})
    cast(bob, poll, {"choice": "o2"})
    pushes.clear()
    cast(alice, poll, {"new_option": "Funghi"})
    assert [p["user_ids"] for p in pushes] == [[bob.pk]]


# --- tie-break --------------------------------------------------------------


def tied_single(alice: User, bob: User, carol: User) -> Poll:
    poll = create(alice, "single_choice", [bob, carol])
    cast(bob, poll, {"choice": "o1"})
    cast(carol, poll, {"choice": "o2"})
    client_for(alice).post(url(poll, "close/"))  # the creator never voted
    assert fresh(poll).result["winners"] == ["o1", "o2"]
    return poll


def test_the_creator_can_call_a_tie_break_without_having_voted(alice, bob, carol, pushes) -> None:
    poll = tied_single(alice, bob, carol)
    assert detail(alice, poll)["can_tie_break"] is True
    assert detail(bob, poll)["can_tie_break"] is False
    pushes.clear()
    resp = client_for(alice).post(url(poll, "tie-break/"))
    assert resp.status_code == 201, resp.json()
    body = resp.json()
    assert body["title"] == "Dogrywka: Co zamawiamy?"
    assert body["kind"] == "single_choice" and body["status"] == "open"
    assert [o["label"] for o in body["config"]["options"]] == ["Margherita", "Pepperoni"]
    assert body["config"]["tie_break_of"] == poll.pk
    assert "open_options" not in body["config"]
    follow_up = Poll.objects.get(pk=body["id"])
    assert follow_up.creator == alice
    assert set(follow_up.participants.values_list("user_id", flat=True)) == {
        alice.pk,
        bob.pk,
        carol.pk,
    }
    assert [p["user_ids"] for p in pushes] == [sorted([bob.pk, carol.pk])]  # the new-poll push


def test_asking_again_returns_the_same_follow_up(alice, bob, carol) -> None:
    poll = tied_single(alice, bob, carol)
    first = client_for(alice).post(url(poll, "tie-break/"))
    second = client_for(alice).post(url(poll, "tie-break/"))
    assert (first.status_code, second.status_code) == (201, 200)
    assert first.json()["id"] == second.json()["id"]
    assert Poll.objects.filter(config__tie_break_of=poll.pk).count() == 1
    body = detail(alice, poll)
    assert body["tie_break_poll"] == first.json()["id"] and body["can_tie_break"] is False


def test_the_tie_break_never_keeps_an_open_pool(alice, bob, carol) -> None:
    poll = create(alice, "single_choice", [bob, carol], open_options=True)
    cast(bob, poll, {"choice": "o1"})
    cast(carol, poll, {"choice": "o2"})
    client_for(alice).post(url(poll, "close/"))
    resp = client_for(alice).post(url(poll, "tie-break/"))
    assert resp.status_code == 201
    assert "open_options" not in resp.json()["config"]


def test_only_the_creator_can_call_a_tie_break(alice, bob, carol, dave) -> None:
    poll = tied_single(alice, bob, carol)
    assert client_for(bob).post(url(poll, "tie-break/")).status_code == 403  # a participant
    assert client_for(dave).post(url(poll, "tie-break/")).status_code == 403
    assert client_for(alice).post(url(99999, "tie-break/")).status_code == 404
    assert Poll.objects.filter(config__tie_break_of=poll.pk).count() == 0


def test_a_tie_break_needs_a_finished_poll_that_ended_in_a_tie(alice, bob) -> None:
    open_poll = create(alice, "single_choice", [bob])
    assert client_for(alice).post(url(open_poll, "tie-break/")).status_code == 400
    decided = create(alice, "single_choice", [bob])
    cast(alice, decided, {"choice": "o1"})
    cast(bob, decided, {"choice": "o1"})
    assert client_for(alice).post(url(decided, "tie-break/")).status_code == 400
    assert detail(alice, decided)["can_tie_break"] is False
    empty = create(alice, "single_choice", [bob])
    client_for(alice).post(url(empty, "close/"))
    assert client_for(alice).post(url(empty, "tie-break/")).status_code == 400


def test_a_tied_multi_choice_poll_gets_a_single_choice_follow_up(alice, bob) -> None:
    poll = create(alice, "multi_choice", [bob])
    cast(alice, poll, {"choices": ["o1"]})
    cast(bob, poll, {"choices": ["o2"]})
    resp = client_for(alice).post(url(poll, "tie-break/"))
    assert resp.status_code == 201
    assert resp.json()["kind"] == "single_choice"
    assert [o["label"] for o in resp.json()["config"]["options"]] == ["Margherita", "Pepperoni"]


def test_a_tied_ranked_poll_gets_a_ranked_follow_up_with_only_the_tied_options(alice, bob) -> None:
    poll = create(alice, "ranked_choice", [bob])
    cast(alice, poll, {"ranking": ["o1", "o2", "o3"]})
    cast(bob, poll, {"ranking": ["o2", "o1", "o3"]})
    assert fresh(poll).result["winners"] == ["o1", "o2"]
    resp = client_for(alice).post(url(poll, "tie-break/"))
    assert resp.status_code == 201
    assert resp.json()["kind"] == "ranked_choice"
    assert [o["label"] for o in resp.json()["config"]["options"]] == ["Margherita", "Pepperoni"]
