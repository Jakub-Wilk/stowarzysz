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

/// Subscriptions kept per person; registering one more forgets the least recently used.
pub const MAX_PER_USER: i64 = 10;

/// The push services browsers subscribe through (and their subdomains). The worker POSTs to
/// whatever endpoint is stored, so anything else would let a client aim the server at any host.
const PUSH_SERVICES: [&str; 4] = [
    "fcm.googleapis.com",        // Chrome, Android, Samsung, Opera
    "push.apple.com",            // Safari (web.push.apple.com)
    "notify.windows.com",        // Edge (wns2-*.notify.windows.com)
    "push.services.mozilla.com", // Firefox (updates.push.services.mozilla.com)
];

fn is_push_service(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    PUSH_SERVICES.iter().any(|service| {
        host == *service
            || host
                .strip_suffix(service)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

/// Whether `endpoint` is a URL the worker may POST to. In development any host (and plain
/// http) is allowed, for local push servers.
fn endpoint_ok(endpoint: &str, dev: bool) -> bool {
    if endpoint.len() > MAX_ENDPOINT {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    if dev {
        return matches!(url.scheme(), "https" | "http");
    }
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(is_push_service)
}

fn validate(subscription: &Subscription, dev: bool) -> Vec<DomainError> {
    let mut errors = Vec::new();
    if !endpoint_ok(subscription.endpoint.trim(), dev) {
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
    let endpoint = subscription.endpoint.trim();
    let mut tx = ctx.db.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query!(
        "insert into push_subscriptions (user_id, endpoint, p256dh, auth, user_agent)
         values ($1, $2, $3, $4, $5)
         on conflict (endpoint) do update set
             user_id = excluded.user_id, p256dh = excluded.p256dh, auth = excluded.auth,
             user_agent = excluded.user_agent, failure_count = 0",
        user_id,
        endpoint,
        subscription.keys.p256dh.trim(),
        subscription.keys.auth.trim(),
        user_agent,
    )
    .execute(&mut *tx)
    .await?;
    // Keep this one and the most recently used others, up to the cap.
    sqlx::query!(
        "delete from push_subscriptions where id in (
             select id from push_subscriptions
             where user_id = $1 and endpoint <> $2
             order by coalesce(last_success_at, created_at) desc, id desc
             offset $3)",
        user_id,
        endpoint,
        MAX_PER_USER - 1,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
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

#[cfg(test)]
mod tests {
    use super::endpoint_ok;

    #[test]
    fn only_known_push_services_are_accepted_outside_development() {
        for endpoint in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://web.push.apple.com/QGx",
            "https://wns2-par02p.notify.windows.com/w/?token=x",
            "https://updates.push.services.mozilla.com/wpush/v2/x",
            "https://FCM.googleapis.com./fcm/send/abc",
        ] {
            assert!(endpoint_ok(endpoint, false), "{endpoint}");
        }
        for endpoint in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://fcm.googleapis.com:8443/fcm/send/abc",
            "https://user@fcm.googleapis.com/fcm/send/abc",
            "https://evilfcm.googleapis.com.example.org/x",
            "https://notfcm.googleapis.com.evil/x",
            "https://xpush.apple.com/x",
            "https://10.0.0.5/x",
            "https://localhost/x",
            "https://push.test/x",
            "not a url",
        ] {
            assert!(!endpoint_ok(endpoint, false), "{endpoint}");
        }
    }

    #[test]
    fn development_accepts_any_http_host() {
        assert!(endpoint_ok("https://push.test/x", true));
        assert!(endpoint_ok("http://localhost:8080/x", true));
        assert!(!endpoint_ok("ftp://push.test/x", true));
    }
}
