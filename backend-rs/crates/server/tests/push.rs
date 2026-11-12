//! Push outbox: queued in the caller's transaction, delivered with retries after commit.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use app::db::Uow;
use app::push::testing::{Delivery, RecordingTransport};
use app::push::{Notification, Outcome, PushWorker};
use common::{TestApp, insert_user};
use sqlx::PgPool;

fn note(title: &str) -> Notification {
    Notification {
        title: title.into(),
        body: "Treść".into(),
        url: "/voting/1".into(),
    }
}

async fn subscribe(db: &PgPool, user_id: i64, endpoint: &str) -> i64 {
    sqlx::query_scalar(
        "insert into push_subscriptions (user_id, endpoint, p256dh, auth) values ($1, $2, 'k', 'a')
         returning id",
    )
    .bind(user_id)
    .bind(endpoint)
    .fetch_one(db)
    .await
    .unwrap()
}

async fn count(db: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(db).await.unwrap()
}

async fn queue(app: &TestApp, users: &[i64], title: &str) -> u64 {
    let mut uow = Uow::begin(app.db()).await.unwrap();
    let queued = app
        .ctx
        .push
        .notify(&mut uow, users, &note(title))
        .await
        .unwrap();
    uow.commit().await.unwrap();
    queued
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn notifications_are_queued_per_subscription_of_the_recipients(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola, ewa) = (
        insert_user(app.db(), "ala").await,
        insert_user(app.db(), "ola").await,
        insert_user(app.db(), "ewa").await,
    );
    subscribe(app.db(), ala, "https://push.test/ala-phone").await;
    subscribe(app.db(), ala, "https://push.test/ala-laptop").await;
    subscribe(app.db(), ewa, "https://push.test/ewa").await;

    assert_eq!(queue(&app, &[ala, ola], "Nowe głosowanie").await, 2);
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 2);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn nothing_is_queued_when_rolled_back_or_push_is_off(pool: PgPool) {
    let app = TestApp::with_push(pool.clone());
    let ala = insert_user(app.db(), "ala").await;
    subscribe(app.db(), ala, "https://push.test/ala").await;

    let mut uow = Uow::begin(app.db()).await.unwrap();
    app.ctx
        .push
        .notify(&mut uow, &[ala], &note("x"))
        .await
        .unwrap();
    drop(uow);
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 0);

    let disabled = TestApp::new(pool);
    assert_eq!(queue(&disabled, &[ala], "x").await, 0);
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_worker_delivers_and_clears_the_outbox(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let ala = insert_user(app.db(), "ala").await;
    subscribe(app.db(), ala, "https://push.test/ala").await;
    queue(&app, &[ala], "Głosowanie zakończone").await;

    let transport = Arc::new(RecordingTransport::default());
    let next = PushWorker::new(app.db().clone(), transport.clone())
        .drain()
        .await
        .unwrap();

    assert_eq!(next, None, "nothing left to do");
    assert_eq!(
        transport.delivered(),
        [Delivery {
            endpoint: "https://push.test/ala".into(),
            notification: note("Głosowanie zakończone")
        }]
    );
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 0);
    assert_eq!(
        count(
            app.db(),
            "select count(*) from push_subscriptions where last_success_at is not null"
        )
        .await,
        1
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn gone_subscriptions_are_forgotten(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let ala = insert_user(app.db(), "ala").await;
    subscribe(app.db(), ala, "https://push.test/old").await;
    subscribe(app.db(), ala, "https://push.test/new").await;
    queue(&app, &[ala], "a").await;
    queue(&app, &[ala], "b").await;

    let transport = Arc::new(RecordingTransport::default());
    // One batch claims both queued rows of the old endpoint; each attempt reports it gone.
    transport.script("https://push.test/old", [Outcome::Gone, Outcome::Gone]);
    PushWorker::new(app.db().clone(), transport.clone())
        .drain()
        .await
        .unwrap();

    // The old subscription is forgotten; the new one got both notifications.
    let endpoints: Vec<String> = transport
        .delivered()
        .into_iter()
        .map(|d| d.endpoint)
        .collect();
    assert_eq!(
        endpoints,
        ["https://push.test/new", "https://push.test/new"]
    );
    assert_eq!(
        count(app.db(), "select count(*) from push_subscriptions").await,
        1
    );
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn transient_failures_are_retried_later(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let ala = insert_user(app.db(), "ala").await;
    subscribe(app.db(), ala, "https://push.test/ala").await;
    queue(&app, &[ala], "a").await;

    let transport = Arc::new(RecordingTransport::default());
    transport.script(
        "https://push.test/ala",
        [Outcome::Retry(Some(Duration::from_secs(120)))],
    );
    let worker = PushWorker::new(app.db().clone(), transport.clone());
    let next = worker.drain().await.unwrap().expect("a retry is scheduled");

    let wait = next - chrono::Utc::now();
    assert!(
        wait > chrono::TimeDelta::seconds(100),
        "Retry-After is honoured: {wait}"
    );
    assert_eq!(transport.delivered(), []);
    assert_eq!(
        count(app.db(), "select attempts::bigint from push_outbox").await,
        1
    );

    // Once due again it goes through.
    sqlx::query("update push_outbox set next_attempt_at = now()")
        .execute(app.db())
        .await
        .unwrap();
    worker.drain().await.unwrap();
    assert_eq!(transport.delivered().len(), 1);
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn rejected_and_expired_notifications_are_dropped(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let ala = insert_user(app.db(), "ala").await;
    subscribe(app.db(), ala, "https://push.test/ala").await;
    queue(&app, &[ala], "rejected").await;
    queue(&app, &[ala], "expired").await;
    sqlx::query(
        "update push_outbox set expires_at = now() - interval '1 second'
                 where payload->>'title' = 'expired'",
    )
    .execute(app.db())
    .await
    .unwrap();

    let transport = Arc::new(RecordingTransport::default());
    transport.script(
        "https://push.test/ala",
        [Outcome::Rejected("HTTP 413".into())],
    );
    PushWorker::new(app.db().clone(), transport.clone())
        .drain()
        .await
        .unwrap();

    assert_eq!(transport.delivered(), []);
    assert_eq!(count(app.db(), "select count(*) from push_outbox").await, 0);
    assert_eq!(
        count(
            app.db(),
            "select failure_count::bigint from push_subscriptions"
        )
        .await,
        1
    );
}
