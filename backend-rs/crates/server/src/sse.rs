//! The SSE stream behind `/api/events`: one per open client, fed by the process's [`Hub`].
//!
//! The stream starts with `ready` (clients refetch what they show), then carries every committed
//! event meant for the user, `resync` whenever events may have been missed, and a comment
//! heartbeat every 15 s so proxies keep the connection open. It ends on shutdown, or after
//! `session.revoked` for the stream's own session. That event arrives through the hub, so if it
//! may have been missed (a `resync`), and every [`RECHECK`] anyway, the session is looked up
//! again and the stream ends (with `session.revoked`) once it is gone.

use std::convert::Infallible;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use app::events::{Event, HubMessage};
use axum::response::sse::{self, KeepAlive, Sse};
use futures_util::future::BoxFuture;
use futures_util::stream::{self, Stream, StreamExt as _};
use tokio::sync::broadcast::{Receiver, error::RecvError};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const HEARTBEAT: Duration = Duration::from_secs(15);
/// How often an open stream checks that its session is still live.
pub const RECHECK: Duration = Duration::from_mins(5);

/// Whose stream this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscriber {
    pub user_id: i64,
    pub session_id: Uuid,
}

/// Whether the stream's session is still live (signed in, not revoked, account active).
#[derive(Clone)]
pub struct SessionCheck(Arc<dyn Fn() -> BoxFuture<'static, bool> + Send + Sync>);

impl SessionCheck {
    pub fn new<F, Fut>(check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self(Arc::new(move || Box::pin(check())))
    }

    /// A session that never ends (for tests).
    pub fn always_live() -> Self {
        Self::new(|| async { true })
    }

    async fn live(&self) -> bool {
        (self.0)().await
    }
}

impl fmt::Debug for SessionCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionCheck")
    }
}

/// The SSE response for `subscriber`.
pub fn response(
    hub: Receiver<HubMessage>,
    subscriber: Subscriber,
    check: SessionCheck,
    shutdown: CancellationToken,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    Sse::new(stream(hub, subscriber, check, shutdown))
        .keep_alive(KeepAlive::new().interval(HEARTBEAT))
}

/// The events of one stream (without the heartbeat, which [`response`] adds).
pub fn stream(
    hub: Receiver<HubMessage>,
    subscriber: Subscriber,
    check: SessionCheck,
    shutdown: CancellationToken,
) -> impl Stream<Item = Result<sse::Event, Infallible>> {
    let ready = stream::once(async { Ok(control("ready")) });
    let revoked = move || {
        to_sse(&Event::SessionRevoked {
            session_id: subscriber.session_id,
        })
    };
    // None: the previous item was the last one.
    let state = Some((hub, check, Instant::now() + RECHECK));
    let updates = stream::unfold(state, move |state| async move {
        let (mut hub, check, mut next_check) = state?;
        loop {
            let message = tokio::select! {
                message = hub.recv() => message,
                () = tokio::time::sleep_until(next_check) => {
                    if !check.live().await {
                        return Some((Ok(revoked()), None));
                    }
                    next_check = Instant::now() + RECHECK;
                    continue;
                }
            };
            match message {
                Ok(HubMessage::Event(envelope))
                    if envelope.audience.includes(subscriber.user_id) =>
                {
                    let last = matches!(
                        envelope.event,
                        Event::SessionRevoked { session_id } if session_id == subscriber.session_id
                    );
                    let rest = (!last).then_some((hub, check, next_check));
                    return Some((Ok(to_sse(&envelope.event)), rest));
                }
                Ok(HubMessage::Event(_)) => {} // for someone else
                Ok(HubMessage::Resync) | Err(RecvError::Lagged(_)) => {
                    // The revocation may have been among what was missed.
                    if !check.live().await {
                        return Some((Ok(revoked()), None));
                    }
                    return Some((Ok(control("resync")), Some((hub, check, next_check))));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });
    ready.chain(updates).take_until(shutdown.cancelled_owned())
}

fn to_sse(event: &Event) -> sse::Event {
    sse::Event::default()
        .event(event.name())
        .data(event.data().to_string())
}

fn control(name: &'static str) -> sse::Event {
    sse::Event::default().event(name).data("{}")
}
