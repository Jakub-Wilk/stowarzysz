//! Secret Santa: superusers start a round (the draw is sealed at once), adjust and end it;
//! givers see only their own victims, may ask them anonymously for ideas, and once a round
//! ends its pairings become public history with a note per gift.
//!
//! While a round is active the pairing exists nowhere in plain text: only [`seal::Sealer`]
//! output in `santa_assignments.sealed`, opened for the giver alone (and to find whom to tell
//! when a victim answers a help request). Ending the round writes the receivers down.

pub mod seal;
mod services;
mod views;

use chrono::{DateTime, Utc};
use domain::santa::Mode;
use sqlx::PgConnection;

use crate::db::Uow;
use crate::{AppError, AppResult, Context};

pub use seal::Sealer;
pub use services::{
    AnswerRequest, GiftNote, HelpRequest, StartRequest, UpdateRequest, answer_help, ask_for_help,
    end, set_gift_note, start, update,
};
pub use views::{
    ActiveEvent, GiftView, GiverHelp, HistoryEvent, HistoryPage, PairingView, ReceiverHelp,
    SantaState, TierVictim, history, state,
};

/// The active round.
#[derive(Debug, Clone)]
pub(crate) struct Event {
    pub id: i64,
    pub mode: Mode,
    pub deadline: DateTime<Utc>,
    pub gift_tiers: Vec<i64>,
}

impl Event {
    /// The amount of tier `tier` (`None`: the giver covers every tier).
    pub fn amount(&self, tier: Option<i16>) -> Option<i64> {
        tier.and_then(|t| usize::try_from(t).ok())
            .and_then(|t| self.gift_tiers.get(t).copied())
    }
}

/// The active round, if any.
pub(crate) async fn active(conn: &mut PgConnection) -> AppResult<Option<Event>> {
    let row = sqlx::query!(
        "select id, mode, deadline, gift_tiers from santa_events where status = 'active'"
    )
    .fetch_optional(conn)
    .await?;
    row.map(|r| {
        Ok(Event {
            id: r.id,
            mode: Mode::parse(&r.mode)
                .ok_or_else(|| anyhow::anyhow!("santa event {} has mode {:?}", r.id, r.mode))?,
            deadline: r.deadline,
            gift_tiers: r.gift_tiers,
        })
    })
    .transpose()
}

/// The active round, locked for the rest of `uow` (404 if none).
pub(crate) async fn lock_active(uow: &mut Uow) -> AppResult<Event> {
    sqlx::query_scalar!("select id from santa_events where status = 'active' for update")
        .fetch_optional(uow.conn())
        .await?
        .ok_or(AppError::NotFound)?;
    active(uow.conn()).await?.ok_or(AppError::NotFound)
}

/// Whom `user_id` gives to in `event`: `(tier, receiver)`, by tier; empty if they are not in it.
pub(crate) async fn victims(
    conn: &mut PgConnection,
    ctx: &Context,
    event_id: i64,
    user_id: i64,
) -> AppResult<Vec<(Option<i16>, i64)>> {
    let rows = sqlx::query!(
        r#"select tier_index, sealed as "sealed!" from santa_assignments
           where event_id = $1 and giver_id = $2 and sealed is not null
           order by tier_index nulls first"#,
        event_id,
        user_id,
    )
    .fetch_all(conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            let receiver = ctx
                .santa
                .open(event_id, user_id, r.tier_index, &r.sealed)
                .map_err(anyhow::Error::from)?;
            Ok((r.tier_index, receiver))
        })
        .collect()
}
