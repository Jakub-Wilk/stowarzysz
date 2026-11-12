//! Web Push through a transactional outbox.
//!
//! [`Push::notify`] writes one `push_outbox` row per subscription in the caller's transaction, so
//! a notification exists exactly when the change that caused it was committed. The
//! [`PushWorker`] delivers rows after commit with bounded concurrency, retries transient failures
//! with backoff until the notification's TTL, and forgets subscriptions the push service reports
//! as gone.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use futures_util::future::BoxFuture;
use futures_util::stream::{self, StreamExt as _};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use web_push_native::jwt_simple::algorithms::ES256KeyPair;
use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint as _;
use web_push_native::{Auth, WebPushBuilder, p256};

use crate::AppResult;
use crate::config::Vapid;
use crate::db::Uow;
use crate::listener::Wake;

/// How long a push service keeps trying to deliver a notification (and how long we do).
pub const TTL: Duration = Duration::from_hours(1);
const BATCH: i64 = 32;
const CONCURRENCY: usize = 8;
const MAX_ATTEMPTS: i16 = 6;
/// A subscription that keeps rejecting notifications is dropped after this many in a row.
const MAX_FAILURES: i32 = 10;
/// The worker looks at the outbox at least this often, even without a wake-up.
const IDLE: Duration = Duration::from_secs(30);

/// What the service worker shows. Polish text; `url` is the in-app path to open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub url: String,
}

/// Queues notifications. Without VAPID keys push is disabled and nothing is queued.
#[derive(Debug, Clone, Copy)]
pub struct Push {
    enabled: bool,
}

impl Push {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }

    /// Queue `notification` for every subscription of `user_ids`, in `uow`'s transaction.
    /// Returns how many deliveries were queued.
    pub async fn notify(
        self,
        uow: &mut Uow,
        user_ids: &[i64],
        notification: &Notification,
    ) -> AppResult<u64> {
        if !self.enabled || user_ids.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(notification).map_err(anyhow::Error::from)?;
        let queued = sqlx::query!(
            "insert into push_outbox (subscription_id, payload, expires_at)
             select id, $2, now() + make_interval(secs => $3)
             from push_subscriptions where user_id = any($1)",
            user_ids,
            payload,
            TTL.as_secs_f64(),
        )
        .execute(uow.conn())
        .await
        .map_err(anyhow::Error::from)?
        .rows_affected();
        if queued > 0 {
            uow.wake(Wake::Push).await?;
        }
        Ok(queued)
    }
}

/// One browser endpoint to deliver to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// How one delivery attempt went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    /// The subscription no longer exists (404/410, or its keys are unusable).
    Gone,
    /// Try again later (429, 5xx, network trouble), after at least the given delay.
    Retry(Option<Duration>),
    /// The push service refused this notification (400, 401, 403, 413).
    Rejected(String),
}

/// Sends one encrypted notification. The real one speaks Web Push; tests record calls.
pub trait PushTransport: Send + Sync + fmt::Debug {
    fn deliver<'a>(&'a self, target: &'a Target, payload: &'a [u8]) -> BoxFuture<'a, Outcome>;
}

/// RFC 8030/8291/8292 Web Push with VAPID over a shared HTTP/2 client.
pub struct WebPushTransport {
    client: reqwest::Client,
    key: ES256KeyPair,
    subject: String,
}

impl fmt::Debug for WebPushTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebPushTransport")
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

