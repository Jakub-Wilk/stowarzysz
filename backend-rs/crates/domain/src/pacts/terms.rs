//! What one person puts into a pact (a stake, a side), and a group bet's sides.

use serde::{Deserialize, Serialize};

use super::kind::PactKind;
use super::outcome::DRAW;
use crate::DomainError;
use crate::ledger::money::MAX_AMOUNT;

pub const STAKE_NOTE_MAX: usize = 200;
pub const SIDE_MAX: usize = 32;
const SIDES_MIN: usize = 2;
const SIDES_MAX: usize = 6;

/// A stake or side as a request sends it. Empty and absent mean the same.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct TermsInput {
    /// In grosze; `0` or `null` for none.
    pub stake_amount: Option<i64>,
    /// A stake in kind, e.g. "kolacja" (up to 200 characters).
    pub stake_note: Option<String>,
    /// Group bets: one of the pact's sides.
    pub side: Option<String>,
}

/// One person's cleaned terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Terms {
    /// Grosze; never zero.
    pub stake_amount: Option<i64>,
    pub stake_note: String,
    pub side: String,
}

/// Kind-specific settings, stored as JSON. Only group bets have any: their sides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PactConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sides: Vec<String>,
}

impl PactConfig {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        Self::deserialize(value)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!(self)
    }
}

/// The config of a new pact of `kind`; a group bet takes its sides (default: tak / nie).
pub fn config(kind: PactKind, sides: Option<&[String]>) -> Result<PactConfig, DomainError> {
    if kind != PactKind::GroupBet {
        if sides.is_some() {
            return Err(DomainError::new(
                "sides_not_allowed",
                "Strony mają tylko zakłady grupowe.",
            )
            .on("sides"));
        }
        return Ok(PactConfig::default());
    }
    let sides: Vec<String> = match sides {
        None => vec!["tak".into(), "nie".into()],
        Some(raw) => raw.iter().map(|s| s.trim().to_owned()).collect(),
    };
    if !(SIDES_MIN..=SIDES_MAX).contains(&sides.len()) {
        return Err(DomainError::new("sides_count", "Podaj od 2 do 6 stron.").on("sides"));
    }
    let mut seen: Vec<String> = Vec::with_capacity(sides.len());
    for side in &sides {
        let folded = side.to_lowercase();
        if side.is_empty() || side.chars().count() > SIDE_MAX || seen.contains(&folded) {
            return Err(DomainError::new(
                "invalid_sides",
                "Strony muszą być niepuste, różne i krótsze niż 32 znaki.",
            )
            .on("sides"));
        }
        if folded == DRAW {
            // "draw" is how a result says nobody was right.
            return Err(
                DomainError::new("reserved_side", "Strona nie może się nazywać „draw”.")
                    .on("sides"),
            );
        }
        seen.push(folded);
    }
    Ok(PactConfig { sides })
}

impl TermsInput {
    /// Trimmed, with a zero stake meaning none.
    pub fn clean(&self) -> Result<Terms, DomainError> {
        let stake_amount = match self.stake_amount {
            None | Some(0) => None,
            Some(amount) if amount < 0 => {
                return Err(
                    DomainError::new("invalid_stake", "Stawka musi być dodatnia.")
                        .on("stake_amount"),
                );
            }
            Some(amount) if amount > MAX_AMOUNT => {
                return Err(
                    DomainError::new("amount_too_large", "Ta kwota jest za duża.")
                        .on("stake_amount"),
                );
            }
            Some(amount) => Some(amount),
        };
        let stake_note = self.stake_note.as_deref().unwrap_or_default().trim();
        if stake_note.chars().count() > STAKE_NOTE_MAX {
            return Err(DomainError::new(
                "stake_note_too_long",
                format!("Opis stawki może mieć najwyżej {STAKE_NOTE_MAX} znaków."),
            )
            .on("stake_note"));
        }
        Ok(Terms {
            stake_amount,
            stake_note: stake_note.to_owned(),
            side: self.side.as_deref().unwrap_or_default().trim().to_owned(),
        })
    }

    /// An invitee's answer on top of what the creator pre-filled: what they send replaces it,
    /// what they leave out is kept.
    pub fn over(&self, prefilled: &Terms) -> Result<Terms, DomainError> {
        let given = self.clean()?;
        Ok(Terms {
            stake_amount: if self.stake_amount.is_some() {
                given.stake_amount
            } else {
                prefilled.stake_amount
            },
            stake_note: if self.stake_note.is_some() {
                given.stake_note
            } else {
                prefilled.stake_note.clone()
            },
            side: if given.side.is_empty() {
                prefilled.side.clone()
            } else {
                given.side
            },
        })
    }
}

