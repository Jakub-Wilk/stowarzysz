//! Counting option votes. Pure and deterministic; checked against the Python implementation's
//! outputs (`fixtures/differential/picks.json`, `schulze.json`).

use std::collections::BTreeMap;

/// How many ballots picked each option (every option appears, 0 if nobody picked it).
pub fn count_picks<'a>(
    option_ids: &[&str],
    picks: impl IntoIterator<Item = Vec<&'a str>>,
) -> BTreeMap<String, u32> {
    let mut counts: BTreeMap<String, u32> =
        option_ids.iter().map(|id| ((*id).to_owned(), 0)).collect();
    for ballot in picks {
        for id in ballot {
            if let Some(count) = counts.get_mut(id) {
                *count += 1;
            }
        }
    }
    counts
}

/// The options sharing the highest count, in option order; empty if nobody got a vote.
pub fn top_ids(option_ids: &[&str], counts: &BTreeMap<String, u32>) -> Vec<String> {
    let best = counts.values().copied().max().unwrap_or(0);
    if best == 0 {
        return Vec::new();
    }
    option_ids
        .iter()
        .filter(|id| counts.get(**id) == Some(&best))
        .map(|id| (*id).to_owned())
        .collect()
}

/// The Schulze method's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schulze {
    /// `pairwise[a][b]`: ballots ranking `a` above `b`.
    pub pairwise: BTreeMap<String, BTreeMap<String, u32>>,
    /// How many other options each one beats (by strongest paths).
    pub wins: BTreeMap<String, u32>,
    /// Beaten by nobody; several is a genuine tie; none without ballots.
    pub winners: Vec<String>,
    /// Beats every other option head to head, if one does.
    pub condorcet_winner: Option<String>,
}

/// Schulze method, winning-votes variant. A ranking lists options best first; options it
/// leaves out count as tied for last. Ids must be unique and belong to `option_ids`.
pub fn schulze(option_ids: &[&str], rankings: &[Vec<&str>]) -> Schulze {
    let n = option_ids.len();
    let index = |id: &str| option_ids.iter().position(|o| *o == id);
    let mut d = vec![vec![0_u32; n]; n];
    for ranking in rankings {
        let mut position = vec![ranking.len(); n]; // unranked: tied for last
        for (rank, id) in ranking.iter().enumerate() {
            if let Some(i) = index(id) {
                position[i] = rank;
            }
        }
        for i in 0..n {
            for j in 0..n {
                if i != j && position[i] < position[j] {
                    d[i][j] += 1;
                }
            }
        }
    }

    // Strongest paths, counting only head-to-head wins.
    let mut p = vec![vec![0_u32; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j && d[i][j] > d[j][i] {
                p[i][j] = d[i][j];
            }
        }
    }
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            for k in 0..n {
                if i != k && j != k {
                    p[j][k] = p[j][k].max(p[j][i].min(p[i][k]));
                }
            }
        }
    }
    let beats = |i: usize, j: usize| p[i][j] > p[j][i];

    let wins = (0..n)
        .map(|i| {
            (
                option_ids[i].to_owned(),
                u32::try_from((0..n).filter(|&j| j != i && beats(i, j)).count())
                    .unwrap_or(u32::MAX),
            )
        })
        .collect();
    let winners = if rankings.is_empty() {
        Vec::new()
    } else {
        (0..n)
            .filter(|&j| !(0..n).any(|i| i != j && beats(i, j)))
            .map(|j| option_ids[j].to_owned())
            .collect()
    };
    let condorcet_winner = if rankings.is_empty() {
        None
    } else {
        (0..n)
            .find(|&i| (0..n).all(|j| j == i || d[i][j] > d[j][i]))
            .map(|i| option_ids[i].to_owned())
    };
    let pairwise = (0..n)
        .map(|i| {
            let row = (0..n)
                .filter(|&j| j != i)
                .map(|j| (option_ids[j].to_owned(), d[i][j]))
                .collect();
            (option_ids[i].to_owned(), row)
        })
        .collect();
    Schulze {
        pairwise,
        wins,
        winners,
        condorcet_winner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: [&str; 3] = ["o1", "o2", "o3"];

    #[test]
    fn a_condorcet_winner_wins() {
        let outcome = schulze(
            &IDS,
            &[
                vec!["o2", "o1", "o3"],
                vec!["o2", "o3", "o1"],
                vec!["o1", "o2", "o3"],
            ],
        );
        assert_eq!(outcome.winners, ["o2"]);
        assert_eq!(outcome.condorcet_winner.as_deref(), Some("o2"));
    }

    #[test]
    fn a_perfect_cycle_is_a_tie_and_no_ballots_no_winner() {
        let cycle = schulze(
            &IDS,
            &[
                vec!["o1", "o2", "o3"],
                vec!["o2", "o3", "o1"],
                vec!["o3", "o1", "o2"],
            ],
        );
        assert_eq!(cycle.winners, IDS);
        assert_eq!(cycle.condorcet_winner, None);
        let empty = schulze(&IDS, &[]);
        assert!(empty.winners.is_empty() && empty.condorcet_winner.is_none());
    }

    #[test]
    fn ballot_order_does_not_matter() {
        let ballots = vec![
            vec!["o1", "o2"],
            vec!["o3"],
            vec!["o2", "o3", "o1"],
            vec!["o2"],
        ];
        let mut reversed = ballots.clone();
        reversed.reverse();
        assert_eq!(schulze(&IDS, &ballots), schulze(&IDS, &reversed));
    }

    #[test]
    fn picks_count_every_option() {
        let counts = count_picks(&IDS, vec![vec!["o1"], vec!["o1", "o3"]]);
        assert_eq!(counts.values().copied().collect::<Vec<_>>(), [2, 0, 1]);
        assert_eq!(top_ids(&IDS, &counts), ["o1"]);
        assert_eq!(
            top_ids(&IDS, &count_picks(&IDS, Vec::new())),
            Vec::<String>::new()
        );
    }
}
