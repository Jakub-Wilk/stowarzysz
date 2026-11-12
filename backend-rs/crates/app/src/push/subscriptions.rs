//! Browsers registering for (and leaving) Web Push.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::DomainError;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{AppResult, Context};

const MAX_ENDPOINT: usize = 2048;

/// A browser's push subscription (`PushSubscription.toJSON()`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct Subscription {
    /// The push service URL (https).
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SubscriptionKeys {
    /// The browser's P-256 public key, base64url.
    pub p256dh: String,
    /// The 16-byte auth secret, base64url.
    pub auth: String,
}

fn validate(subscription: &Subscription, allow_http: bool) -> Vec<DomainError> {
    let mut errors = Vec::new();
    let endpoint = subscription.endpoint.trim();
    let scheme_ok =
        endpoint.starts_with("https://") || (allow_http && endpoint.starts_with("http://"));
    if !scheme_ok || endpoint.len() > MAX_ENDPOINT || reqwest::Url::parse(endpoint).is_err() {
        errors.push(
            DomainError::new("invalid_endpoint", "Nieprawidłowy adres subskrypcji.").on("endpoint"),
        );
    }
    let decodes_to = |value: &str, len: usize| {
        URL_SAFE_NO_PAD
            .decode(value.trim().trim_end_matches('='))
            .is_ok_and(|bytes| bytes.len() == len)
    };
    if !decodes_to(&subscription.keys.p256dh, 65) {
        errors.push(
            DomainError::new("invalid_key", "Nieprawidłowy klucz subskrypcji.").on("keys.p256dh"),
        );
    }
    if !decodes_to(&subscription.keys.auth, 16) {
        errors.push(
            DomainError::new("invalid_key", "Nieprawidłowy klucz subskrypcji.").on("keys.auth"),
        );
    }
    errors
}

/// Register `subscription` for `user_id`. An endpoint identifies a browser: if someone else
/// signs in on it, it follows them.
pub async fn subscribe(
    ctx: &Context,
    user_id: i64,
    subscription: &Subscription,
    user_agent: &str,
) -> AppResult<()> {
    let errors = validate(subscription, ctx.config.is_dev());
    if !errors.is_empty() {
        return Err(errors.into());
    }
    let user_agent: String = user_agent.chars().take(255).collect();
    sqlx::query!(
        "insert into push_subscriptions (user_id, endpoint, p256dh, auth, user_agent)
         values ($1, $2, $3, $4, $5)
         on conflict (endpoint) do update set
             user_id = excluded.user_id, p256dh = excluded.p256dh, auth = excluded.auth,
             user_agent = excluded.user_agent, failure_count = 0",
        user_id,
        subscription.endpoint.trim(),
        subscription.keys.p256dh.trim(),
        subscription.keys.auth.trim(),
        user_agent,
    )
    .execute(&ctx.db)
    .await?;
    Ok(())
}

/// Forget `endpoint`, if it is `user_id`'s.
pub async fn unsubscribe(ctx: &Context, user_id: i64, endpoint: &str) -> AppResult<()> {
    sqlx::query!(
        "delete from push_subscriptions where user_id = $1 and endpoint = $2",
        user_id,
        endpoint.trim(),
    )
    .execute(&ctx.db)
    .await?;
    Ok(())
}
