"""Vote counting for options polls. Pure functions: no Django, no I/O, fully deterministic."""

from collections.abc import Iterable, Sequence
from dataclasses import dataclass


def count_picks(option_ids: Sequence[str], picks: Iterable[Iterable[str]]) -> dict[str, int]:
    """How many ballots picked each option (one pick per ballot for single choice, any number
    for multi choice). Every option appears, with 0 if nobody picked it."""
    counts = dict.fromkeys(option_ids, 0)
    for ballot_picks in picks:
        for option_id in ballot_picks:
            counts[option_id] += 1
    return counts


def top_ids(option_ids: Sequence[str], counts: dict[str, int]) -> list[str]:
    """The options sharing the highest count, in option order. Empty if nobody got a vote."""
    best = max(counts.values(), default=0)
    if best == 0:
        return []
    return [i for i in option_ids if counts[i] == best]


@dataclass(frozen=True)
class SchulzeResult:
    pairwise: dict[str, dict[str, int]]  # pairwise[a][b]: ballots ranking a above b
    wins: dict[str, int]  # how many other options each one beats (Schulze, not just pairwise)
    winners: list[str]  # beaten by nobody; several means a genuine tie; none: no ballots
    condorcet_winner: str | None  # beats every other option head to head, if there is one


def schulze(option_ids: Sequence[str], rankings: Sequence[Sequence[str]]) -> SchulzeResult:
    """Schulze method, winning-votes variant.

    A ranking lists options best first; options it leaves out count as tied for last, so
    partial rankings are fine. Ids in a ranking must be unique and belong to `option_ids`.
    """
    n = len(option_ids)
    index = {option_id: i for i, option_id in enumerate(option_ids)}
    d = [[0] * n for _ in range(n)]  # d[i][j]: ballots preferring i over j
    for ranking in rankings:
        position = {index[option_id]: rank for rank, option_id in enumerate(ranking)}
        unranked = len(ranking)
        for i in range(n):
            for j in range(n):
                if i != j and position.get(i, unranked) < position.get(j, unranked):
                    d[i][j] += 1

    # strongest paths, counting only head-to-head wins
    p = [[d[i][j] if d[i][j] > d[j][i] else 0 for j in range(n)] for i in range(n)]
    for i in range(n):
        for j in range(n):
            if i == j:
                continue
            for k in range(n):
                if i != k and j != k:
                    p[j][k] = max(p[j][k], min(p[j][i], p[i][k]))

    def beats(i: int, j: int) -> bool:
        return p[i][j] > p[j][i]

    wins = {option_ids[i]: sum(beats(i, j) for j in range(n) if j != i) for i in range(n)}
    winners = [
        option_ids[j]
        for j in range(n)
        if rankings and not any(beats(i, j) for i in range(n) if i != j)
    ]
    condorcet = [
        option_ids[i] for i in range(n) if all(d[i][j] > d[j][i] for j in range(n) if j != i)
    ]
    pairwise = {
        option_ids[i]: {option_ids[j]: d[i][j] for j in range(n) if j != i} for i in range(n)
    }
    return SchulzeResult(
        pairwise=pairwise,
        wins=wins,
        winners=winners,
        condorcet_winner=condorcet[0] if rankings and condorcet else None,
    )
