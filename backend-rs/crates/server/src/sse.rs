//! The SSE stream behind `/api/events`: one per open client, fed by the process's [`Hub`].
//!
//! The stream starts with `ready` (clients refetch what they show), then carries every committed
//! event meant for the user, `resync` whenever events may have been missed, and a comment
//! heartbeat every 15 s so proxies keep the connection open. It ends on shutdown, or after
//! `session.revoked` for the stream's own session.

use std::convert::Infallible;
use std::time::Duration;

use app::events::{Event, HubMessage};
use axum::response::sse::{self, KeepAlive, Sse};
use futures_util::stream::{self, Stream, StreamExt as _};
use tokio::sync::broadcast::{Receiver, error::RecvError};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const HEARTBEAT: Duration = Duration::from_secs(15);

/// Whose stream this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscriber {
    pub user_id: i64,
    pub session_id: Uuid,
}

/// The SSE response for `subscriber`.
pub fn response(
    hub: Receiver<HubMessage>,
    subscriber: Subscriber,
    shutdown: CancellationToken,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    Sse::new(stream(hub, subscriber, shutdown)).keep_alive(KeepAlive::new().interval(HEARTBEAT))
}

/// The events of one stream (without the heartbeat, which [`response`] adds).
pub fn stream(
    hub: Receiver<HubMessage>,
    subscriber: Subscriber,
    shutdown: CancellationToken,
) -> impl Stream<Item = Result<sse::Event, Infallible>> {
    let ready = stream::once(async { Ok(control("ready")) });
    let updates = stream::unfold(Some(hub), move |hub| async move {
        let mut hub = hub?; // None: the previous item was the last one
        loop {
            match hub.recv().await {
                Ok(HubMessage::Event(envelope))
                    if envelope.audience.includes(subscriber.user_id) =>
                {
                    let last = matches!(
                        envelope.event,
                        Event::SessionRevoked { session_id } if session_id == subscriber.session_id
                    );
                    return Some((Ok(to_sse(&envelope.event)), (!last).then_some(hub)));
                }
                Ok(HubMessage::Event(_)) => {} // for someone else
                Ok(HubMessage::Resync) | Err(RecvError::Lagged(_)) => {
                    return Some((Ok(control("resync")), Some(hub)));
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