/// Check `terms` for a pact of `kind`. `host`: the creator's own terms. `final`: the person is
/// committing (accepting, asking to join), as opposed to the creator pre-filling an invitee's
/// terms, which may leave things open.
pub fn validate(
    kind: PactKind,
    terms: Terms,
    config: &PactConfig,
    host: bool,
    final_: bool,
) -> Result<Terms, DomainError> {
    match kind {
        PactKind::Bet => {
            if host {
                // The host matches each opponent's stake, so has none of their own.
                return Ok(Terms::default());
            }
            if terms.stake_amount.is_none() && terms.stake_note.is_empty() {
                return Err(DomainError::new(
                    "stake_required",
                    "Podaj stawkę: kwotę albo opis stawki.",
                )
                .on("stake_amount"));
            }
            Ok(Terms {
                side: String::new(),
                ..terms
            })
        }
        PactKind::GroupBet => {
            if (host || final_ || !terms.side.is_empty()) && !config.sides.contains(&terms.side) {
                return Err(
                    DomainError::new("invalid_side", "Wybierz jedną ze stron zakładu.").on("side"),
                );
            }
            Ok(terms)
        }
        PactKind::Resolution | PactKind::SharedGoal => {
            if terms.stake_amount.is_some() || !terms.side.is_empty() {
                let message = if kind == PactKind::Resolution {
                    "Postanowienia nie mają stawki pieniężnej ani stron."
                } else {
                    "Wspólne cele nie mają stawki pieniężnej ani stron."
                };
                return Err(DomainError::new("no_stakes", message).on("stake_amount"));
            }
            Ok(terms)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(amount: Option<i64>, note: &str, side: &str) -> TermsInput {
        TermsInput {
            stake_amount: amount,
            stake_note: Some(note.into()),
            side: Some(side.into()),
        }
    }

    fn sides() -> PactConfig {
        config(PactKind::GroupBet, None).unwrap()
    }

    #[test]
    fn a_group_bet_has_two_to_six_distinct_sides_and_none_called_draw() {
        assert_eq!(sides().sides, ["tak", "nie"]);
        let raw = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            config(PactKind::GroupBet, Some(&raw(&[" A ", "B"])))
                .unwrap()
                .sides,
            ["A", "B"]
        );
        for bad in [
            raw(&["tak"]),
            raw(&["a", "a"]),
            raw(&["a", "A"]),
            raw(&["a", ""]),
            raw(&["a", "b", "c", "d", "e", "f", "g"]),
            raw(&["a", &"x".repeat(33)]),
            raw(&["a", "Draw"]),
        ] {
            assert!(config(PactKind::GroupBet, Some(&bad)).is_err(), "{bad:?}");
        }
        assert_eq!(
            config(PactKind::Bet, Some(&raw(&["a", "b"])))
                .unwrap_err()
                .code,
            "sides_not_allowed"
        );
    }

    #[test]
    fn cleaning_trims_and_treats_zero_as_no_stake() {
        let terms = input(Some(0), "  kolacja ", " tak ").clean().unwrap();
        assert_eq!(
            terms,
            Terms {
                stake_amount: None,
                stake_note: "kolacja".into(),
                side: "tak".into()
            }
        );
        assert_eq!(
            input(Some(-1), "", "").clean().unwrap_err().code,
            "invalid_stake"
        );
        assert_eq!(
            input(Some(MAX_AMOUNT + 1), "", "")
                .clean()
                .unwrap_err()
                .code,
            "amount_too_large"
        );
        assert_eq!(
            input(None, &"x".repeat(201), "").clean().unwrap_err().code,
            "stake_note_too_long"
        );
    }

    #[test]
    fn an_answer_keeps_what_it_leaves_out() {
        let prefilled = Terms {
            stake_amount: Some(500),
            stake_note: "piwo".into(),
            side: "nie".into(),
        };
        let nothing = TermsInput::default();
        assert_eq!(nothing.over(&prefilled).unwrap(), prefilled);
        let zero = TermsInput {
            stake_amount: Some(0),
            side: Some("tak".into()),
            ..TermsInput::default()
        };
        let merged = zero.over(&prefilled).unwrap();
        assert_eq!(
            (
                merged.stake_amount,
                merged.side.as_str(),
                merged.stake_note.as_str()
            ),
            (None, "tak", "piwo")
        );
    }

    #[test]
    fn a_bet_needs_an_opponent_stake_and_ignores_the_hosts() {
        let host = validate(
            PactKind::Bet,
            input(Some(100), "x", "tak").clean().unwrap(),
            &PactConfig::default(),
            true,
            true,
        )
        .unwrap();
        assert_eq!(host, Terms::default());
        let missing = validate(
            PactKind::Bet,
            Terms::default(),
            &PactConfig::default(),
            false,
            false,
        );
        assert_eq!(missing.unwrap_err().code, "stake_required");
        let note = validate(
            PactKind::Bet,
            input(None, " kolacja ", "").clean().unwrap(),
            &PactConfig::default(),
            false,
            false,
        )
        .unwrap();
        assert_eq!(note.stake_note, "kolacja");
    }

    #[test]
    fn a_group_bet_side_is_required_once_someone_commits() {
        let config = sides();
        let open = validate(PactKind::GroupBet, Terms::default(), &config, false, false);
        assert_eq!(open.unwrap(), Terms::default());
        let no_side = Terms {
            stake_amount: Some(100),
            ..Terms::default()
        };
        for (terms, host, final_) in [
            (no_side.clone(), false, true),
            (no_side, true, false),
            (
                Terms {
                    side: "maybe".into(),
                    ..Terms::default()
                },
                false,
                false,
            ),
        ] {
            assert_eq!(
                validate(PactKind::GroupBet, terms, &config, host, final_)
                    .unwrap_err()
                    .code,
                "invalid_side"
            );
        }
        let ok = Terms {
            side: "tak".into(),
            ..Terms::default()
        };
        assert_eq!(
            validate(PactKind::GroupBet, ok, &config, false, true)
                .unwrap()
                .side,
            "tak"
        );
    }

    #[test]
    fn resolutions_and_goals_take_no_money_or_sides() {
        for kind in [PactKind::Resolution, PactKind::SharedGoal] {
            let money = Terms {
                stake_amount: Some(100),
                ..Terms::default()
            };
            assert_eq!(
                validate(kind, money, &PactConfig::default(), true, true)
                    .unwrap_err()
                    .code,
                "no_stakes"
            );
        }
    }
}