impl WebPushTransport {
    /// Fails if the keys are malformed or the public key does not belong to the private key.
    pub fn new(vapid: &Vapid) -> anyhow::Result<Self> {
        let private = decode_b64(vapid.private_key.expose())
            .ok_or_else(|| anyhow::anyhow!("VAPID_PRIVATE_KEY is not base64"))?;
        let secret = p256::SecretKey::from_slice(&private)
            .map_err(|_| anyhow::anyhow!("VAPID_PRIVATE_KEY is not a P-256 private key"))?;
        let derived = secret.public_key().to_encoded_point(false);
        let configured = decode_b64(&vapid.public_key)
            .ok_or_else(|| anyhow::anyhow!("VAPID_PUBLIC_KEY is not base64"))?;
        anyhow::ensure!(
            derived.as_bytes() == configured.as_slice(),
            "VAPID_PUBLIC_KEY does not match VAPID_PRIVATE_KEY"
        );
        let key = ES256KeyPair::from_bytes(&private)
            .map_err(|error| anyhow::anyhow!("VAPID_PRIVATE_KEY: {error}"))?;
        let client = reqwest::Client::builder()
            .user_agent("stowarzysz")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self {
            client,
            key,
            subject: vapid.subject.clone(),
        })
    }

    async fn send(&self, target: &Target, payload: &[u8]) -> Outcome {
        let request = match self.build(target, payload) {
            Ok(request) => request,
            Err(error) => {
                tracing::info!(%error, "dropping a subscription with unusable keys");
                return Outcome::Gone;
            }
        };
        match self.client.execute(request).await {
            Ok(response) => classify(response.status().as_u16(), retry_after(&response)),
            Err(error) => {
                tracing::debug!(%error, "push delivery failed");
                Outcome::Retry(None)
            }
        }
    }

    fn build(&self, target: &Target, payload: &[u8]) -> anyhow::Result<reqwest::Request> {
        let endpoint = target.endpoint.parse()?;
        let p256dh = decode_b64(&target.p256dh).ok_or_else(|| anyhow::anyhow!("p256dh"))?;
        let public = p256::PublicKey::from_sec1_bytes(&p256dh)?;
        let auth = decode_b64(&target.auth).ok_or_else(|| anyhow::anyhow!("auth"))?;
        anyhow::ensure!(auth.len() == 16, "auth must be 16 bytes");
        let request = WebPushBuilder::new(endpoint, public, Auth::clone_from_slice(&auth))
            .with_valid_duration(TTL)
            .with_vapid(&self.key, &self.subject)
            .build(payload.to_vec())?;
        Ok(reqwest::Request::try_from(request)?)
    }
}

impl PushTransport for WebPushTransport {
    fn deliver<'a>(&'a self, target: &'a Target, payload: &'a [u8]) -> BoxFuture<'a, Outcome> {
        Box::pin(self.send(target, payload))
    }
}

fn classify(status: u16, retry_after: Option<Duration>) -> Outcome {
    match status {
        200..=299 => Outcome::Delivered,
        404 | 410 => Outcome::Gone,
        408 | 429 | 500..=599 => Outcome::Retry(retry_after),
        other => Outcome::Rejected(format!("HTTP {other}")),
    }
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let value = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?;
    value.trim().parse().ok().map(Duration::from_secs)
}

/// Browsers hand out base64url without padding; be lenient about the variant.
fn decode_b64(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    [URL_SAFE_NO_PAD, URL_SAFE, STANDARD]
        .iter()
        .find_map(|engine| engine.decode(value).ok())
}

/// Delay before the next attempt (attempts made so far: 1, 2, ...).
fn backoff(attempts: i16) -> Duration {
    match attempts {
        ..=1 => Duration::from_secs(10),
        2 => Duration::from_secs(30),
        3 => Duration::from_mins(2),
        _ => Duration::from_mins(10),
    }
}

/// Delivers the outbox. Run one per process; rows are claimed with `SKIP LOCKED`, so several
/// processes share the work without sending anything twice.
#[derive(Debug, Clone)]
pub struct PushWorker {
    pool: PgPool,
    transport: Arc<dyn PushTransport>,
}

struct Due {
    id: i64,
    subscription_id: i64,
    payload: serde_json::Value,
    attempts: i16,
    expires_at: DateTime<Utc>,
    endpoint: String,
    p256dh: String,
    auth: String,
}

impl PushWorker {
    pub fn new(pool: PgPool, transport: Arc<dyn PushTransport>) -> Self {
        Self { pool, transport }
    }

