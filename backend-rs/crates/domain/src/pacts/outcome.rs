//! How a pact (or one wager of a bet) ended, as somebody claims it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::kind::PactKind;
use super::terms::PactConfig;
use crate::DomainError;

/// The host won a wager (`bet`).
pub const HOST: &str = "host";
/// The opponent won a wager (`bet`).
pub const OPPONENT: &str = "opponent";
/// Nobody won.
pub const DRAW: &str = "draw";

/// A claimed result, stored as the JSON object it serialises to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum Outcome {
    /// Call it off: nobody wins, nothing is owed. Always `true`.
    Void { void: bool },
    /// `bet`: `host`, `opponent` or `draw`; `group_bet`: a side or `draw`.
    Winner { winner: String },
    /// `resolution`: whether the author kept their word.
    Kept { kept: bool },
    /// `shared_goal`: whether the goal was reached.
    Done { done: bool },
}

impl Outcome {
    pub const VOID: Self = Self::Void { void: true };

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void { .. })
    }

    /// Whether this claim may be made before the deadline (calling it off always may).
    pub fn early_ok(&self, kind: PactKind) -> bool {
        match self {
            Self::Void { .. } => true,
            Self::Done { done } => *done && kind.rules().early_claims,
            Self::Winner { .. } | Self::Kept { .. } => false,
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!(self)
    }

    /// Read a stored claim.
    pub fn from_json(value: &Value) -> Result<Self, serde_json::Error> {
        Self::deserialize(value)
    }

    /// A claim as a client sends it, for a pact of `kind`. `{"void": true}` calls it off
    /// (always possible); anything else must be a result of that kind.
    pub fn parse(kind: PactKind, config: &PactConfig, value: &Value) -> Result<Self, DomainError> {
        let Some(object) = value.as_object() else {
            return Err(
                DomainError::new("invalid_result", "Wynik musi być obiektem JSON.").on("result"),
            );
        };
        if object.get("void") == Some(&Value::Bool(true)) {
            return Ok(Self::VOID);
        }
        let invalid =
            |message: &'static str| DomainError::new("invalid_result", message).on("result");
        match kind {
            PactKind::Bet => match object.get("winner").and_then(Value::as_str) {
                Some(winner @ (HOST | OPPONENT | DRAW)) => Ok(Self::Winner {
                    winner: winner.to_owned(),
                }),
                _ => Err(invalid(
                    "Wynik musi wskazywać zwycięzcę: host, przeciwnik albo remis.",
                )),
            },
            PactKind::GroupBet => match object.get("winner").and_then(Value::as_str) {
                Some(winner) if winner == DRAW || config.sides.iter().any(|s| s == winner) => {
                    Ok(Self::Winner {
                        winner: winner.to_owned(),
                    })
                }
                _ => Err(invalid("Wynik musi wskazywać jedną ze stron albo remis.")),
            },
            PactKind::Resolution => match object.get("kept") {
                Some(Value::Bool(kept)) => Ok(Self::Kept { kept: *kept }),
                _ => Err(invalid(
                    "Wynik musi mówić, czy postanowienie dotrzymano (kept: true/false).",
                )),
            },
            PactKind::SharedGoal => match object.get("done") {
                Some(Value::Bool(done)) => Ok(Self::Done { done: *done }),
                _ => Err(invalid(
                    "Wynik musi mówić, czy cel osiągnięto (done: true/false).",
                )),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::pacts::terms::config;

    #[test]
    fn claims_are_checked_against_the_kind() {
        let none = PactConfig::default();
        assert_eq!(
            Outcome::parse(PactKind::Bet, &none, &json!({"winner": "host"})).unwrap(),
            Outcome::Winner {
                winner: "host".into()
            }
        );
        assert!(Outcome::parse(PactKind::Bet, &none, &json!({"winner": "nobody"})).is_err());
        let sides = config(PactKind::GroupBet, None).unwrap();
        assert!(Outcome::parse(PactKind::GroupBet, &sides, &json!({"winner": "nie"})).is_ok());
        assert!(Outcome::parse(PactKind::GroupBet, &sides, &json!({"winner": "draw"})).is_ok());
        assert!(Outcome::parse(PactKind::GroupBet, &sides, &json!({"winner": "host"})).is_err());
        assert!(Outcome::parse(PactKind::Resolution, &none, &json!({"kept": "yes"})).is_err());
        assert!(Outcome::parse(PactKind::SharedGoal, &none, &json!({"done": false})).is_ok());
        for bad in [json!([1]), json!("void"), json!(null)] {
            assert_eq!(
                Outcome::parse(PactKind::Bet, &none, &bad).unwrap_err().code,
                "invalid_result"
            );
        }
    }

    #[test]
    fn calling_it_off_is_always_a_valid_claim() {
        let none = PactConfig::default();
        for kind in PactKind::ALL {
            let claim = Outcome::parse(*kind, &none, &json!({"void": true, "winner": "x"}));
            assert_eq!(claim.unwrap(), Outcome::VOID);
        }
        assert!(Outcome::parse(PactKind::Bet, &none, &json!({"void": false})).is_err());
    }

    #[test]
    fn only_a_reached_goal_may_be_claimed_early() {
        assert!(Outcome::Done { done: true }.early_ok(PactKind::SharedGoal));
        assert!(!Outcome::Done { done: false }.early_ok(PactKind::SharedGoal));
        assert!(Outcome::VOID.early_ok(PactKind::Bet));
        assert!(
            !Outcome::Winner {
                winner: HOST.into()
            }
            .early_ok(PactKind::Bet)
        );
    }

    #[test]
    fn claims_are_stored_as_the_json_clients_send() {
        for (outcome, stored) in [
            (Outcome::VOID, json!({"void": true})),
            (
                Outcome::Winner {
                    winner: "nie".into(),
                },
                json!({"winner": "nie"}),
            ),
            (Outcome::Kept { kept: false }, json!({"kept": false})),
            (Outcome::Done { done: true }, json!({"done": true})),
        ] {
            assert_eq!(outcome.to_json(), stored);
            assert_eq!(Outcome::from_json(&stored).unwrap(), outcome);
        }
    }
}
