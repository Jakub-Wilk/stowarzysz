//! What a closed vote decides, applied in the closing transaction: renaming a member, changing
//! their picture, or (with pacts) settling a dispute or judging a resolution. A failure is
//! recorded in the result (`applied: false`, `apply_error`), never raised: the vote is over
//! either way.

use domain::accounts::username;
use domain::polls::PollConfig;
use serde_json::{Map, Value, json};

use super::PollRow;
use crate::db::Uow;
use crate::{AppResult, Context};

const GONE: &str = "Ten poseł już nie istnieje.";

/// Keys to add to the result, or `None` if the kind decides nothing.
pub(crate) async fn on_close(
    uow: &mut Uow,
    ctx: &Context,
    poll: &PollRow,
    config: &PollConfig,
    result: &Value,
) -> AppResult<Option<Map<String, Value>>> {
    let approved = result.get("approved").and_then(Value::as_bool) == Some(true);
    // `Ok(None)`: applied; `Ok(Some(reason))`: approved but not applicable; `Err`: a failure.
    let error: Option<String> = match config {
        PollConfig::Score {} | PollConfig::Options(_) => return Ok(None),
        PollConfig::Nickname { .. } | PollConfig::Avatar { .. } if !approved => {
            return Ok(Some(keys(false, None)));
        }
        PollConfig::Nickname {
            target_user_id,
            new_username,
            ..
        } => {
            uow.savepoint().await?;
            let step = rename(uow, *target_user_id, new_username.clone()).await;
            settle(uow, step).await?
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
            settle(uow, step).await?
        }
        PollConfig::PactRuling { .. } | PollConfig::ResolutionJudgment { .. } => {
            // Settled by the pacts module once it exists (phase 5): these polls are only
            // created through pacts, so none can close before then.
            tracing::error!(
                poll = poll.id,
                "a pact poll closed before pacts are implemented"
            );
            let _ = ctx;
            Some("Rozstrzyganie umów nie jest jeszcze dostępne.".to_owned())
        }
    };
    Ok(Some(keys(error.is_none(), error)))
}

fn keys(applied: bool, error: Option<String>) -> Map<String, Value> {
    let mut keys = Map::new();
    keys.insert("applied".into(), json!(applied));
    if let Some(error) = error {
        keys.insert("apply_error".into(), json!(error));
    }
    keys
}

/// End the savepoint around a step: kept if it ran, rolled back (and reported as a reason, so
/// the poll still closes) if it failed unexpectedly.
async fn settle(uow: &mut Uow, step: AppResult<Option<String>>) -> AppResult<Option<String>> {
    match step {
        Ok(reason) => {
            uow.release_savepoint().await?;
            Ok(reason)
        }
        Err(error) => {
            tracing::error!(%error, "applying a vote failed");
            uow.rollback_to_savepoint().await?;
            Ok(Some("Nie udało się zastosować zmiany.".to_owned()))
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
async fn rename(uow: &mut Uow, user_id: i64, new_username: String) -> AppResult<Option<String>> {
    if member(uow, user_id).await?.is_none() {
        return Ok(Some(GONE.into()));
    }
    if let Err(error) = username::validate(&new_username) {
        return Ok(Some(error.message.into_owned()));
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
        return Ok(Some(username::taken().message.into_owned()));
    }
    sqlx::query!(
        "update users set username = $2 where id = $1",
        user_id,
        new_username
    )
    .execute(uow.conn())
    .await?;
    Ok(None)
}

/// The member's picture becomes the proposed one (or none); the poll keeps the picture it
/// replaced. Keys are shared, not copied: the media sweeper never deletes a referenced key.
async fn swap_avatar(
    uow: &mut Uow,
    poll_id: i64,
    user_id: i64,
    proposed: Option<String>,
) -> AppResult<Option<String>> {
    let Some(current) = member(uow, user_id).await? else {
        return Ok(Some(GONE.into()));
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
    Ok(None)
}
