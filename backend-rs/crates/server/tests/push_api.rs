//! The Web Push subscription endpoints.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::{Value, json};
use sqlx::PgPool;

/// A subscription as a browser hands it out (keys from RFC 8291's example).
fn subscription(endpoint: &str) -> Value {
    json!({
        "endpoint": endpoint,
        "keys": {
            "p256dh": "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
            "auth": "BTBZMqHH6r4Tts7J_aSIgg"
        }
    })
}

async fn owners(app: &TestApp) -> Vec<(String, i64)> {
    sqlx::query_as("select endpoint, user_id from push_subscriptions order by endpoint")
        .fetch_all(app.db())
        .await
        .unwrap()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn endpoints_require_a_session(pool: PgPool) {
    let app = TestApp::new(pool);
    app.get("/api/push/public-key")
        .await
        .expect(StatusCode::UNAUTHORIZED);
    app.call(
        Method::PUT,
        "/api/push/subscription",
        None,
        Some(subscription("https://push.test/a")),
    )
    .await
    .expect(StatusCode::UNAUTHORIZED);
    app.call(
        Method::DELETE,
        "/api/push/subscription",
        None,
        Some(json!({"endpoint": "x"})),
    )
    .await
    .expect(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_public_key_is_null_until_configured(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ala = app.member("ala").await;
    assert_eq!(
        app.get_as(&ala, "/api/push/public-key").await.json(),
        json!({"public_key": null})
    );

    let public =
        "BOiOUte3VhqJOjcXfKxUCRT0nATViFckskhpJzhPC8OOgAiiMvQa9cHt1awa7_eWJuNDimhiGqrg_VJZnIW1Wvg";
    let configured = TestApp::with_env(
        pool,
        &[
            ("VAPID_PUBLIC_KEY", public),
            (
                "VAPID_PRIVATE_KEY",
                "rp_3tg50IMtLFfwWrI1aYaM1tN7E5roukpEP8HrQ5yQ",
            ),
        ],
    );
    let ola = configured.member("ola").await;
    assert_eq!(
        configured.get_as(&ola, "/api/push/public-key").await.json(),
        json!({"public_key": public})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn subscribing_upserts_by_endpoint_and_follows_the_last_user(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let ola = app.member("ola").await;
    for _ in 0..2 {
        app.put_as(
            &ala,
            "/api/push/subscription",
            subscription("https://push.test/phone"),
        )
        .await
        .expect(StatusCode::NO_CONTENT);
    }
    assert_eq!(
        owners(&app).await,
        [("https://push.test/phone".to_owned(), ala.id)]
    );

    // The same browser, after ola signs in on it.
    app.put_as(
        &ola,
        "/api/push/subscription",
        subscription("https://push.test/phone"),
    )
    .await
    .expect(StatusCode::NO_CONTENT);
    assert_eq!(
        owners(&app).await,
        [("https://push.test/phone".to_owned(), ola.id)]
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn subscriptions_are_validated(pool: PgPool) {
    let app = TestApp::with_env(pool, &[("APP_ENV", "dev")]);
    let ala = app.member("ala").await;
    let mut bad = subscription("ftp://push.test/x");
    bad["keys"]["auth"] = json!("short");
    let response = app.put_as(&ala, "/api/push/subscription", bad).await;
    response.expect(StatusCode::BAD_REQUEST);
    let errors = response.json()["errors"].clone();
    assert!(
        errors["endpoint"].is_array() && errors["keys.auth"].is_array(),
        "{errors}"
    );
    assert!(errors.get("keys.p256dh").is_none());

    let missing = app
        .put_as(
            &ala,
            "/api/push/subscription",
            json!({"endpoint": "https://push.test/x"}),
        )
        .await;
    missing.expect(StatusCode::BAD_REQUEST);
    assert_eq!(missing.code(), "invalid_body");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn unsubscribing_removes_only_your_own(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let ola = app.member("ola").await;
    app.put_as(
        &ala,
        "/api/push/subscription",
        subscription("https://push.test/ala"),
    )
    .await
    .expect(StatusCode::NO_CONTENT);
    app.call(
        Method::DELETE,
        "/api/push/subscription",
        Some(&ola.token),
        Some(json!({"endpoint": "https://push.test/ala"})),
    )
    .await
    .expect(StatusCode::NO_CONTENT);
    assert_eq!(owners(&app).await.len(), 1, "ola cannot remove ala's");
    app.call(
        Method::DELETE,
        "/api/push/subscription",
        Some(&ala.token),
        Some(json!({"endpoint": "https://push.test/ala"})),
    )
    .await
    .expect(StatusCode::NO_CONTENT);
    assert_eq!(owners(&app).await, Vec::<(String, i64)>::new());
}
