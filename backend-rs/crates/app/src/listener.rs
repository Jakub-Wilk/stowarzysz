//! One `LISTEN` connection per process: routes committed SSE events to the [`Hub`] and wakes
//! background workers when work was queued for them.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::events::{CHANNEL_EVENTS, Envelope, Hub};

/// A background worker that can be woken through NOTIFY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// New push outbox rows.
    Push,
    /// Something the scheduler should look at sooner than its next planned run.
    Jobs,
}

impl Wake {
    pub fn channel(self) -> &'static str {
        match self {
            Self::Push => "push_outbox",
            Self::Jobs => "jobs",
        }
    }
}

/// In-process wake-up signals, fed by the listener.
#[derive(Debug, Clone, Default)]
pub struct Signals {
    pub push: Arc<Notify>,
    pub jobs: Arc<Notify>,
}

impl Signals {
    pub fn notify(&self, wake: Wake) {
        match wake {
            Wake::Push => self.push.notify_one(),
            Wake::Jobs => self.jobs.notify_one(),
        }
    }
}

/// Listen until cancelled, reconnecting with backoff. Whenever the connection is (re)made,
/// streams are told to resync: notifications sent while it was down are lost.
pub async fn run(pool: PgPool, hub: Hub, signals: Signals, cancel: CancellationToken) {
    let mut backoff = Duration::from_millis(500);
    loop {
        match listen(&pool, &hub, &signals, &cancel).await {
            Ok(()) => return, // cancelled
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    ?backoff,
                    "event listener failed"
                );
            }
        }
        // Wake everything: work may have been queued while we were not listening.
        signals.notify(Wake::Push);
        signals.notify(Wake::Jobs);
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn listen(
    pool: &PgPool,
    hub: &Hub,
    signals: &Signals,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener
        .listen_all([CHANNEL_EVENTS, Wake::Push.channel(), Wake::Jobs.channel()])
        .await?;
    tracing::debug!("event listener connected");
    hub.resync();
    loop {
        let notification = tokio::select! {
            () = cancel.cancelled() => return Ok(()),
            received = listener.try_recv() => received?,
        };
        let Some(notification) = notification else {
            // The connection dropped; the next `try_recv` reconnects.
            tracing::warn!("event listener lost its connection; reconnecting");
            hub.resync();
            signals.notify(Wake::Push);
            signals.notify(Wake::Jobs);
            continue;
        };
        match notification.channel() {
            CHANNEL_EVENTS => match serde_json::from_str::<Envelope>(notification.payload()) {
                Ok(envelope) => hub.deliver(envelope),
                Err(error) => tracing::error!(%error, "malformed event notification"),
            },
            channel if channel == Wake::Push.channel() => signals.notify(Wake::Push),
            channel if channel == Wake::Jobs.channel() => signals.notify(Wake::Jobs),
            other => tracing::warn!(channel = other, "notification on an unexpected channel"),
        }
    }
}
