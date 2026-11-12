//! Events: delivered through Postgres NOTIFY only when the transaction commits, fanned out by
//! the hub, and framed as SSE per subscriber.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use app::db::Uow;
use app::events::{Audience, Envelope, Event, Hub, HubMessage};
use axum::response::IntoResponse as _;
use common::TestApp;
use futures_util::StreamExt as _;
use server::sse::{self, Subscriber};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<HubMessage>) -> Envelope {
    let wait = async {
        loop {
            if let HubMessage::Event(envelope) = rx.recv().await.unwrap() {
                return (*envelope).clone();
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("an event arrives")
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_committed_events_are_delivered(pool: PgPool) {
    let app = TestApp::new(pool);
    let _listener = app.start_listener().await;
    let mut rx = app.ctx.hub.subscribe();

    // Rolled back: must never arrive.
    let mut uow = Uow::begin(app.db()).await.unwrap();
    uow.publish(Audience::All, Event::PollCreated { poll_id: 1 })
        .await
        .unwrap();
    drop(uow);

    // Committed: arrives, after the rolled-back one would have (NOTIFY preserves order).
    let mut uow = Uow::begin(app.db()).await.unwrap();
    uow.publish(Audience::User(5), Event::PollUpdated { poll_id: 2 })
        .await
        .unwrap();
    uow.publish(Audience::All, Event::PollClosed { poll_id: 2 })
        .await
        .unwrap();
    uow.commit().await.unwrap();

    let first = next_event(&mut rx).await;
    assert_eq!(
        first,
        Envelope {
            audience: Audience::User(5),
            event: Event::PollUpdated { poll_id: 2 }
        }
    );
    let second = next_event(&mut rx).await;
    assert_eq!(second.event, Event::PollClosed { poll_id: 2 });
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn events_are_not_visible_before_commit(pool: PgPool) {
    let app = TestApp::new(pool);
    let _listener = app.start_listener().await;
    let mut rx = app.ctx.hub.subscribe();

    let mut uow = Uow::begin(app.db()).await.unwrap();
    uow.publish(Audience::All, Event::PactCreated { pact_id: 9 })
        .await
        .unwrap();
    let early = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(early.is_err(), "delivered before commit: {early:?}");
    uow.commit().await.unwrap();
    assert_eq!(
        next_event(&mut rx).await.event,
        Event::PactCreated { pact_id: 9 }
    );
}

/// Render an SSE response and collect its text until `until` appears (or 2 s pass).
async fn transcript(response: axum::response::Response, until: &str) -> String {
    let mut body = response.into_body().into_data_stream();
    let mut text = String::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(chunk) = body.next().await {
            text.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if text.contains(until) {
                break;
            }
        }
    })
    .await;
    text
}

const ME: Subscriber = Subscriber {
    user_id: 1,
    session_id: Uuid::from_u128(1),
};

#[tokio::test]
async fn a_stream_starts_ready_and_carries_only_its_users_events() {
    let hub = Hub::new(16);
    let response = sse::response(hub.subscribe(), ME, CancellationToken::new()).into_response();
    hub.deliver(Envelope {
        audience: Audience::User(2),
        event: Event::SantaHelp { event_id: 1 },
    });
    hub.deliver(Envelope {
        audience: Audience::User(1),
        event: Event::SantaHelp { event_id: 7 },
    });
    hub.deliver(Envelope {
        audience: Audience::All,
        event: Event::LedgerUpdated { entry_id: 3 },
    });
    hub.resync();

    let text = transcript(response, "event: resync").await;
    insta::assert_snapshot!(text, @r#"
    event: ready
    data: {}

    event: santa.help
    data: {"event_id":7}

    event: ledger.updated
    data: {"entry_id":3}

    event: resync
    data: {}
    "#);
}

#[tokio::test]
async fn a_lagging_stream_is_told_to_resync() {
    let hub = Hub::new(2);
    let response = sse::response(hub.subscribe(), ME, CancellationToken::new()).into_response();
    for poll_id in 0..10 {
        hub.deliver(Envelope {
            audience: Audience::All,
            event: Event::PollUpdated { poll_id },
        });
    }
    let text = transcript(response, "event: resync").await;
    assert!(text.contains("event: resync"), "{text}");
}

#[tokio::test]
async fn revoking_the_session_ends_its_stream() {
    let hub = Hub::new(16);
    let response = sse::response(hub.subscribe(), ME, CancellationToken::new()).into_response();
    // Another session of the same user is revoked: this stream continues.
    let other = Uuid::from_u128(2);
    hub.deliver(Envelope {
        audience: Audience::User(1),
        event: Event::SessionRevoked { session_id: other },
    });
    hub.deliver(Envelope {
        audience: Audience::User(1),
        event: Event::SessionRevoked {
            session_id: ME.session_id,
        },
    });
    hub.deliver(Envelope {
        audience: Audience::All,
        event: Event::PollCreated { poll_id: 1 },
    });

    // The body ends by itself after its own revocation; nothing after it is sent.
    let mut body = response.into_body().into_data_stream();
    let mut text = String::new();
    let ended = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(chunk) = body.next().await {
            text.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream did not end: {text}");
    assert_eq!(text.matches("event: session.revoked").count(), 2, "{text}");
    assert!(!text.contains("poll.created"), "{text}");
}

#[tokio::test]
async fn shutdown_ends_every_stream() {
    let hub = Hub::new(16);
    let shutdown = CancellationToken::new();
    let response = sse::response(hub.subscribe(), ME, shutdown.clone()).into_response();
    shutdown.cancel();
    let mut body = response.into_body().into_data_stream();
    let ended = tokio::time::timeout(Duration::from_secs(2), async {
        while body.next().await.is_some() {}
    })
    .await;
    assert!(ended.is_ok());
}
