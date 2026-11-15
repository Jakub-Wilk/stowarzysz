//! Signing in: password checks, sessions, access tokens.
//!
//! A session is one signed-in device. Its refresh token is opaque, stored hashed and long-lived
//! (sliding: 400 days since it was last used, no absolute cap; the app is used rarely). Every
//! refresh rotates it, but rotation is loss-tolerant: until the client has used the new token,
//! the previous one keeps working, so a phone that never stored the rotated token (an Android
//! PWA killed mid-request) is not logged out. Only logout, deactivation or an administrator end
//! a session. Access tokens are short JWTs naming the session, checked against it on every
//! request, so ending a session takes effect immediately.

pub mod password;
pub mod tokens;

use chrono::{DateTime, TimeDelta, Utc};
use domain::accounts::username;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::{AppError, AppResult, Context};
use password::Verified;

/// A session lives this long after it was last used.
pub const SESSION_TTL: TimeDelta = TimeDelta::days(400);
const USER_AGENT_MAX: usize = 255;

/// The signed-in caller of a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub user_id: i64,
    pub username: String,
    pub is_superuser: bool,
    pub session_id: Uuid,
}

/// What a client keeps after signing in.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TokenPair {
    /// Short-lived bearer token for `Authorization: Bearer ...`.
    pub access: String,
    /// Seconds until `access` expires.
    pub access_expires_in: i64,
    /// Long-lived token for `POST /api/auth/refresh`; rotates on every use.
    pub refresh: String,
}

pub fn not_authenticated() -> AppError {
    AppError::unauthorized(
        "not_authenticated",
        "Nie podano danych uwierzytelniających.",
    )
}

pub fn invalid_token() -> AppError {
    AppError::unauthorized("invalid_token", "Token jest nieprawidłowy lub wygasł.")
}

fn invalid_credentials() -> AppError {
    AppError::unauthorized(
        "invalid_credentials",
        "Nieprawidłowa nazwa użytkownika lub hasło.",
    )
}

fn session_expired() -> AppError {
    AppError::unauthorized("session_expired", "Sesja wygasła. Zaloguj się ponownie.")
}

/// Sign in with a username and password. Returns the tokens and the user id.
pub async fn login(
    ctx: &Context,
    raw_username: &str,
    password: &str,
    user_agent: &str,
) -> AppResult<(TokenPair, i64)> {
    let name = username::normalize(raw_username);
    let account = sqlx::query!(
        "select id, password_hash from users where username = $1 and is_active",
        name,
    )
    .fetch_optional(&ctx.db)
    .await?;
    let stored = account.as_ref().and_then(|a| a.password_hash.as_deref());
    let verified = ctx.passwords.verify(password, stored).await?;
    let user_id = match (&account, &verified) {
        (Some(account), Verified::Yes | Verified::YesRehash(_)) => account.id,
        _ => return Err(invalid_credentials()),
    };

    let mut uow = Uow::begin(&ctx.db).await?;
    if let Verified::YesRehash(new_hash) = verified {
        sqlx::query!(
            "update users set password_hash = $2 where id = $1",
            user_id,
            new_hash
        )
        .execute(uow.conn())
        .await?;
    }
    let tokens = open_session(&mut uow, ctx, user_id, user_agent).await?;
    uow.commit().await?;
    Ok((tokens, user_id))
}

/// Start a session for `user_id` (who must already be authenticated).
pub async fn open_session(
    uow: &mut Uow,
    ctx: &Context,
    user_id: i64,
    user_agent: &str,
) -> AppResult<TokenPair> {
    let now = ctx.clock.now();
    let session_id = Uuid::new_v4();
    let (refresh, hash) = tokens::new_refresh_token();
    sqlx::query!(
        "insert into auth_sessions (id, user_id, refresh_hash, user_agent, created_at, last_used_at, expires_at)
         values ($1, $2, $3, $4, $5, $5, $6)",
        session_id,
        user_id,
        hash,
        truncate(user_agent, USER_AGENT_MAX),
        now,
        now + SESSION_TTL,
    )
    .execute(uow.conn())
    .await?;
    pair(ctx, user_id, session_id, refresh, now)
}

fn pair(
    ctx: &Context,
    user_id: i64,
    session_id: Uuid,
    refresh: String,
    now: DateTime<Utc>,
) -> AppResult<TokenPair> {
    Ok(TokenPair {
        access: ctx.signer.issue(user_id, session_id, now)?,
        access_expires_in: tokens::ACCESS_TTL.num_seconds(),
        refresh,
    })
}

