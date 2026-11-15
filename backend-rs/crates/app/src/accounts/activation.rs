//! One-time links that let an administrator-created account set its own password.

use chrono::{DateTime, TimeDelta, Utc};
use domain::DomainError;
use domain::accounts::password;
use serde::Serialize;
use utoipa::ToSchema;

use crate::auth::tokens;
use crate::db::Uow;
use crate::{AppError, AppResult, Context};

/// An activation link, shown once to the administrator who made it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActivationLink {
    pub token: String,
    /// The frontend page that completes the activation.
    pub url: String,
    pub expires_at: DateTime<Utc>,
}

/// Every failure looks the same, so a token cannot be probed.
fn invalid() -> AppError {
    DomainError::new(
        "invalid_activation_token",
        "Ten link aktywacyjny jest nieprawidłowy lub wygasł.",
    )
    .on("token")
    .into()
}

/// Issue a link for `user_id`; any earlier unused link of theirs stops working.
pub async fn issue(ctx: &Context, issued_by: i64, user_id: i64) -> AppResult<ActivationLink> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let active = sqlx::query_scalar!(
        "select is_active from users where id = $1 for update",
        user_id
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    if !active {
        return Err(DomainError::new(
            "user_inactive",
            "Nie można wystawić linku dla nieaktywnego użytkownika.",
        )
        .into());
    }
    sqlx::query!(
        "delete from activation_tokens where user_id = $1 and used_at is null",
        user_id
    )
    .execute(uow.conn())
    .await?;
    let token = tokens::new_link_token();
    let now = ctx.clock.now();
    let ttl =
        TimeDelta::from_std(ctx.config.limits.activation_token_ttl).map_err(anyhow::Error::from)?;
    let expires_at = now + ttl;
    sqlx::query!(
        "insert into activation_tokens (user_id, token_hash, created_by, created_at, expires_at)
         values ($1, $2, $3, $4, $5)",
        user_id,
        tokens::hash_token(&token),
        issued_by,
        now,
        expires_at,
    )
    .execute(uow.conn())
    .await?;
    uow.commit().await?;
    let url = format!("{}/activate/{token}", ctx.config.frontend_url);
    Ok(ActivationLink {
        token,
        url,
        expires_at,
    })
}

/// The username a valid token activates.
pub async fn validate(ctx: &Context, token: &str) -> AppResult<String> {
    sqlx::query_scalar!(
        "select u.username from activation_tokens t join users u on u.id = t.user_id
         where t.token_hash = $1 and t.used_at is null and t.expires_at > $2 and u.is_active",
        tokens::hash_token(token),
        ctx.clock.now(),
    )
    .fetch_optional(&ctx.db)
    .await?
    .ok_or_else(invalid)
}

/// Set the account's password and use the token up. The person signs in afterwards.
pub async fn complete(ctx: &Context, token: &str, new_password: &str) -> AppResult<()> {
    // Checked and hashed before the transaction, so it is not held open while argon2 runs
    // (or waits for its turn).
    let username = validate(ctx, token).await?;
    let problems = password::validate(new_password, &username);
    if !problems.is_empty() {
        return Err(problems.into());
    }
    let hash = ctx.passwords.hash(new_password).await?;

    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    // Locked and checked again, so two concurrent submissions cannot both succeed.
    let target = sqlx::query!(
        "select t.id, t.user_id from activation_tokens t join users u on u.id = t.user_id
         where t.token_hash = $1 and t.used_at is null and t.expires_at > $2 and u.is_active
         for update of t",
        tokens::hash_token(token),
        now,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or_else(invalid)?;
    sqlx::query!(
        "update users set password_hash = $2 where id = $1",
        target.user_id,
        hash
    )
    .execute(uow.conn())
    .await?;
    sqlx::query!(
        "update activation_tokens set used_at = $2 where id = $1",
        target.id,
        now
    )
    .execute(uow.conn())
    .await?;
    uow.commit().await
}
