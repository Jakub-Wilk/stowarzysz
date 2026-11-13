//! What a poll is about, per kind. Stored as JSON in the same shapes the Django backend used.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::PollKind;
use crate::DomainError;

pub const MIN_OPTIONS: usize = 2;
/// When the poll is created.
pub const MAX_OPTIONS: usize = 10;
/// Once participants have added options to an open pool.
pub const MAX_OPTIONS_OPEN: usize = 20;
pub const LABEL_MAX: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollOption {
    pub id: String,
    pub label: String,
}

/// The options of a single, multi or ranked choice poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionsConfig {
    pub options: Vec<PollOption>,
    /// Participants may add an option with their vote (single and multi only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub open_options: bool,
    /// Multi choice: at most this many picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_choices: Option<usize>,
}

impl OptionsConfig {
    pub fn ids(&self) -> Vec<&str> {
        self.options.iter().map(|o| o.id.as_str()).collect()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.options.iter().any(|o| o.id == id)
    }

    /// The option labelled `label` (case-insensitively), if any.
    pub fn find_label(&self, label: &str) -> Option<&PollOption> {
        let wanted = label.to_lowercase();
        self.options
            .iter()
            .find(|o| o.label.to_lowercase() == wanted)
    }

    /// The id the next added option gets (`o<largest number + 1>`).
    pub fn next_id(&self) -> String {
        let largest = self
            .options
            .iter()
            .filter_map(|o| o.id.strip_prefix('o')?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("o{}", largest + 1)
    }
}

/// A poll's configuration.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PollConfig {
    Score {},
    Nickname {
        target_user_id: i64,
        target_username: String,
        new_username: String,
    },
    Avatar {
        target_user_id: i64,
        target_username: String,
        remove: bool,
    },
    PactRuling {
        proposal_id: i64,
        pact_id: i64,
        pact_title: String,
        claim: Value,
    },
    ResolutionJudgment {
        pact_id: i64,
        pact_title: String,
        condition: String,
        author_id: i64,
        author_username: String,
    },
    Options(OptionsConfig),
}

impl PollConfig {
    /// Read the stored JSON of a poll of `kind`. Unknown extra keys (e.g. the Django backend's
    /// `tie_break_of`, now a column) are ignored.
    pub fn from_json(kind: PollKind, value: &Value) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct Nickname {
            target_user_id: i64,
            target_username: String,
            new_username: String,
        }
        #[derive(Deserialize)]
        struct Avatar {
            target_user_id: i64,
            target_username: String,
            remove: bool,
        }
        #[derive(Deserialize)]
        struct Ruling {
            proposal_id: i64,
            pact_id: i64,
            pact_title: String,
            claim: Value,
        }
        #[derive(Deserialize)]
        struct Judgment {
            pact_id: i64,
            pact_title: String,
            condition: String,
            author_id: i64,
            author_username: String,
        }
        let value = value.clone();
        Ok(match kind {
            PollKind::Score => Self::Score {},
            PollKind::Nickname => {
                let c: Nickname = serde_json::from_value(value)?;
                Self::Nickname {
                    target_user_id: c.target_user_id,
                    target_username: c.target_username,
                    new_username: c.new_username,
                }
            }
            PollKind::Avatar => {
                let c: Avatar = serde_json::from_value(value)?;
                Self::Avatar {
                    target_user_id: c.target_user_id,
                    target_username: c.target_username,
                    remove: c.remove,
                }
            }
            PollKind::PactRuling => {
                let c: Ruling = serde_json::from_value(value)?;
                Self::PactRuling {
                    proposal_id: c.proposal_id,
                    pact_id: c.pact_id,
                    pact_title: c.pact_title,
                    claim: c.claim,
                }
            }
            PollKind::ResolutionJudgment => {
                let c: Judgment = serde_json::from_value(value)?;
                Self::ResolutionJudgment {
                    pact_id: c.pact_id,
                    pact_title: c.pact_title,
                    condition: c.condition,
                    author_id: c.author_id,
                    author_username: c.author_username,
                }
            }
            PollKind::SingleChoice | PollKind::MultiChoice | PollKind::RankedChoice => {
                Self::Options(serde_json::from_value(value)?)
            }
        })
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn options(&self) -> Option<&OptionsConfig> {
        match self {
            Self::Options(options) => Some(options),
            _ => None,
        }
    }

    /// The title that follows from the config, for kinds that do not take one.
    pub fn generated_title(&self) -> Option<String> {
        match self {
            Self::Nickname {
                target_username,
                new_username,
                ..
            } => Some(format!("Zmiana nicku: {target_username} → {new_username}")),
            Self::Avatar {
                target_username,
                remove,
                ..
            } => {
                let action = if *remove {
                    "Usunięcie zdjęcia"
                } else {
                    "Zmiana zdjęcia"
                };
                Some(format!("{action}: {target_username}"))
            }
            Self::PactRuling { pact_title, .. } => Some(format!("Spór o umowę: {pact_title}")),
            Self::ResolutionJudgment { pact_title, .. } => {
                Some(format!("Ocena postanowienia: {pact_title}"))
            }
            Self::Score {} | Self::Options(_) => None,
        }
    }
}

