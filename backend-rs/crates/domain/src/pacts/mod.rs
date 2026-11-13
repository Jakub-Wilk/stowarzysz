//! Umowy: bets, group bets, resolutions and shared goals between members.
//!
//! A pact is made of participants: its creator (the host) plus everyone invited or who asked to
//! join. What a participant row means depends on the [`PactKind`]: in a `bet` each opponent is
//! a separate wager against the host, in the other kinds everyone shares one outcome. Someone
//! claims how it ended ([`Outcome`]), the other side confirms, and [`settle`] turns the
//! confirmed outcome into verdicts and debts (money never lives here: the ledger records them).
//!
//! [`Pact`] is the whole aggregate as stored; [`policy`] decides from it who may do what, both
//! for the services and for the `actions` block clients read.

pub mod kind;
pub mod outcome;
pub mod policy;
pub mod settle;
pub mod state;
pub mod terms;

pub use kind::{KindRules, PactKind};
pub use outcome::Outcome;
pub use policy::{Action, Actions, Denial};
pub use settle::{Party, Settlement, Verdict, settle};
pub use state::{
    Claim, ClaimState, Consent, Pact, PactOutcome, Participant, ParticipantState, Role, Ruling,
    Status, WagerOutcome,
};
pub use terms::{Terms, TermsInput};

use chrono::TimeDelta;

/// Unanswered invites and join requests lapse after this long.
pub const INVITE_TTL: TimeDelta = TimeDelta::days(7);
/// How often the parties of an overdue pact are reminded to settle it.
pub const NUDGE_EVERY: TimeDelta = TimeDelta::days(7);
/// Pictures per pact.
pub const MAX_ATTACHMENTS: usize = 10;
/// An owed item (a stake in kind, as the ledger records it) is at most this long.
pub const ITEM_LENGTH: usize = 60;
pub const TITLE_MAX: usize = 200;
pub const CONDITION_MAX: usize = 2000;
pub const NOTES_MAX: usize = 5000;
pub const CAPTION_MAX: usize = 200;
