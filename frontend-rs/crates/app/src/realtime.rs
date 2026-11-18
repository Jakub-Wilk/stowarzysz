//! The `/api/events` connection while signed in: events bump the refetch bus (they carry ids
//! only), `ready`/`resync` refetch everything, `session.revoked` signs out. Reconnects with
//! exponential backoff and jitter. Mirrors frontend/src/features/voting/realtime.tsx.

use std::collections::VecDeque;
use std::time::Duration;

use api::sse::ServerEvent;
use api::types::Event;
use api::{ApiError, Client};
use dioxus::prelude::*;
use futures_util::StreamExt as _;

use crate::query::{RefetchBus, Topic};
use crate::session::use_client;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamStatus {
    Connecting,
    Open,
    /// Waiting before the next attempt.
    Retrying,
    /// Gave up (signed out, or the server refused the stream).
    Stopped,
}

/// A reaction someone sent on a finished vote (patched into the open poll, not refetched).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaction {
    pub poll_id: i64,
    pub user_id: i64,
    pub emoji: String,
}

/// The receipt-OCR progress events of this user's own requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrRequest {
    pub job_id: String,
    pub attempt: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Realtime {
    pub status: Signal<StreamStatus>,
    /// The latest events, newest first (Diagnostyka shows them).
    pub log: Signal<VecDeque<String>>,
    pub reaction: Signal<Option<Reaction>>,
    pub ocr: Signal<Option<OcrRequest>>,
}

const LOG_SIZE: usize = 30;

/// Which topics an event invalidates.
fn topics(event: &Event) -> &'static [Topic] {
    match event {
        Event::PollCreated { .. } | Event::PollUpdated { .. } => &[Topic::Polls],
        Event::PollClosed { .. } => &[
            Topic::Polls,
            Topic::Pacts,
            Topic::Ledger,
            Topic::People,
            Topic::Me,
        ],
        Event::PactCreated { .. } | Event::PactUpdated { .. } => &[Topic::Pacts, Topic::Ledger],
        Event::LedgerUpdated { .. } => &[Topic::Ledger],
        Event::SantaUpdated { .. } | Event::SantaHelp { .. } => &[Topic::Santa],
        Event::PollReaction { .. }
        | Event::LedgerOcrRequest { .. }
        | Event::SessionRevoked { .. } => &[],
    }
}

/// Delay before reconnect attempt `attempt` (0-based): 1 s doubling to 30 s, plus up to 1 s.
fn backoff(attempt: u32) -> Duration {
    let base = 1000u64.saturating_mul(1 << attempt.min(5)).min(30_000);
    let jitter = u64::from(chrono::Utc::now().timestamp_subsec_millis());
    Duration::from_millis(base + jitter)
}

/// Whether a failure to open the stream is worth retrying.
fn retryable(error: &ApiError) -> bool {
    match error {
        ApiError::SignedOut => false,
        ApiError::Http { status, .. } => {
            !(400..500).contains(status) || matches!(status, 408 | 429)
        }
        ApiError::Network(_) | ApiError::Decode(_) => true,
    }
}

/// The realtime state, for the whole app (in `App`); `RealtimeConnection` feeds it.
pub fn use_realtime_provider() -> Realtime {
    use_context_provider(|| Realtime {
        status: Signal::new(StreamStatus::Stopped),
        log: Signal::new(VecDeque::new()),
        reaction: Signal::new(None),
        ocr: Signal::new(None),
    })
}

/// Holds the connection while mounted (`App` mounts it while signed in).
#[component]
pub fn RealtimeConnection() -> Element {
    let realtime = use_realtime();
    let bus = crate::query::use_refetch_bus();
    let client = use_client();
    use_future(move || {
        let client = client.clone();
        async move { run(client, realtime, bus).await }
    });
    use_drop(move || {
        let mut status = realtime.status;
        status.set(StreamStatus::Stopped);
    });
    rsx! {}
}

pub fn use_realtime() -> Realtime {
    use_context()
}

