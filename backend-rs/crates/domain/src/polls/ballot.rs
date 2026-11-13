//! Ballots: what a participant sends, and the cleaned form stored (the Django backend's shapes:
//! `{"value"}`, `{"choice"}`, `{"choices"}`, `{"ranking"}`, `{"upheld"}`, `{"kept"}`).

use serde::{Deserialize, Serialize};

use super::PollKind;
use super::config::{MAX_OPTIONS_OPEN, OptionsConfig, PollConfig, PollOption, clean_label};
use crate::DomainError;

pub const SCORE_MIN: i64 = -5;
pub const SCORE_MAX: i64 = 5;

/// A ballot as sent: only the field of the poll's kind is read, others are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct BallotInput {
    pub value: Option<i64>,
    pub choice: Option<String>,
    pub choices: Option<Vec<String>>,
    pub ranking: Option<Vec<String>>,
    pub upheld: Option<bool>,
    pub kept: Option<bool>,
    /// Open option pools: vote for a new option with this label (added if new).
    pub new_option: Option<String>,
}

/// A cleaned ballot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Ballot {
    Score { value: i64 },
    Choice { choice: String },
    Choices { choices: Vec<String> },
    Ranking { ranking: Vec<String> },
    Upheld { upheld: bool },
    Kept { kept: bool },
}

