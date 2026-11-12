use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::media::Image;

/// A member as other members see them.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Person {
    pub id: i64,
    pub username: String,
    pub avatar: Option<Image>,
}

/// The signed-in user.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Me {
    pub id: i64,
    pub username: String,
    pub is_superuser: bool,
    pub avatar: Option<Image>,
    pub birthday: Option<NaiveDate>,
}

/// What the (public) login screen may know about an account.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct LoginUser {
    pub username: String,
    pub avatar: Option<Image>,
}

/// An account as administrators see it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ManagedUser {
    pub id: i64,
    pub username: String,
    pub is_active: bool,
    pub is_superuser: bool,
    /// False until the person has set a password through an activation link.
    pub has_password: bool,
    pub avatar: Option<Image>,
    pub birthday: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub voting: VotingStats,
    pub pacts: PactStats,
}

/// Score votes in closed polls only (open ones would reveal hidden ballots).
#[derive(Debug, Clone, PartialEq, Default, Serialize, ToSchema)]
pub struct VotingStats {
    pub votes_cast: i64,
    /// Average score given, rounded to 2 decimals; `null` without votes.
    pub average_score: Option<f64>,
    pub veto_count: i64,
    /// Share of the votes cast that were vetoes, in percent (1 decimal).
    pub veto_percent: f64,
}

/// A member's record across settled pacts. Money is PLN grosze.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
pub struct PactStats {
    pub won: i64,
    pub lost: i64,
    pub draw: i64,
    pub money_won: i64,
    pub money_lost: i64,
    /// Per pact kind (`bet`, `group_bet`, ...).
    pub by_kind: BTreeMap<String, KindStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
pub struct KindStats {
    pub won: i64,
    pub lost: i64,
    pub draw: i64,
}