/// One option label: whitespace collapsed, 1-60 characters.
pub fn clean_label(raw: &str, field: &'static str) -> Result<String, DomainError> {
    let label = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let length = label.chars().count();
    if length == 0 || length > LABEL_MAX {
        return Err(DomainError::new(
            "invalid_option",
            format!("Opcja musi mieć od 1 do {LABEL_MAX} znaków."),
        )
        .on(field));
    }
    Ok(label)
}

/// The options of a new poll: 2-10 distinct labels, ids `o1`, `o2`, ...
pub fn new_options(
    kind: PollKind,
    labels: &[String],
    open_options: bool,
    max_choices: Option<usize>,
) -> Result<OptionsConfig, DomainError> {
    if open_options && kind == PollKind::RankedChoice {
        return Err(DomainError::new(
            "open_options_not_allowed",
            "Ten rodzaj głosowania nie pozwala uczestnikom dodawać opcji.",
        )
        .on("open_options"));
    }
    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&labels.len()) {
        return Err(DomainError::new(
            "option_count",
            format!("Podaj od {MIN_OPTIONS} do {MAX_OPTIONS} opcji."),
        )
        .on("options"));
    }
    let labels = labels
        .iter()
        .map(|l| clean_label(l, "options"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen: Vec<String> = labels.iter().map(|l| l.to_lowercase()).collect();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != labels.len() {
        return Err(DomainError::new("duplicate_options", "Opcje muszą się różnić.").on("options"));
    }
    if let Some(limit) = max_choices {
        let most = labels.len() - 1;
        if kind != PollKind::MultiChoice || !(1..=most).contains(&limit) {
            return Err(DomainError::new(
                "invalid_max_choices",
                format!("Limit wyborów musi być liczbą od 1 do {most}."),
            )
            .on("max_choices"));
        }
    }
    let options = labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| PollOption {
            id: format!("o{}", i + 1),
            label,
        })
        .collect();
    Ok(OptionsConfig {
        options,
        open_options,
        max_choices,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn labels(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn assigns_ids_and_cleans_labels() {
        let config = new_options(
            PollKind::SingleChoice,
            &labels(&["  Pizza ", "Sushi\t bar"]),
            true,
            None,
        )
        .unwrap();
        assert_eq!(
            config.clone().options[1],
            PollOption {
                id: "o2".into(),
                label: "Sushi bar".into()
            }
        );
        assert_eq!(
            PollConfig::Options(config).to_json(),
            json!({"options": [{"id": "o1", "label": "Pizza"}, {"id": "o2", "label": "Sushi bar"}], "open_options": true})
        );
    }

    #[test]
    fn rejects_bad_option_sets() {
        let code = |kind, values: &[&str], open, max| {
            new_options(kind, &labels(values), open, max)
                .unwrap_err()
                .code
        };
        assert_eq!(
            code(PollKind::SingleChoice, &["a"], false, None),
            "option_count"
        );
        assert_eq!(
            code(PollKind::SingleChoice, &["a"; 11], false, None),
            "option_count"
        );
        assert_eq!(
            code(PollKind::SingleChoice, &["Tak", "tak"], false, None),
            "duplicate_options"
        );
        assert_eq!(
            code(PollKind::SingleChoice, &["a", " "], false, None),
            "invalid_option"
        );
        assert_eq!(
            code(PollKind::SingleChoice, &["a", &"x".repeat(61)], false, None),
            "invalid_option"
        );
        assert_eq!(
            code(PollKind::RankedChoice, &["a", "b"], true, None),
            "open_options_not_allowed"
        );
        assert_eq!(
            code(PollKind::MultiChoice, &["a", "b", "c"], false, Some(3)),
            "invalid_max_choices"
        );
        assert_eq!(
            code(PollKind::SingleChoice, &["a", "b", "c"], false, Some(1)),
            "invalid_max_choices"
        );
        assert!(
            new_options(
                PollKind::MultiChoice,
                &labels(&["a", "b", "c"]),
                false,
                Some(2)
            )
            .is_ok()
        );
    }

    #[test]
    fn reads_django_configs_and_ignores_retired_keys() {
        let stored = json!({"options": [{"id": "o1", "label": "A"}, {"id": "o3", "label": "B"}], "tie_break_of": 5});
        let config = PollConfig::from_json(PollKind::SingleChoice, &stored).unwrap();
        let options = config.options().unwrap();
        assert_eq!(options.next_id(), "o4");
        assert_eq!(options.find_label("b").unwrap().id, "o3");
        assert_eq!(
            config.to_json(),
            json!({"options": [{"id": "o1", "label": "A"}, {"id": "o3", "label": "B"}]})
        );
        let nickname =
            json!({"target_user_id": 2, "target_username": "ala", "new_username": "Ala"});
        let config = PollConfig::from_json(PollKind::Nickname, &nickname).unwrap();
        assert_eq!(config.to_json(), nickname);
        assert_eq!(config.generated_title().unwrap(), "Zmiana nicku: ala → Ala");
        assert_eq!(
            PollConfig::from_json(PollKind::Score, &json!({}))
                .unwrap()
                .to_json(),
            json!({})
        );
    }
}
