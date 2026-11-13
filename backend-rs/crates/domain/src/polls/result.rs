//! Final results, computed once when a poll closes and then frozen. The JSON shapes are the
//! Django backend's, so stored history and new results look alike.

use serde_json::{Value, json};

use super::ballot::{Ballot, picks};
use super::config::PollConfig;
use super::{PollKind, tally};

/// One counted ballot (abstentions are not counted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub ballot: Ballot,
    pub vetoed: bool,
}

/// The result of a poll of `kind` from the ballots cast. `abstain_count` is recorded for kinds
/// that allow abstaining.
pub fn compute(
    kind: PollKind,
    config: &PollConfig,
    entries: &[Entry],
    abstain_count: Option<i64>,
) -> Value {
    let mut result = match (kind, config) {
        (PollKind::Score, _) => score(entries),
        (PollKind::Nickname | PollKind::Avatar, _) => profile_change(entries),
        (PollKind::PactRuling, _) => ruling(entries),
        (PollKind::ResolutionJudgment, _) => judgment(entries),
        (PollKind::SingleChoice | PollKind::MultiChoice, PollConfig::Options(options)) => {
            let ids = options.ids();
            let counts = tally::count_picks(&ids, entries.iter().map(|e| picks(&e.ballot)));
            picks_result(config, entries.len(), &counts)
        }
        (PollKind::RankedChoice, PollConfig::Options(options)) => {
            ranked(config, &options.ids(), entries)
        }
        _ => json!({"votes_cast": entries.len()}),
    };
    if let (Some(count), Some(object)) = (
        abstain_count.filter(|_| kind.rules().allows_abstain),
        result.as_object_mut(),
    ) {
        object.insert("abstain_count".into(), json!(count));
    }
    result
}

fn values(entries: &[Entry]) -> impl Iterator<Item = i64> + '_ {
    entries.iter().map(|e| match e.ballot {
        Ballot::Score { value } => value,
        _ => 0,
    })
}

fn score(entries: &[Entry]) -> Value {
    let vetoes = entries.iter().filter(|e| e.vetoed).count();
    let count = i64::try_from(entries.len()).unwrap_or(i64::MAX);
    let sum: i64 = values(entries).sum();
    let (score, tone) = if count == 0 {
        (Value::Null, Value::Null)
    } else {
        let tone = match sum.signum() {
            1 => "positive",
            -1 => "negative",
            _ => "neutral",
        };
        (json!(round_mean(sum, count)), json!(tone))
    };
    json!({"votes_cast": entries.len(), "veto_count": vetoes, "vetoed": vetoes > 0, "score": score, "tone": tone})
}

/// `sum / count` to 3 decimals, half to even (Python's `round(float(mean), 3)`: every tie is a
/// dyadic fraction, exact in binary, so exact arithmetic gives the same answer).
fn round_mean(sum: i64, count: i64) -> f64 {
    let scaled = i128::from(sum) * 1000;
    let count = i128::from(count);
    let quotient = scaled.div_euclid(count);
    let remainder = scaled.rem_euclid(count);
    let rounded = match (2 * remainder).cmp(&count) {
        std::cmp::Ordering::Less => quotient,
        std::cmp::Ordering::Greater => quotient + 1,
        std::cmp::Ordering::Equal => quotient + (quotient & 1),
    };
    #[allow(clippy::cast_precision_loss)] // |rounded| <= 5000
    let value = rounded as f64 / 1000.0;
    value
}

/// Passes at any positive average (an empty vote does not pass); never vetoed.
fn profile_change(entries: &[Entry]) -> Value {
    let mut result = score(entries);
    let approved = !entries.is_empty() && values(entries).sum::<i64>() > 0;
    result["approved"] = json!(approved);
    result["tone"] = json!(if approved { "positive" } else { "negative" });
    result
}

/// A strict majority of the votes cast upholds the claim; a tie or no votes overrules it.
fn ruling(entries: &[Entry]) -> Value {
    let upheld = entries
        .iter()
        .filter(|e| matches!(e.ballot, Ballot::Upheld { upheld: true }))
        .count();
    let rejected = entries.len() - upheld;
    let approved = upheld > rejected;
    json!({
        "votes_cast": entries.len(), "upheld": upheld, "rejected": rejected,
        "approved": approved, "tone": if approved { "positive" } else { "negative" },
    })
}

