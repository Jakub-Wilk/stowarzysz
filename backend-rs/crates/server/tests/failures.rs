//! A failure partway through a change leaves nothing behind: no event reaches anyone, no push is
//! queued, no file is lost or orphaned. Failures are injected with triggers that raise after the
//! service has already done its side effects (consumed an upload, queued a file for deletion,
//! published an event, queued a push) inside the transaction.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use app::db::Uow;
use app::events::{Audience, Event, HubMessage};
use axum::http::StatusCode;
use common::{Member, TestApp, png, subscribe};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Make every `when` on `table` fail from now on.
async fn break_table(app: &TestApp, table: &str, when: &str) {
    sqlx::raw_sql(&format!(
        "create or replace function injected_failure() returns trigger language plpgsql as
           $$ begin raise exception 'injected failure'; end $$;
         create trigger injected_failure before {when} on {table}
           for each row execute function injected_failure();"
    ))
    .execute(app.db())
    .await
    .unwrap();
}

async fn photo(app: &TestApp, who: &Member, purpose: &str) -> Value {
    app.upload(who, purpose, "image/png", png(64, 48))
        .await
        .expect(StatusCode::CREATED)
        .json()
}

/// The next event this process delivers. A sentinel committed after the failed request proves
/// that nothing from the failure was delivered before it (NOTIFY keeps commit order).
async fn first_event_after_sentinel(
    app: &TestApp,
    rx: &mut tokio::sync::broadcast::Receiver<HubMessage>,
) -> Event {
    let mut uow = Uow::begin(app.db()).await.unwrap();
    uow.publish(Audience::All, Event::SantaUpdated { event_id: -1 })
        .await
        .unwrap();
    uow.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(HubMessage::Event(envelope)) = rx.recv().await {
                return envelope.event.clone();
            }
        }
    })
    .await
    .unwrap()
}

fn files(app: &TestApp, key: &str) -> usize {
    std::fs::read_dir(app.ctx.media.dir(key)).map_or(0, Iterator::count)
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_failed_attachment_keeps_the_upload_and_tells_nobody(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola) = (app.member("ala").await, app.member("ola").await);
    subscribe(app.db(), &[ala.id, ola.id]).await;
    let pact = app
        .post_as(
            &ala,
            "/api/pacts",
            json!({"kind": "bet", "title": "t", "condition": "c",
                   "opponents": [{"user_id": ola.id, "stake_amount": 100}]}),
        )
        .await
        .expect(StatusCode::CREATED)
        .json()["id"]
        .as_i64()
        .unwrap();
    sqlx::query("delete from push_outbox")
        .execute(app.db())
        .await
        .unwrap();
    let upload = photo(&app, &ala, "photo").await;
    let key: String = sqlx::query_scalar("select key from uploads")
        .fetch_one(app.db())
        .await
        .unwrap();
    let _listener = app.start_listener().await;
    let mut rx = app.ctx.hub.subscribe();
    break_table(&app, "pact_attachments", "insert").await;

    let response = app
        .post_as(
            &ala,
            &format!("/api/pacts/{pact}/attachments"),
            json!({"upload_id": upload["id"]}),
        )
        .await;
    response.expect(StatusCode::INTERNAL_SERVER_ERROR);

    assert_eq!(
        app.count("select count(*) from uploads").await,
        1,
        "the upload was not consumed"
    );
    assert_eq!(files(&app, &key), 3, "its files are where they were");
    assert_eq!(app.count("select count(*) from media_trash").await, 0);
    assert_eq!(app.count("select count(*) from push_outbox").await, 0);
    assert_eq!(
        first_event_after_sentinel(&app, &mut rx).await,
        Event::SantaUpdated { event_id: -1 },
        "no pact.updated escaped the rolled-back transaction"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_failed_removal_keeps_the_picture(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let pact = app
        .post_as(
            &ala,
            "/api/pacts",
            json!({"kind": "bet", "title": "t", "condition": "c"}),
        )
        .await
        .json()["id"]
        .as_i64()
        .unwrap();
    let upload = photo(&app, &ala, "photo").await;
    let attached = app
        .post_as(
            &ala,
            &format!("/api/pacts/{pact}/attachments"),
            json!({"upload_id": upload["id"]}),
        )
        .await
        .expect(StatusCode::CREATED)
        .json();
    let attachment = attached["attachments"][0]["id"].as_i64().unwrap();
    let key: String = sqlx::query_scalar("select image_key from pact_attachments")
        .fetch_one(app.db())
        .await
        .unwrap();
    break_table(&app, "pact_attachments", "delete").await;

    app.delete_as(&ala, &format!("/api/pacts/{pact}/attachments/{attachment}"))
        .await
        .expect(StatusCode::INTERNAL_SERVER_ERROR);

    assert_eq!(app.count("select count(*) from pact_attachments").await, 1);
    assert_eq!(
        app.count("select count(*) from media_trash").await,
        0,
        "the file was not queued for deletion"
    );
    assert_eq!(files(&app, &key), 3);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_failed_avatar_change_keeps_the_old_picture(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("szef").await;
    let ala = app.member("ala").await;
    let first = photo(&app, &admin, "avatar").await;
    app.put_as(
        &admin,
        &format!("/api/users/{}/avatar", ala.id),
        json!({"upload_id": first["id"]}),
    )
    .await
    .expect(StatusCode::OK);
    let old: String = sqlx::query_scalar("select avatar_key from users where id = $1")
        .bind(ala.id)
        .fetch_one(app.db())
        .await
        .unwrap();
    let second = photo(&app, &admin, "avatar").await;
    break_table(&app, "users", "update").await;

    let response = app
        .put_as(
            &admin,
            &format!("/api/users/{}/avatar", ala.id),
            json!({"upload_id": second["id"]}),
        )
        .await;
    response.expect(StatusCode::INTERNAL_SERVER_ERROR);

    let still: String = sqlx::query_scalar("select avatar_key from users where id = $1")
        .bind(ala.id)
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(still, old);
    assert_eq!(files(&app, &old), 3, "the old picture was not deleted");
    assert_eq!(app.count("select count(*) from media_trash").await, 0);
    assert_eq!(
        app.count("select count(*) from uploads").await,
        1,
        "the new upload is still unclaimed (the sweeper removes it when it expires)"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_failed_ledger_entry_leaves_no_trace(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola) = (app.member("ala").await, app.member("ola").await);
    subscribe(app.db(), &[ala.id, ola.id]).await;
    let _listener = app.start_listener().await;
    let mut rx = app.ctx.hub.subscribe();
    // The entry row, its shares and the event are written first; the obligations fail.
    break_table(&app, "ledger_obligations", "insert").await;

    let response = app
        .post_as(
            &ala,
            "/api/ledger/entries",
            json!({"kind": "expense", "title": "Pizza", "category": "food",
                   "items": [{"name": "Pizza", "amount": 3000, "shares": [{"user_id": ala.id}, {"user_id": ola.id}]}],
                   "payers": [{"user_id": ala.id, "amount": 3000}]}),
        )
        .await;
    response.expect(StatusCode::INTERNAL_SERVER_ERROR);

    for table in ["ledger_entries", "ledger_entry_shares", "push_outbox"] {
        assert_eq!(
            app.count(&format!("select count(*) from {table}")).await,
            0,
            "{table}"
        );
    }
    assert_eq!(
        first_event_after_sentinel(&app, &mut rx).await,
        Event::SantaUpdated { event_id: -1 }
    );
}