/// Exchange a refresh token for new tokens (see the module docs for the rotation rules).
pub async fn refresh(ctx: &Context, token: &str, user_agent: &str) -> AppResult<TokenPair> {
    if tokens::is_legacy_refresh(token) {
        return exchange_legacy(ctx, token, user_agent).await;
    }
    let now = ctx.clock.now();
    let hash = tokens::hash_token(token);
    let mut uow = Uow::begin(&ctx.db).await?;
    let session = sqlx::query!(
        r#"select s.id, s.user_id, s.refresh_hash = $1 as "is_current!"
           from auth_sessions s join users u on u.id = s.user_id
           where (s.refresh_hash = $1 or s.prev_refresh_hash = $1)
             and s.revoked_at is null and s.expires_at > $2
             and u.is_active and u.password_hash is not null
           for update of s"#,
        hash,
        now,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or_else(session_expired)?;

    let (refresh, new_hash) = tokens::new_refresh_token();
    // Presenting the current token proves the client stored it: the one before it retires.
    // Presenting the previous one means the last rotation never reached the client: issue
    // another replacement and keep accepting the previous token.
    sqlx::query!(
        "update auth_sessions
         set prev_refresh_hash = case when $2 then refresh_hash else prev_refresh_hash end,
             refresh_hash = $3, last_used_at = $4, expires_at = $5, user_agent = $6
         where id = $1",
        session.id,
        session.is_current,
        new_hash,
        now,
        now + SESSION_TTL,
        truncate(user_agent, USER_AGENT_MAX),
    )
    .execute(uow.conn())
    .await?;
    uow.commit().await?;
    pair(ctx, session.user_id, session.id, refresh, now)
}

/// A SimpleJWT refresh token from the Django backend: exchangeable once for a new session.
async fn exchange_legacy(ctx: &Context, token: &str, user_agent: &str) -> AppResult<TokenPair> {
    let now = ctx.clock.now();
    let (jti, user_id) = ctx
        .signer
        .verify_legacy_refresh(token, now)
        .ok_or_else(session_expired)?;
    let mut uow = Uow::begin(&ctx.db).await?;
    let exchanged = sqlx::query_scalar!(
        "delete from legacy_refresh_tokens t using users u
         where t.jti = $1 and t.user_id = $2 and t.expires_at > $3
           and u.id = t.user_id and u.is_active and u.password_hash is not null
         returning t.user_id",
        jti,
        user_id,
        now,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or_else(session_expired)?;
    let tokens = open_session(&mut uow, ctx, exchanged, user_agent).await?;
    uow.commit().await?;
    Ok(tokens)
}

/// The caller behind a bearer access token: the token must be valid, and its session live and
/// its account active.
pub async fn authenticate(ctx: &Context, access_token: &str) -> AppResult<Actor> {
    let now = ctx.clock.now();
    let token = ctx
        .signer
        .verify(access_token, now)
        .ok_or_else(invalid_token)?;
    let actor = sqlx::query!(
        "select u.id, u.username, u.is_superuser
         from auth_sessions s join users u on u.id = s.user_id
         where s.id = $1 and s.user_id = $2 and s.revoked_at is null and s.expires_at > $3
           and u.is_active and u.password_hash is not null",
        token.session_id,
        token.user_id,
        now,
    )
    .fetch_optional(&ctx.db)
    .await?
    .ok_or_else(invalid_token)?;
    Ok(Actor {
        user_id: actor.id,
        username: actor.username,
        is_superuser: actor.is_superuser,
        session_id: token.session_id,
    })
}

/// Whether `session_id` of `user_id` is still live: not revoked or expired, and the account
/// still active (what [`authenticate`] requires besides a valid token).
pub async fn session_live(ctx: &Context, user_id: i64, session_id: Uuid) -> AppResult<bool> {
    Ok(sqlx::query_scalar!(
        r#"select exists(
             select 1 from auth_sessions s join users u on u.id = s.user_id
             where s.id = $1 and s.user_id = $2 and s.revoked_at is null and s.expires_at > $3
               and u.is_active and u.password_hash is not null) as "live!""#,
        session_id,
        user_id,
        ctx.clock.now(),
    )
    .fetch_one(&ctx.db)
    .await?)
}

/// End the caller's session.
pub async fn logout(ctx: &Context, actor: &Actor) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    revoke(&mut uow, ctx, actor.user_id, Some(actor.session_id)).await?;
    uow.commit().await
}

/// End one session (`Some`) or every session (`None`) of `user_id`; their event streams close.
pub async fn revoke(
    uow: &mut Uow,
    ctx: &Context,
    user_id: i64,
    session_id: Option<Uuid>,
) -> AppResult<u64> {
    let revoked = sqlx::query_scalar!(
        "update auth_sessions set revoked_at = $3
         where user_id = $1 and ($2::uuid is null or id = $2) and revoked_at is null
         returning id",
        user_id,
        session_id,
        ctx.clock.now(),
    )
    .fetch_all(uow.conn())
    .await?;
    for session_id in &revoked {
        uow.publish(
            Audience::User(user_id),
            Event::SessionRevoked {
                session_id: *session_id,
            },
        )
        .await?;
    }
    Ok(revoked.len() as u64)
}

fn truncate(value: &str, max: usize) -> &str {
    value
        .char_indices()
        .nth(max)
        .map_or(value, |(at, _)| &value[..at])
}