/// A strict majority decides; a tie (or no votes) decides nothing.
fn judgment(entries: &[Entry]) -> Value {
    let kept = entries
        .iter()
        .filter(|e| matches!(e.ballot, Ballot::Kept { kept: true }))
        .count();
    let broken = entries.len() - kept;
    let decided = kept != broken;
    let tone = match (decided, kept > broken) {
        (false, _) => "neutral",
        (true, true) => "positive",
        (true, false) => "negative",
    };
    json!({
        "votes_cast": entries.len(), "kept": kept, "broken": broken,
        "decided": decided, "approved": kept > broken, "tone": tone,
    })
}

fn option_rows(
    config: &PollConfig,
    counts: &std::collections::BTreeMap<String, u32>,
) -> Vec<(usize, Value)> {
    let Some(options) = config.options() else {
        return Vec::new();
    };
    options
        .options
        .iter()
        .enumerate()
        .map(|(order, o)| (order, json!({"id": o.id, "label": o.label, "count": counts.get(&o.id).copied().unwrap_or(0)})))
        .collect()
}

fn picks_result(
    config: &PollConfig,
    votes_cast: usize,
    counts: &std::collections::BTreeMap<String, u32>,
) -> Value {
    let ids = config
        .options()
        .map(super::OptionsConfig::ids)
        .unwrap_or_default();
    let mut rows = option_rows(config, counts);
    rows.sort_by_key(|(order, row)| {
        (
            std::cmp::Reverse(row["count"].as_u64().unwrap_or(0)),
            *order,
        )
    });
    let winners = tally::top_ids(&ids, counts);
    json!({
        "votes_cast": votes_cast,
        "options": rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
        "tie": winners.len() > 1,
        "tone": if winners.len() == 1 { "positive" } else { "neutral" },
        "winners": winners,
    })
}

