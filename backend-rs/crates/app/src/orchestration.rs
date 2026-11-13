//! What a closed vote decides, handed to the context that owns it, in the closing transaction:
//! renaming a member or changing their picture (accounts), settling a disputed pact claim or
//! judging a resolution (pacts). The polls module only says that a poll closed; it never knows
//! what its subject means.
//!
//! Each step runs in a savepoint. A failure is recorded in the result (`applied: false`,
//! `apply_error`), never raised: the vote is over either way.

use domain::accounts::username;
use domain::polls::PollConfig;
use serde_json::{Map, Value, json};

use crate::db::Uow;
use crate::pacts::Applied;
use crate::polls::PollRow;
use crate::{AppError, AppResult, Context};

const GONE: &str = "Ten poseł już nie istnieje.";

/// Keys to add to the result of `poll`, or `None` if the kind decides nothing.
pub(crate) async fn on_close(
    uow: &mut Uow,
    ctx: &Context,
    poll: &PollRow,
    config: &PollConfig,
    result: &Value,
) -> AppResult<Option<Map<String, Value>>> {
    let flag = |key: &str| result.get(key).and_then(Value::as_bool) == Some(true);
    let approved = flag("approved");
    let applied = match config {
        PollConfig::Score {} | PollConfig::Options(_) => return Ok(None),
        PollConfig::Nickname { .. } | PollConfig::Avatar { .. } if !approved => Applied::No,
        PollConfig::Nickname {
            target_user_id,
            new_username,
            ..
        } => {
            uow.savepoint().await?;
            let step = rename(uow, *target_user_id, new_username.clone()).await;
            settle(uow, step, "Nie udało się zastosować zmiany.").await?
        }
        PollConfig::Avatar {
            target_user_id,
            remove,
            ..
        } => {
            let proposed = if *remove {
                None
            } else {
                poll.proposed_image_key.clone()
            };
            uow.savepoint().await?;
            let step = swap_avatar(uow, poll.id, *target_user_id, proposed).await;
            settle(uow, step, "Nie udało się zastosować zmiany.").await?
        }
        PollConfig::PactRuling { proposal_id, .. } => {
            uow.savepoint().await?;
            let step = crate::pacts::apply_ruling(uow, ctx, *proposal_id, approved).await;
            settle(uow, step, "Nie udało się zastosować rozstrzygnięcia.").await?
        }
        PollConfig::ResolutionJudgment { pact_id, .. } => {
            uow.savepoint().await?;
            let step =
                crate::pacts::apply_judgment(uow, ctx, *pact_id, approved, flag("decided")).await;
            settle(uow, step, "Nie udało się zastosować oceny.").await?
        }
    };
    let mut keys = Map::new();
    keys.insert("applied".into(), json!(applied == Applied::Yes));
    if let Applied::Refused(reason) = applied {
        keys.insert("apply_error".into(), json!(reason));
    }
    Ok(Some(keys))
}

/// End the savepoint around a step: kept if it ran, rolled back (and reported as a reason, so
/// the poll still closes) if it failed. A refusal the step raised as a conflict keeps its
/// message; anything else is logged and reported as `failure`.
async fn settle(uow: &mut Uow, step: AppResult<Applied>, failure: &str) -> AppResult<Applied> {
    match step {
        Ok(applied) => {
            uow.release_savepoint().await?;
            Ok(applied)
        }
        Err(AppError::Conflict { message, .. }) => {
            uow.rollback_to_savepoint().await?;
            Ok(Applied::Refused(message.into_owned()))
        }
        Err(error) => {
            tracing::error!(%error, "applying a vote failed");
            uow.rollback_to_savepoint().await?;
            Ok(Applied::Refused(failure.to_owned()))
        }
    }
}

/// The current picture of the member a profile vote is about, if they are still a member.
async fn member(uow: &mut Uow, user_id: i64) -> AppResult<Option<Option<String>>> {
    Ok(sqlx::query_scalar!(
        "select avatar_key from users where id = $1 and is_active and password_hash is not null for update",
        user_id,
    )
    .fetch_optional(uow.conn())
    .await?)
}

// Owned arguments: a borrowed `&str` bound into a query inside the scheduler's boxed future
// trips rustc's higher-ranked `Send` check.
async fn rename(uow: &mut Uow, user_id: i64, new_username: String) -> AppResult<Applied> {
    if member(uow, user_id).await?.is_none() {
        return Ok(Applied::Refused(GONE.into()));
    }
    if let Err(error) = username::validate(&new_username) {
        return Ok(Applied::Refused(error.message.into_owned()));
    }
    // Someone may have taken the name while the vote was running.
    // A runtime query with owned binds: the `query!` macro's type-check scaffolding keeps a
    // `&str` alive across the await, which fails the scheduler's higher-ranked `Send` check.
    let taken: bool =
        sqlx::query_scalar("select exists(select 1 from users where username = $1 and id <> $2)")
            .bind(new_username.clone())
            .bind(user_id)
            .fetch_one(uow.conn())
            .await?;
    if taken {
        return Ok(Applied::Refused(username::taken().message.into_owned()));
    }
    sqlx::query!(
        "update users set username = $2 where id = $1",
        user_id,
        new_username
    )
    .execute(uow.conn())
    .await?;
    Ok(Applied::Yes)
}

/// The member's picture becomes the proposed one (or none); the poll keeps the picture it
/// replaced. Keys are shared, not copied: the media sweeper never deletes a referenced key.
async fn swap_avatar(
    uow: &mut Uow,
    poll_id: i64,
    user_id: i64,
    proposed: Option<String>,
) -> AppResult<Applied> {
    let Some(current) = member(uow, user_id).await? else {
        return Ok(Applied::Refused(GONE.into()));
    };
    sqlx::query!(
        "update users set avatar_key = $2 where id = $1",
        user_id,
        proposed
    )
    .execute(uow.conn())
    .await?;
    sqlx::query!(
        "update polls set previous_image_key = $2 where id = $1",
        poll_id,
        current
    )
    .execute(uow.conn())
    .await?;
    Ok(Applied::Yes)
}