impl Ballot {
    /// The ballot a veto counts as (score-style kinds only).
    pub fn veto() -> Self {
        Self::Score { value: SCORE_MIN }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// A ballot ready to store, and the option it added to an open pool (if it did).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub ballot: Ballot,
    /// The new option; the caller stores `config` with it appended.
    pub added: Option<PollOption>,
}

fn invalid(code: &'static str, message: impl Into<std::borrow::Cow<'static, str>>) -> DomainError {
    DomainError::new(code, message).on("ballot")
}

/// Validate `input` for a poll of `kind` with `config`, adding a new option to an open pool if
/// the ballot asks for one.
pub fn resolve(
    kind: PollKind,
    config: &PollConfig,
    input: &BallotInput,
) -> Result<Resolved, DomainError> {
    if let Some(label) = &input.new_option {
        let Some(options) = config
            .options()
            .filter(|o| o.open_options && kind != PollKind::RankedChoice)
        else {
            return Err(invalid(
                "options_closed",
                "W tym głosowaniu nie można dodawać opcji.",
            ));
        };
        return add_option(kind, options, input, label);
    }
    validate(kind, config, input).map(|ballot| Resolved {
        ballot,
        added: None,
    })
}

fn add_option(
    kind: PollKind,
    options: &OptionsConfig,
    input: &BallotInput,
    raw_label: &str,
) -> Result<Resolved, DomainError> {
    let label = clean_label(raw_label, "new_option")?;
    let (option_id, added, extended) = if let Some(existing) = options.find_label(&label) {
        // Already there: the vote just goes to it.
        (existing.id.clone(), None, options.clone())
    } else {
        {
            if options.options.len() >= MAX_OPTIONS_OPEN {
                return Err(DomainError::new(
                    "option_limit",
                    format!("Osiągnięto limit {MAX_OPTIONS_OPEN} opcji."),
                )
                .on("new_option"));
            }
            let option = PollOption {
                id: options.next_id(),
                label,
            };
            let mut extended = options.clone();
            extended.options.push(option.clone());
            (option.id.clone(), Some(option), extended)
        }
    };
    let with_new = match kind {
        PollKind::SingleChoice => {
            if input.choice.is_some() {
                return Err(invalid(
                    "choice_and_new_option",
                    "Wybierz istniejącą opcję albo dodaj nową, nie obie naraz.",
                ));
            }
            BallotInput {
                choice: Some(option_id),
                ..BallotInput::default()
            }
        }
        PollKind::MultiChoice => {
            let mut choices = input.choices.clone().unwrap_or_default();
            if !choices.contains(&option_id) {
                choices.push(option_id);
            }
            BallotInput {
                choices: Some(choices),
                ..BallotInput::default()
            }
        }
        _ => {
            return Err(invalid(
                "options_closed",
                "W tym głosowaniu nie można dodawać opcji.",
            ));
        }
    };
    let ballot = validate(kind, &PollConfig::Options(extended), &with_new)?;
    Ok(Resolved { ballot, added })
}

/// Validate a ballot without a new option.
pub fn validate(
    kind: PollKind,
    config: &PollConfig,
    input: &BallotInput,
) -> Result<Ballot, DomainError> {
    match (kind, config) {
        (PollKind::Score | PollKind::Nickname | PollKind::Avatar, _) => {
            let value = input
                .value
                .ok_or_else(|| invalid("score_required", "Podaj ocenę (liczbę całkowitą)."))?;
            if !(SCORE_MIN..=SCORE_MAX).contains(&value) {
                return Err(invalid(
                    "score_out_of_range",
                    format!("Ocena musi mieścić się w zakresie od {SCORE_MIN} do {SCORE_MAX}."),
                ));
            }
            Ok(Ballot::Score { value })
        }
        (PollKind::PactRuling, _) => input
            .upheld
            .map(|upheld| Ballot::Upheld { upheld })
            .ok_or_else(|| {
                invalid(
                    "upheld_required",
                    "Zdecyduj, czy roszczenie się utrzymuje (upheld: true/false).",
                )
            }),
        (PollKind::ResolutionJudgment, _) => {
            input.kept.map(|kept| Ballot::Kept { kept }).ok_or_else(|| {
                invalid(
                    "kept_required",
                    "Zdecyduj, czy postanowienie zostało dotrzymane (kept: true/false).",
                )
            })
        }
        (PollKind::SingleChoice, PollConfig::Options(options)) => match &input.choice {
            Some(choice) if options.contains(choice) => Ok(Ballot::Choice {
                choice: choice.clone(),
            }),
            _ => Err(invalid("invalid_choice", "Wybierz jedną z opcji.")),
        },
        (PollKind::MultiChoice, PollConfig::Options(options)) => {
            let choices = input.choices.as_deref().unwrap_or_default();
            if choices.is_empty() || !all_known_and_unique(options, choices) {
                return Err(invalid(
                    "invalid_choices",
                    "Wybierz co najmniej jedną z opcji, każdą raz.",
                ));
            }
            if let Some(limit) = options.max_choices.filter(|limit| choices.len() > *limit) {
                return Err(invalid(
                    "too_many_choices",
                    format!("Możesz wybrać najwyżej {limit} opcji."),
                ));
            }
            // Stored in option order, so equal ballots look equal.
            let ordered = options
                .ids()
                .into_iter()
                .filter(|id| choices.iter().any(|c| c == id))
                .map(str::to_owned)
                .collect();
            Ok(Ballot::Choices { choices: ordered })
        }
        (PollKind::RankedChoice, PollConfig::Options(options)) => {
            let ranking = input.ranking.as_deref().unwrap_or_default();
            if ranking.is_empty() || !all_known_and_unique(options, ranking) {
                return Err(invalid(
                    "invalid_ranking",
                    "Uszereguj opcje, każdą najwyżej raz.",
                ));
            }
            Ok(Ballot::Ranking {
                ranking: ranking.to_vec(),
            })
        }
        _ => Err(invalid("invalid_ballot", "Nieprawidłowy głos.")),
    }
}

fn all_known_and_unique(options: &OptionsConfig, ids: &[String]) -> bool {
    let mut seen: Vec<&str> = Vec::with_capacity(ids.len());
    ids.iter().all(|id| {
        let fresh = !seen.contains(&id.as_str());
        seen.push(id);
        fresh && options.contains(id)
    })
}

/// The ids a ballot picks, for counting (empty for non-option ballots).
pub fn picks(ballot: &Ballot) -> Vec<&str> {
    match ballot {
        Ballot::Choice { choice } => vec![choice.as_str()],
        Ballot::Choices { choices } => choices.iter().map(String::as_str).collect(),
        Ballot::Ranking { ranking } => ranking.first().map(String::as_str).into_iter().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polls::config::new_options;

    fn options(kind: PollKind, open: bool, max: Option<usize>) -> PollConfig {
        let labels: Vec<String> = ["Pizza", "Sushi", "Kebab"]
            .iter()
            .map(|l| (*l).to_owned())
            .collect();
        PollConfig::Options(new_options(kind, &labels, open, max).unwrap())
    }

    fn input() -> BallotInput {
        BallotInput::default()
    }

    #[test]
    fn score_ballots() {
        let score = |value| {
            validate(
                PollKind::Score,
                &PollConfig::Score {},
                &BallotInput {
                    value: Some(value),
                    ..input()
                },
            )
        };
        assert_eq!(score(-5).unwrap(), Ballot::Score { value: -5 });
        assert_eq!(score(5).unwrap().to_json(), serde_json::json!({"value": 5}));
        assert_eq!(score(6).unwrap_err().code, "score_out_of_range");
        assert_eq!(
            validate(PollKind::Score, &PollConfig::Score {}, &input())
                .unwrap_err()
                .code,
            "score_required"
        );
        // Fields of other kinds are ignored.
        let extra = BallotInput {
            value: Some(1),
            choice: Some("o1".into()),
            ..input()
        };
        assert_eq!(
            validate(PollKind::Score, &PollConfig::Score {}, &extra).unwrap(),
            Ballot::Score { value: 1 }
        );
    }

    #[test]
    fn choice_ballots() {
        let config = options(PollKind::MultiChoice, false, Some(2));
        let multi = |ids: &[&str]| {
            validate(
                PollKind::MultiChoice,
                &config,
                &BallotInput {
                    choices: Some(ids.iter().map(|i| (*i).to_owned()).collect()),
                    ..input()
                },
            )
        };
        assert_eq!(
            multi(&["o3", "o1"]).unwrap(),
            Ballot::Choices {
                choices: vec!["o1".into(), "o3".into()]
            }
        );
        assert_eq!(multi(&["o1", "o1"]).unwrap_err().code, "invalid_choices");
        assert_eq!(multi(&[]).unwrap_err().code, "invalid_choices");
        assert_eq!(multi(&["o9"]).unwrap_err().code, "invalid_choices");
        assert_eq!(
            multi(&["o1", "o2", "o3"]).unwrap_err().code,
            "too_many_choices"
        );

        let ranked = options(PollKind::RankedChoice, false, None);
        let rank = |ids: &[&str]| {
            validate(
                PollKind::RankedChoice,
                &ranked,
                &BallotInput {
                    ranking: Some(ids.iter().map(|i| (*i).to_owned()).collect()),
                    ..input()
                },
            )
        };
        assert!(rank(&["o2"]).is_ok(), "partial rankings are fine");
        assert_eq!(rank(&["o2", "o2"]).unwrap_err().code, "invalid_ranking");
    }

    #[test]
    fn new_options_join_an_open_pool() {
        let config = options(PollKind::SingleChoice, true, None);
        let resolved = resolve(
            PollKind::SingleChoice,
            &config,
            &BallotInput {
                new_option: Some("  Tacos ".into()),
                ..input()
            },
        )
        .unwrap();
        assert_eq!(
            resolved.ballot,
            Ballot::Choice {
                choice: "o4".into()
            }
        );
        assert_eq!(
            resolved.added,
            Some(PollOption {
                id: "o4".into(),
                label: "Tacos".into()
            })
        );

        // An existing label (any case) just takes the vote.
        let existing = resolve(
            PollKind::SingleChoice,
            &config,
            &BallotInput {
                new_option: Some("sushi".into()),
                ..input()
            },
        )
        .unwrap();
        assert_eq!(
            (existing.ballot, existing.added),
            (
                Ballot::Choice {
                    choice: "o2".into()
                },
                None
            )
        );

        let both = BallotInput {
            choice: Some("o1".into()),
            new_option: Some("Tacos".into()),
            ..input()
        };
        assert_eq!(
            resolve(PollKind::SingleChoice, &config, &both)
                .unwrap_err()
                .code,
            "choice_and_new_option"
        );

        let multi = options(PollKind::MultiChoice, true, None);
        let added = resolve(
            PollKind::MultiChoice,
            &multi,
            &BallotInput {
                choices: Some(vec!["o2".into()]),
                new_option: Some("Tacos".into()),
                ..input()
            },
        )
        .unwrap();
        assert_eq!(
            added.ballot,
            Ballot::Choices {
                choices: vec!["o2".into(), "o4".into()]
            }
        );
    }

    #[test]
    fn closed_pools_and_ranked_polls_take_no_new_options() {
        let closed = options(PollKind::SingleChoice, false, None);
        let new = BallotInput {
            new_option: Some("Tacos".into()),
            ..input()
        };
        assert_eq!(
            resolve(PollKind::SingleChoice, &closed, &new)
                .unwrap_err()
                .code,
            "options_closed"
        );
        assert_eq!(
            resolve(
                PollKind::RankedChoice,
                &options(PollKind::RankedChoice, false, None),
                &new
            )
            .unwrap_err()
            .code,
            "options_closed"
        );
        assert_eq!(
            resolve(
                PollKind::SingleChoice,
                &options(PollKind::SingleChoice, true, None),
                &BallotInput {
                    new_option: Some(" ".into()),
                    ..input()
                }
            )
            .unwrap_err()
            .code,
            "invalid_option"
        );
    }

    #[test]
    fn the_pool_stops_at_twenty() {
        let mut config = options(PollKind::SingleChoice, true, None);
        if let PollConfig::Options(o) = &mut config {
            for n in 4..=20 {
                o.options.push(PollOption {
                    id: format!("o{n}"),
                    label: format!("Opcja {n}"),
                });
            }
        }
        let new = BallotInput {
            new_option: Some("Jeszcze jedna".into()),
            ..input()
        };
        assert_eq!(
            resolve(PollKind::SingleChoice, &config, &new)
                .unwrap_err()
                .code,
            "option_limit"
        );
    }
}