async fn run(client: Client, mut realtime: Realtime, bus: RefetchBus) {
    let mut attempt = 0;
    // The first `ready` comes right after the screen's queries started: refetching then would
    // only cancel and repeat them. After a reconnect, events may have been missed: refetch.
    let mut connected_before = false;
    loop {
        realtime.status.set(StreamStatus::Connecting);
        match client.events().await {
            Ok(stream) => {
                let mut stream = std::pin::pin!(stream);
                while let Some(item) = stream.next().await {
                    let Ok(event) = item else { break };
                    attempt = 0;
                    realtime.status.set(StreamStatus::Open);
                    let first_ready = matches!(event, ServerEvent::Ready) && !connected_before;
                    if matches!(event, ServerEvent::Ready) {
                        connected_before = true;
                    }
                    if !handle(&client, &mut realtime, bus, event, !first_ready) {
                        realtime.status.set(StreamStatus::Stopped);
                        return;
                    }
                }
            }
            Err(error) if !retryable(&error) => {
                realtime.status.set(StreamStatus::Stopped);
                return;
            }
            Err(_) => {}
        }
        realtime.status.set(StreamStatus::Retrying);
        crate::platform::sleep(backoff(attempt)).await;
        attempt = attempt.saturating_add(1);
    }
}

/// Apply one event (`refetch`: whether `ready` should refetch everything); `false` ends the
/// stream.
fn handle(
    client: &Client,
    realtime: &mut Realtime,
    bus: RefetchBus,
    event: ServerEvent,
    refetch: bool,
) -> bool {
    let label = match &event {
        ServerEvent::Ready => "ready".to_owned(),
        ServerEvent::Resync => "resync".to_owned(),
        ServerEvent::Event(event) => serde_json::to_string(event).unwrap_or_default(),
        ServerEvent::Unknown { name } => format!("(nieznane) {name}"),
    };
    {
        let mut log = realtime.log.write();
        log.push_front(format!(
            "{} {label}",
            crate::polish::dates::now().format("%H:%M:%S")
        ));
        log.truncate(LOG_SIZE);
    }
    match event {
        ServerEvent::Ready if !refetch => {}
        ServerEvent::Ready | ServerEvent::Resync => bus.bump_all(),
        ServerEvent::Unknown { .. } => {}
        ServerEvent::Event(event) => {
            bus.bump(topics(&event));
            match event {
                Event::PollReaction {
                    poll_id,
                    user_id,
                    emoji,
                } => realtime.reaction.set(Some(Reaction {
                    poll_id,
                    user_id,
                    emoji,
                })),
                Event::LedgerOcrRequest {
                    job_id, attempt, ..
                } => realtime.ocr.set(Some(OcrRequest {
                    job_id: job_id.to_string(),
                    attempt,
                })),
                Event::SessionRevoked { .. } => {
                    client.end_session();
                    return false;
                }
                _ => {}
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_invalidate_like_the_react_app() {
        assert_eq!(topics(&Event::PollCreated { poll_id: 1 }), &[Topic::Polls]);
        assert_eq!(
            topics(&Event::PactUpdated { pact_id: 1 }),
            &[Topic::Pacts, Topic::Ledger]
        );
        assert!(topics(&Event::PollClosed { poll_id: 1 }).contains(&Topic::Me));
        let reaction = Event::PollReaction {
            poll_id: 1,
            user_id: 2,
            emoji: "🔥".into(),
        };
        assert_eq!(topics(&reaction), &[] as &[Topic]);
    }

    #[test]
    fn retries_only_what_can_recover() {
        let http = |status| ApiError::Http {
            status,
            code: String::new(),
            detail: String::new(),
            errors: std::collections::HashMap::new(),
        };
        assert!(retryable(&ApiError::Network("reset".into())));
        assert!(retryable(&http(502)));
        assert!(retryable(&http(429)));
        assert!(!retryable(&http(403)));
        assert!(!retryable(&ApiError::SignedOut));
    }

    #[test]
    fn backs_off_up_to_half_a_minute() {
        assert!(backoff(0) >= Duration::from_secs(1) && backoff(0) < Duration::from_secs(2));
        assert!(backoff(3) >= Duration::from_secs(8));
        assert!(backoff(30) < Duration::from_secs(31));
    }
}
