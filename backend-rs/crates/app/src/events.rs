//! Server-sent events: what can be announced, to whom, and the in-process hub that fans
//! committed events out to open SSE connections.
//!
//! Events are published with [`crate::db::Uow::publish`] (`pg_notify` inside the transaction),
//! so they are delivered only if the transaction commits, and to every process: each process
//! runs one listener ([`crate::listener`]) that feeds its [`Hub`]. Payloads carry ids only (plus
//! the few fields a client needs without refetching), so visibility rules stay in the API.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use utoipa::ToSchema;
use uuid::Uuid;

/// The Postgres NOTIFY channel events travel on.
pub const CHANNEL_EVENTS: &str = "app_events";

/// Everything the server announces over `/api/events`. The SSE `event:` field is the `type`
/// and `data:` is the JSON of `data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    #[serde(rename = "poll.created")]
    PollCreated { poll_id: i64 },
    #[serde(rename = "poll.updated")]
    PollUpdated { poll_id: i64 },
    #[serde(rename = "poll.closed")]
    PollClosed { poll_id: i64 },
    /// Carries the emoji so clients patch their copy instead of refetching.
    #[serde(rename = "poll.reaction")]
    PollReaction {
        poll_id: i64,
        user_id: i64,
        emoji: String,
    },
    #[serde(rename = "pact.created")]
    PactCreated { pact_id: i64 },
    #[serde(rename = "pact.updated")]
    PactUpdated { pact_id: i64 },
    #[serde(rename = "ledger.updated")]
    LedgerUpdated { entry_id: i64 },
    /// Sent to the caller only, right before each request to an OCR model.
    #[serde(rename = "ledger.ocr.request")]
    LedgerOcrRequest {
        job_id: Uuid,
        attempt: u32,
        candidate: String,
    },
    #[serde(rename = "santa.updated")]
    SantaUpdated { event_id: i64 },
    #[serde(rename = "santa.help")]
    SantaHelp { event_id: i64 },
    /// The session behind this stream was revoked; the client should sign out.
    #[serde(rename = "session.revoked")]
    SessionRevoked { session_id: Uuid },
}

impl Event {
    /// The SSE event name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::PollCreated { .. } => "poll.created",
            Self::PollUpdated { .. } => "poll.updated",
            Self::PollClosed { .. } => "poll.closed",
            Self::PollReaction { .. } => "poll.reaction",
            Self::PactCreated { .. } => "pact.created",
            Self::PactUpdated { .. } => "pact.updated",
            Self::LedgerUpdated { .. } => "ledger.updated",
            Self::LedgerOcrRequest { .. } => "ledger.ocr.request",
            Self::SantaUpdated { .. } => "santa.updated",
            Self::SantaHelp { .. } => "santa.help",
            Self::SessionRevoked { .. } => "session.revoked",
        }
    }

    /// The SSE `data:` payload.
    pub fn data(&self) -> serde_json::Value {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(mut tagged)) => {
                tagged.remove("data").unwrap_or(serde_json::Value::Null)
            }
            _ => serde_json::Value::Null,
        }
    }
}

/// Who receives an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    /// Every signed-in member with an open stream.
    All,
    User(i64),
}

impl Audience {
    pub fn includes(self, user_id: i64) -> bool {
        match self {
            Self::All => true,
            Self::User(id) => id == user_id,
        }
    }
}

/// What travels over NOTIFY.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub audience: Audience,
    pub event: Event,
}

/// What a stream receives from the hub.
#[derive(Debug, Clone)]
pub enum HubMessage {
    Event(Arc<Envelope>),
    /// Events may have been missed (the listener reconnected): clients should refetch.
    Resync,
}

/// In-process fan-out of committed events to SSE connections. Cloning is cheap; a broadcast
/// costs O(1) regardless of how many users there are (each stream filters by audience).
#[derive(Debug, Clone)]
pub struct Hub {
    sender: broadcast::Sender<HubMessage>,
}

impl Hub {
    /// `capacity` is how far a slow stream may fall behind before it is told to resync.
    pub fn new(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HubMessage> {
        self.sender.subscribe()
    }

    /// Hand a committed event to every open stream (no-op without subscribers).
    pub fn deliver(&self, envelope: Envelope) {
        let _ = self.sender.send(HubMessage::Event(Arc::new(envelope)));
    }

    pub fn resync(&self) {
        let _ = self.sender.send(HubMessage::Resync);
    }

    pub fn subscribers(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_through_notify_json() {
        let envelope = Envelope {
            audience: Audience::User(7),
            event: Event::PollReaction {
                poll_id: 3,
                user_id: 7,
                emoji: "🔥".into(),
            },
        };
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            json,
            r#"{"audience":{"user":7},"event":{"type":"poll.reaction","data":{"poll_id":3,"user_id":7,"emoji":"🔥"}}}"#
        );
        assert_eq!(serde_json::from_str::<Envelope>(&json).unwrap(), envelope);
    }

    #[test]
    fn name_and_data_match_the_serialised_tag() {
        let event = Event::LedgerUpdated { entry_id: 12 };
        assert_eq!(event.name(), "ledger.updated");
        assert_eq!(event.data(), serde_json::json!({"entry_id": 12}));
        let tagged = serde_json::to_value(&event).unwrap();
        assert_eq!(tagged["type"], event.name());
    }

    #[test]
    fn audience_filters_by_user() {
        assert!(Audience::All.includes(1));
        assert!(Audience::User(1).includes(1));
        assert!(!Audience::User(1).includes(2));
    }

    #[tokio::test]
    async fn hub_fans_out_to_every_subscriber() {
        let hub = Hub::new(8);
        let (mut a, mut b) = (hub.subscribe(), hub.subscribe());
        hub.deliver(Envelope {
            audience: Audience::All,
            event: Event::PactCreated { pact_id: 1 },
        });
        for rx in [&mut a, &mut b] {
            match rx.recv().await.unwrap() {
                HubMessage::Event(e) => assert_eq!(e.event, Event::PactCreated { pact_id: 1 }),
                HubMessage::Resync => panic!("unexpected resync"),
            }
        }
    }
}