    /// Deliver until cancelled, sleeping until the next row is due or a wake-up arrives.
    pub async fn run(self, wake: Arc<Notify>, cancel: CancellationToken) {
        loop {
            let next = match self.drain().await {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "push outbox drain failed");
                    None
                }
            };
            let pause = next
                .map_or(IDLE, |at| {
                    (at - Utc::now()).to_std().unwrap_or(Duration::ZERO)
                })
                .min(IDLE);
            tokio::select! {
                () = cancel.cancelled() => return,
                () = wake.notified() => {}
                () = tokio::time::sleep(pause) => {}
            }
        }
    }

    /// Deliver everything that is due now. Returns when the next remaining row is due.
    pub async fn drain(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        sqlx::query!("delete from push_outbox where expires_at <= now()")
            .execute(&self.pool)
            .await?;
        while self.deliver_batch().await? == usize::try_from(BATCH)? {}
        Ok(
            sqlx::query_scalar!("select min(next_attempt_at) from push_outbox")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    async fn deliver_batch(&self) -> anyhow::Result<usize> {
        let mut tx = self.pool.begin().await?;
        let due = sqlx::query_as!(
            Due,
            r#"select o.id, o.subscription_id, o.payload, o.attempts, o.expires_at,
                      s.endpoint, s.p256dh, s.auth
               from push_outbox o join push_subscriptions s on s.id = o.subscription_id
               where o.next_attempt_at <= now()
               order by o.id
               limit $1
               for update of o skip locked"#,
            BATCH,
        )
        .fetch_all(&mut *tx)
        .await?;
        let claimed = due.len();

        let transport = &self.transport;
        let outcomes: Vec<(Due, Outcome)> = stream::iter(due)
            .map(|row| async move {
                let target = Target {
                    endpoint: row.endpoint.clone(),
                    p256dh: row.p256dh.clone(),
                    auth: row.auth.clone(),
                };
                let payload = row.payload.to_string();
                let outcome = transport.deliver(&target, payload.as_bytes()).await;
                (row, outcome)
            })
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await;

        for (row, outcome) in outcomes {
            match outcome {
                Outcome::Delivered => {
                    sqlx::query!("delete from push_outbox where id = $1", row.id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query!(
                        "update push_subscriptions set last_success_at = now(), failure_count = 0
                         where id = $1",
                        row.subscription_id,
                    )
                    .execute(&mut *tx)
                    .await?;
                }
                Outcome::Gone => {
                    // Cascades to every queued row of this subscription.
                    sqlx::query!(
                        "delete from push_subscriptions where id = $1",
                        row.subscription_id
                    )
                    .execute(&mut *tx)
                    .await?;
                }
                Outcome::Retry(after) => {
                    let attempts = row.attempts + 1;
                    let delay = after.unwrap_or_default().max(backoff(attempts));
                    let next = Utc::now() + TimeDelta::from_std(delay)?;
                    if attempts >= MAX_ATTEMPTS || next >= row.expires_at {
                        self.give_up(&mut tx, &row, "retries exhausted").await?;
                    } else {
                        sqlx::query!(
                            "update push_outbox set attempts = $2, next_attempt_at = $3 where id = $1",
                            row.id,
                            attempts,
                            next,
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                }
                Outcome::Rejected(reason) => self.give_up(&mut tx, &row, &reason).await?,
            }
        }
        tx.commit().await?;
        Ok(claimed)
    }

    async fn give_up(
        &self,
        tx: &mut sqlx::PgConnection,
        row: &Due,
        reason: &str,
    ) -> anyhow::Result<()> {
        tracing::warn!(endpoint = %truncate(&row.endpoint, 60), reason, "push notification dropped");
        sqlx::query!("delete from push_outbox where id = $1", row.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!(
            "delete from push_subscriptions where id = $1 and failure_count + 1 >= $2",
            row.subscription_id,
            MAX_FAILURES,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "update push_subscriptions set failure_count = failure_count + 1 where id = $1",
            row.subscription_id,
        )
        .execute(&mut *tx)
        .await?;
        Ok(())
    }
}

fn truncate(value: &str, max: usize) -> &str {
    value
        .char_indices()
        .nth(max)
        .map_or(value, |(at, _)| &value[..at])
}

/// A transport that records deliveries and replays scripted outcomes; for tests.
#[cfg(any(test, feature = "test-util"))]
pub mod testing {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    use super::{BoxFuture, Notification, Outcome, PushTransport, Target};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Delivery {
        pub endpoint: String,
        pub notification: Notification,
    }

    #[derive(Debug, Default)]
    pub struct RecordingTransport {
        delivered: Mutex<Vec<Delivery>>,
        scripted: Mutex<HashMap<String, VecDeque<Outcome>>>,
    }

    impl RecordingTransport {
        /// The next deliveries to `endpoint` return these outcomes, then `Delivered`.
        pub fn script(&self, endpoint: &str, outcomes: impl IntoIterator<Item = Outcome>) {
            self.scripted
                .lock()
                .expect("not poisoned")
                .entry(endpoint.to_owned())
                .or_default()
                .extend(outcomes);
        }

        /// Successful deliveries so far, in order.
        pub fn delivered(&self) -> Vec<Delivery> {
            self.delivered.lock().expect("not poisoned").clone()
        }
    }

    impl PushTransport for RecordingTransport {
        fn deliver<'a>(&'a self, target: &'a Target, payload: &'a [u8]) -> BoxFuture<'a, Outcome> {
            let outcome = self
                .scripted
                .lock()
                .expect("not poisoned")
                .get_mut(&target.endpoint)
                .and_then(VecDeque::pop_front)
                .unwrap_or(Outcome::Delivered);
            if outcome == Outcome::Delivered {
                let notification =
                    serde_json::from_slice(payload).expect("payload is a notification");
                self.delivered.lock().expect("not poisoned").push(Delivery {
                    endpoint: target.endpoint.clone(),
                    notification,
                });
            }
            Box::pin(async move { outcome })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Secret;

    #[test]
    fn statuses_map_to_outcomes() {
        assert_eq!(classify(201, None), Outcome::Delivered);
        assert_eq!(classify(410, None), Outcome::Gone);
        assert_eq!(classify(404, None), Outcome::Gone);
        let wait = Some(Duration::from_secs(5));
        assert_eq!(classify(429, wait), Outcome::Retry(wait));
        assert_eq!(classify(503, None), Outcome::Retry(None));
        assert_eq!(classify(413, None), Outcome::Rejected("HTTP 413".into()));
    }

    #[test]
    fn backoff_grows() {
        assert!(backoff(1) < backoff(2) && backoff(2) < backoff(3) && backoff(3) < backoff(4));
        assert_eq!(backoff(9), backoff(4));
    }

    // A real VAPID pair, generated for these tests only (the mismatch test uses a second pair).
    const PRIVATE: &str = "rp_3tg50IMtLFfwWrI1aYaM1tN7E5roukpEP8HrQ5yQ";
    const PUBLIC: &str =
        "BOiOUte3VhqJOjcXfKxUCRT0nATViFckskhpJzhPC8OOgAiiMvQa9cHt1awa7_eWJuNDimhiGqrg_VJZnIW1Wvg";

    fn vapid(public: &str) -> Vapid {
        Vapid {
            public_key: public.into(),
            private_key: Secret::new(PRIVATE),
            subject: "mailto:test@example.org".into(),
        }
    }

    #[test]
    fn vapid_keys_must_match() {
        let error = WebPushTransport::new(&vapid(
            "BKh4tUVIel11GB8HjF1OSqxU74m5T2b1RwZIAapVAz7tBdnTCzPtCCfhGaX_7cNqHsx6e3I24iKusQZLpQRJO4U",
        ))
        .unwrap_err();
        assert!(error.to_string().contains("does not match"), "{error}");
    }

    #[test]
    fn builds_an_encrypted_vapid_request() {
        let transport = WebPushTransport::new(&vapid(PUBLIC)).unwrap();
        // A subscription as a browser would hand it out (keys from RFC 8291's example).
        let target = Target {
            endpoint: "https://push.example.net/push/JzLQ3raZJfFBR0aqvOMsLrt54w4rJUsV".into(),
            p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4".into(),
            auth: "BTBZMqHH6r4Tts7J_aSIgg".into(),
        };
        let request = transport.build(&target, br#"{"title":"t"}"#).unwrap();
        assert_eq!(request.url().as_str(), target.endpoint);
        let headers = request.headers();
        assert_eq!(headers["content-encoding"], "aes128gcm");
        assert_eq!(headers["ttl"], "3600");
        assert!(
            headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("vapid t=")
        );
    }

    #[test]
    fn unusable_subscription_keys_fail_to_build() {
        let transport = WebPushTransport::new(&vapid(PUBLIC)).unwrap();
        let target = Target {
            endpoint: "https://push.example.net/x".into(),
            p256dh: "not-a-key".into(),
            auth: "short".into(),
        };
        assert!(transport.build(&target, b"{}").is_err());
    }
}