fn ranked(config: &PollConfig, ids: &[&str], entries: &[Entry]) -> Value {
    let rankings: Vec<Vec<&str>> = entries
        .iter()
        .map(|e| match &e.ballot {
            Ballot::Ranking { ranking } => ranking.iter().map(String::as_str).collect(),
            _ => Vec::new(),
        })
        .collect();
    let outcome = tally::schulze(ids, &rankings);
    let first_choices = tally::count_picks(
        ids,
        rankings.iter().map(|r| r.iter().take(1).copied().collect()),
    );
    let mut rows = option_rows(config, &first_choices);
    for (_, row) in &mut rows {
        let id = row["id"].as_str().unwrap_or_default().to_owned();
        row["wins"] = json!(outcome.wins.get(&id).copied().unwrap_or(0));
    }
    rows.sort_by_key(|(order, row)| (std::cmp::Reverse(row["wins"].as_u64().unwrap_or(0)), *order));
    json!({
        "votes_cast": entries.len(),
        "options": rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
        "tie": outcome.winners.len() > 1,
        "tone": if outcome.winners.len() == 1 { "positive" } else { "neutral" },
        "winners": outcome.winners,
        "pairwise": outcome.pairwise,
        "condorcet_winner": outcome.condorcet_winner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polls::config::new_options;

    fn scores(values: &[(i64, bool)]) -> Vec<Entry> {
        values
            .iter()
            .map(|(value, vetoed)| Entry {
                ballot: Ballot::Score { value: *value },
                vetoed: *vetoed,
            })
            .collect()
    }

    #[test]
    fn score_tone_follows_the_sign_of_the_mean() {
        let result = compute(
            PollKind::Score,
            &PollConfig::Score {},
            &scores(&[(5, false), (-5, true), (3, false)]),
            Some(1),
        );
        assert_eq!(
            result,
            json!({"votes_cast": 3, "veto_count": 1, "vetoed": true, "score": 1.0, "tone": "positive", "abstain_count": 1})
        );
        assert_eq!(
            compute(
                PollKind::Score,
                &PollConfig::Score {},
                &scores(&[(1, false), (-1, false)]),
                None
            )["tone"],
            "neutral"
        );
        let empty = compute(PollKind::Score, &PollConfig::Score {}, &[], Some(2));
        assert_eq!(
            (empty["score"].clone(), empty["tone"].clone()),
            (Value::Null, Value::Null)
        );
    }

    #[test]
    fn scores_round_like_python() {
        assert!((round_mean(7, 3) - 2.333).abs() < 1e-12);
        assert!((round_mean(-7, 3) - -2.333).abs() < 1e-12);
        assert!(
            (round_mean(1, 16) - 0.062).abs() < 1e-12,
            "0.0625 rounds half to even"
        ); // Python: round(1/16, 3) == 0.062
        assert!((round_mean(3, 16) - 0.188).abs() < 1e-12); // 0.1875 -> 0.188
        assert!((round_mean(2, 3) - 0.667).abs() < 1e-12);
    }

    #[test]
    fn profile_changes_pass_at_any_positive_average() {
        let pass = compute(
            PollKind::Nickname,
            &PollConfig::Score {},
            &scores(&[(1, false), (0, false)]),
            None,
        );
        assert_eq!(
            (pass["approved"].clone(), pass["tone"].clone()),
            (json!(true), json!("positive"))
        );
        assert!(pass.get("abstain_count").is_none());
        assert_eq!(
            compute(
                PollKind::Avatar,
                &PollConfig::Score {},
                &scores(&[(1, false), (-1, false)]),
                None
            )["approved"],
            false
        );
        assert_eq!(
            compute(PollKind::Avatar, &PollConfig::Score {}, &[], None)["approved"],
            false
        );
    }

    #[test]
    fn rulings_and_judgments_need_a_strict_majority() {
        let upheld = |values: &[bool]| {
            values
                .iter()
                .map(|v| Entry {
                    ballot: Ballot::Upheld { upheld: *v },
                    vetoed: false,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            compute(
                PollKind::PactRuling,
                &PollConfig::Score {},
                &upheld(&[true, false]),
                None
            )["approved"],
            false
        );
        assert_eq!(
            compute(
                PollKind::PactRuling,
                &PollConfig::Score {},
                &upheld(&[true, true, false]),
                None
            )["approved"],
            true
        );
        let kept = |values: &[bool]| {
            values
                .iter()
                .map(|v| Entry {
                    ballot: Ballot::Kept { kept: *v },
                    vetoed: false,
                })
                .collect::<Vec<_>>()
        };
        let tie = compute(
            PollKind::ResolutionJudgment,
            &PollConfig::Score {},
            &kept(&[true, false]),
            Some(0),
        );
        assert_eq!(
            (tie["decided"].clone(), tie["tone"].clone()),
            (json!(false), json!("neutral"))
        );
        assert_eq!(
            compute(
                PollKind::ResolutionJudgment,
                &PollConfig::Score {},
                &kept(&[false]),
                None
            )["tone"],
            "negative"
        );
    }

    #[test]
    fn options_results_rank_rows_and_report_ties() {
        let labels: Vec<String> = ["A", "B", "C"].iter().map(|l| (*l).to_owned()).collect();
        let config =
            PollConfig::Options(new_options(PollKind::SingleChoice, &labels, false, None).unwrap());
        let ballot = |id: &str| Entry {
            ballot: Ballot::Choice { choice: id.into() },
            vetoed: false,
        };
        let result = compute(
            PollKind::SingleChoice,
            &config,
            &[ballot("o3"), ballot("o2"), ballot("o3")],
            Some(0),
        );
        assert_eq!(result["winners"], json!(["o3"]));
        assert_eq!(
            result["options"][0],
            json!({"id": "o3", "label": "C", "count": 2})
        );
        assert_eq!(
            result["options"][2]["id"], "o1",
            "zero counts keep option order"
        );
        let tie = compute(
            PollKind::SingleChoice,
            &config,
            &[ballot("o1"), ballot("o2")],
            None,
        );
        assert_eq!(
            (tie["tie"].clone(), tie["tone"].clone()),
            (json!(true), json!("neutral"))
        );
        let none = compute(PollKind::SingleChoice, &config, &[], None);
        assert_eq!(none["winners"], json!([]));
    }

    #[test]
    fn ranked_results_carry_the_schulze_details() {
        let labels: Vec<String> = ["A", "B", "C"].iter().map(|l| (*l).to_owned()).collect();
        let config =
            PollConfig::Options(new_options(PollKind::RankedChoice, &labels, false, None).unwrap());
        let rank = |ids: &[&str]| Entry {
            ballot: Ballot::Ranking {
                ranking: ids.iter().map(|i| (*i).to_owned()).collect(),
            },
            vetoed: false,
        };
        let result = compute(
            PollKind::RankedChoice,
            &config,
            &[rank(&["o2", "o1"]), rank(&["o2", "o3"]), rank(&["o1"])],
            None,
        );
        assert_eq!(result["winners"], json!(["o2"]));
        assert_eq!(result["condorcet_winner"], "o2");
        assert_eq!(
            result["options"][0],
            json!({"id": "o2", "label": "B", "count": 2, "wins": 2})
        );
        assert_eq!(result["pairwise"]["o2"]["o1"], 2);
    }
}
