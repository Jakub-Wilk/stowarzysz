//! Sejmik: creating polls, voting, closing, deadlines. Every change runs in one transaction
//! with the poll row locked; events and pushes are part of that transaction.

mod close;
pub mod deadlines;
mod services;
mod subjects;
mod views;

use chrono::{DateTime, Utc};
use domain::polls::{PollConfig, PollFacts, PollKind};
use serde_json::Value;

use crate::db::Uow;
use crate::{AppError, AppResult};

pub use services::{NewPoll, abstain, cast_ballot, close_early, create, react, tie_break, veto};
pub use views::{
    ListFilter, MyState, ParticipantView, PollActions, PollDetail, PollPage, PollSummary, Revealed,
    detail, list,
};

/// A poll as stored.
#[derive(Debug, Clone)]
pub(crate) struct PollRow {
    pub id: i64,
    pub creator_id: i64,
    pub title: String,
    pub kind: String,
    pub config: Value,
    pub status: String,
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub reminders_sent: i16,
    pub proposed_image_key: Option<String>,
}

impl PollRow {
    pub fn kind(&self) -> AppResult<PollKind> {
        PollKind::parse(&self.kind).ok_or_else(|| {
            anyhow::anyhow!("poll {} has unknown kind {}", self.id, self.kind).into()
        })
    }

    pub fn config(&self) -> AppResult<PollConfig> {
        Ok(PollConfig::from_json(self.kind()?, &self.config).map_err(anyhow::Error::from)?)
    }

    pub fn is_open(&self) -> bool {
        self.status == "open"
    }
}

/// Lock poll `id` for the rest of `uow` (404 if it does not exist).
pub(crate) async fn lock(uow: &mut Uow, id: i64) -> AppResult<PollRow> {
    sqlx::query_as!(
        PollRow,
        "select id, creator_id, title, kind, config, status, result, created_at, reminders_sent,
                proposed_image_key
         from polls where id = $1 for update",
        id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)
}

/// The caller's part in a poll.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Mine {
    pub participating: bool,
    pub vetoed: bool,
}

pub(crate) async fn mine(uow: &mut Uow, poll_id: i64, user_id: i64) -> AppResult<Mine> {
    let row = sqlx::query!(
        "select vetoed from poll_participants where poll_id = $1 and user_id = $2",
        poll_id,
        user_id
    )
    .fetch_optional(uow.conn())
    .await?;
    Ok(row.map_or_else(Mine::default, |row| Mine {
        participating: true,
        vetoed: row.vetoed,
    }))
}

/// The policy's view of `poll` for `user_id`.
pub(crate) fn facts(
    poll: &PollRow,
    user_id: i64,
    mine: Mine,
    has_tie_break: bool,
) -> AppResult<PollFacts> {
    let kind = poll.kind()?;
    Ok(PollFacts {
        kind,
        open: poll.is_open(),
        is_creator: poll.creator_id == user_id,
        participating: mine.participating,
        vetoed: mine.vetoed,
        tied: !domain::polls::tied_winners(kind, poll.result.as_ref()).is_empty(),
        has_tie_break,
    })
}

/// Map a policy refusal to the API error.
pub(crate) fn denied(denial: domain::polls::Denial) -> AppError {
    match denial {
        domain::polls::Denial::Closed => {
            AppError::conflict("poll_closed", "To głosowanie już się zakończyło.")
        }
        domain::polls::Denial::Forbidden { code, message } => AppError::forbidden(code, message),
        domain::polls::Denial::Invalid { code, message } => {
            domain::DomainError::new(code, message).into()
        }
    }
}
