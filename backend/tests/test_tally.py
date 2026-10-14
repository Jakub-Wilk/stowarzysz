from itertools import permutations

from voting.tally import count_picks, schulze, top_ids

ABC = ["a", "b", "c"]


def test_count_picks_includes_every_option() -> None:
    assert count_picks(ABC, [["a"], ["a", "c"], []]) == {"a": 2, "b": 0, "c": 1}


def test_top_ids_returns_ties_in_option_order_and_nothing_without_votes() -> None:
    assert top_ids(ABC, {"a": 1, "b": 2, "c": 2}) == ["b", "c"]
    assert top_ids(ABC, {"a": 0, "b": 0, "c": 0}) == []


def test_schulze_condorcet_winner_wins() -> None:
    result = schulze(ABC, [["a", "b", "c"], ["a", "c", "b"], ["b", "a", "c"]])
    assert result.winners == ["a"]
    assert result.condorcet_winner == "a"
    assert result.wins == {"a": 2, "b": 1, "c": 0}
    assert result.pairwise["a"]["b"] == 2 and result.pairwise["b"]["a"] == 1


def test_schulze_resolves_a_cycle_with_strongest_paths() -> None:
    # a beats b 6-3, b beats c 7-2, c beats a 5-4: a cycle, and the strongest paths pick a
    rankings = [["a", "b", "c"]] * 4 + [["b", "c", "a"]] * 3 + [["c", "a", "b"]] * 2
    result = schulze(ABC, rankings)
    assert result.condorcet_winner is None
    assert result.winners == ["a"]
    assert result.wins == {"a": 2, "b": 1, "c": 0}


def test_schulze_a_true_tie_has_several_winners() -> None:
    result = schulze(["a", "b"], [["a", "b"], ["b", "a"]])
    assert result.winners == ["a", "b"]
    assert result.condorcet_winner is None


def test_schulze_partial_rankings_put_unranked_options_last() -> None:
    result = schulze(ABC, [["a"]])
    assert result.winners == ["a"]
    assert result.pairwise["a"] == {"b": 1, "c": 1}
    assert result.pairwise["b"]["c"] == 0 and result.pairwise["c"]["b"] == 0  # tied for last


def test_schulze_a_single_voter_decides() -> None:
    result = schulze(ABC, [["c", "a", "b"]])
    assert result.winners == ["c"]
    assert result.wins == {"a": 1, "b": 0, "c": 2}


def test_schulze_without_ballots_has_no_winner() -> None:
    result = schulze(ABC, [])
    assert result.winners == []
    assert result.condorcet_winner is None
    assert result.wins == {"a": 0, "b": 0, "c": 0}


def test_schulze_does_not_depend_on_ballot_order() -> None:
    rankings = [["a", "b", "c"], ["b", "c", "a"], ["c", "a", "b"], ["a", "c"], ["b"]]
    expected = schulze(ABC, rankings)
    for shuffled in permutations(rankings):
        assert schulze(ABC, list(shuffled)) == expected
