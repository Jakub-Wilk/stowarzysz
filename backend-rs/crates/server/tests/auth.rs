//! Signing in and out, sessions and refresh tokens, the event stream's authentication.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use chrono::TimeDelta;
use common::{PASSWORD, TestApp};
use futures_util::StreamExt as _;
use serde_json::json;
use sqlx::PgPool;

async fn login(app: &TestApp, username: &str, password: &str) -> common::TestResponse {
    app.call(
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"username": username, "password": password})),
    )
    .await
}

async fn refresh(app: &TestApp, token: &str) -> common::TestResponse {
    app.call(
        Method::POST,
        "/api/auth/refresh",
        None,
        Some(json!({"refresh": token})),
    )
    .await
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn health_is_public(pool: PgPool) {
    TestApp::new(pool)
        .get("/api/health")
        .await
        .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn login_returns_tokens_and_the_user(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let response = login(&app, "ala", PASSWORD).await;
    response.expect(StatusCode::OK);
    let body = response.json();
    assert_eq!(body["access_expires_in"], 900);
    assert!(body["refresh"].as_str().unwrap().starts_with("s1_"));
    assert_eq!(
        body["user"],
        json!({"id": ala.id, "username": "ala", "is_superuser": false, "avatar": null, "birthday": null})
    );

    let token = body["access"].as_str().unwrap();
    app.call(Method::GET, "/api/me", Some(token), None)
        .await
        .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn login_failures_look_the_same(pool: PgPool) {
    let app = TestApp::new(pool);
    app.member("ala").await;
    let inactive = app.member("ola").await;
    sqlx::query("update users set is_active = false where id = $1")
        .bind(inactive.id)
        .execute(app.db())
        .await
        .unwrap();
    sqlx::query("insert into users (username) values ('nowy')")
        .execute(app.db())
        .await
        .unwrap(); // not activated

    for (username, password) in [
        ("ala", "wrong"),
        ("nobody", PASSWORD),
        ("ola", PASSWORD),
        ("nowy", PASSWORD),
        ("nowy", ""),
    ] {
        let response = login(&app, username, password).await;
        response.expect(StatusCode::UNAUTHORIZED);
        assert_eq!(response.code(), "invalid_credentials", "{username}");
        assert_eq!(
            response.json()["detail"],
            "Nieprawidłowa nazwa użytkownika lub hasło."
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn login_normalises_the_username(pool: PgPool) {
    let app = TestApp::new(pool);
    app.member("ala").await;
    login(&app, "  ala ", PASSWORD).await.expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn login_is_rate_limited(pool: PgPool) {
    let app = TestApp::with_env(pool, &[("LOGIN_RATE", "3/min")]);
    app.member("ala").await;
    for _ in 0..3 {
        login(&app, "ala", "wrong")
            .await
            .expect(StatusCode::UNAUTHORIZED);
    }
    let limited = login(&app, "ala", PASSWORD).await;
    limited.expect(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.code(), "rate_limited");
    assert_ne!(limited.header("retry-after"), "");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn me_requires_a_valid_token(pool: PgPool) {
    let app = TestApp::new(pool);
    let missing = app.get("/api/me").await;
    missing.expect(StatusCode::UNAUTHORIZED);
    assert_eq!(missing.code(), "not_authenticated");
    assert_eq!(missing.header("www-authenticate"), "Bearer");

    let garbage = app
        .call(Method::GET, "/api/me", Some("not-a-jwt"), None)
        .await;
    garbage.expect(StatusCode::UNAUTHORIZED);
    assert_eq!(garbage.code(), "invalid_token");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn access_tokens_expire_after_15_minutes(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    app.clock.advance(TimeDelta::minutes(14));
    app.get_as(&ala, "/api/me").await.expect(StatusCode::OK);
    app.clock.advance(TimeDelta::minutes(2));
    app.get_as(&ala, "/api/me")
        .await
        .expect(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn refresh_rotates_and_the_previous_token_survives_until_the_new_one_is_used(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;

    let first = refresh(&app, &ala.refresh).await;
    first.expect(StatusCode::OK);
    let second_token = first.json()["refresh"].as_str().unwrap().to_owned();
    assert_ne!(second_token, ala.refresh);

    // The client lost the response (a PWA killed mid-request): the old token still works…
    let retry = refresh(&app, &ala.refresh).await;
    retry.expect(StatusCode::OK);
    let third_token = retry.json()["refresh"].as_str().unwrap().to_owned();
    // …and the token it never stored is replaced by the new one.
    refresh(&app, &second_token)
        .await
        .expect(StatusCode::UNAUTHORIZED);

    // Once the client uses the newest token, the old one retires.
    let used = refresh(&app, &third_token).await;
    used.expect(StatusCode::OK);
    let rejected = refresh(&app, &ala.refresh).await;
    rejected.expect(StatusCode::UNAUTHORIZED);
    assert_eq!(rejected.code(), "session_expired");
    // The token just issued keeps working.
    refresh(&app, used.json()["refresh"].as_str().unwrap())
        .await
        .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn sessions_slide_for_400_days(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    app.clock.advance(TimeDelta::days(399));
    let renewed = refresh(&app, &ala.refresh).await;
    renewed.expect(StatusCode::OK);
    // Used again within 400 days of the last use: still fine, far beyond 400 days in total.
    app.clock.advance(TimeDelta::days(399));
    refresh(&app, renewed.json()["refresh"].as_str().unwrap())
        .await
        .expect(StatusCode::OK);

    let ola = app.member("ola").await;
    app.clock.advance(TimeDelta::days(401));
    refresh(&app, &ola.refresh)
        .await
        .expect(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn logout_ends_only_this_session(pool: PgPool) {
    let app = TestApp::new(pool);
    let phone = app.member("ala").await;
    let laptop = app.session(phone.id).await;

    app.call(Method::POST, "/api/auth/logout", Some(&phone.token), None)
        .await
        .expect(StatusCode::NO_CONTENT);
    app.get_as(&phone, "/api/me")
        .await
        .expect(StatusCode::UNAUTHORIZED);
    refresh(&app, &phone.refresh)
        .await
        .expect(StatusCode::UNAUTHORIZED);

    app.call(Method::GET, "/api/me", Some(&laptop.access), None)
        .await
        .expect(StatusCode::OK);
    refresh(&app, &laptop.refresh).await.expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_django_password_works_once_and_is_upgraded(pool: PgPool) {
    let app = TestApp::new(pool);
    // Made by Django 6: make_password("s3cret-pass-123", salt="fixedsaltfixedsalt12").
    let django =
        "pbkdf2_sha256$1500000$fixedsaltfixedsalt12$Ko7BwSPmIe4CpYnNmje8vTQd0RyY7kihGbAZ0kUpvM8=";
    sqlx::query("insert into users (username, password_hash) values ('ala', $1)")
        .bind(django)
        .execute(app.db())
        .await
        .unwrap();

    login(&app, "ala", "wrong")
        .await
        .expect(StatusCode::UNAUTHORIZED);
    login(&app, "ala", PASSWORD).await.expect(StatusCode::OK);
    let stored: String = sqlx::query_scalar("select password_hash from users")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert!(stored.starts_with("$argon2id$"), "{stored}");
    login(&app, "ala", PASSWORD).await.expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_django_refresh_token_is_exchanged_once(pool: PgPool) {
    #[derive(serde::Serialize)]
    struct Claims<'a> {
        token_type: &'a str,
        exp: i64,
        iat: i64,
        jti: &'a str,
        user_id: String,
    }
    let app = TestApp::with_env(pool, &[("LEGACY_SECRET_KEY", "the-old-django-secret")]);
    let ala = app.member("ala").await;
    let exp = (app.now() + TimeDelta::days(100)).timestamp();
    sqlx::query("insert into legacy_refresh_tokens (jti, user_id, expires_at) values ('abc', $1, now() + interval '1 year')")
        .bind(ala.id)
        .execute(app.db())
        .await
        .unwrap();
    let sign = |secret: &[u8], jti| {
        let claims = Claims {
            token_type: "refresh",
            exp,
            iat: 0,
            jti,
            user_id: ala.id.to_string(),
        };
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        )
        .unwrap()
    };

    refresh(&app, &sign(b"another-secret", "abc"))
        .await
        .expect(StatusCode::UNAUTHORIZED);
    refresh(&app, &sign(b"the-old-django-secret", "unknown"))
        .await
        .expect(StatusCode::UNAUTHORIZED);
    let legacy = sign(b"the-old-django-secret", "abc");
    let exchanged = refresh(&app, &legacy).await;
    exchanged.expect(StatusCode::OK);
    let access = exchanged.json()["access"].as_str().unwrap().to_owned();
    let me = app.call(Method::GET, "/api/me", Some(&access), None).await;
    assert_eq!(me.json()["username"], "ala");
    refresh(&app, &legacy)
        .await
        .expect(StatusCode::UNAUTHORIZED); // only once
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn login_users_is_public_minimal_and_lists_members_only(pool: PgPool) {
    let app = TestApp::new(pool);
    app.member("ola").await;
    app.member("ala").await;
    let gone = app.member("ewa").await;
    sqlx::query("update users set is_active = false where id = $1")
        .bind(gone.id)
        .execute(app.db())
        .await
        .unwrap();
    sqlx::query("insert into users (username) values ('nowy')")
        .execute(app.db())
        .await
        .unwrap();

    let response = app.get("/api/login-users").await;
    response.expect(StatusCode::OK);
    assert_eq!(
        response.json(),
        json!([{"username": "ala", "avatar": null}, {"username": "ola", "avatar": null}])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn people_lists_members_for_members(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let ola = app.member("ola").await;
    sqlx::query("insert into users (username) values ('nowy')")
        .execute(app.db())
        .await
        .unwrap();
    app.get("/api/people")
        .await
        .expect(StatusCode::UNAUTHORIZED);
    let people = app.get_as(&ala, "/api/people").await.json();
    assert_eq!(
        people,
        json!([{"id": ala.id, "username": "ala", "avatar": null}, {"id": ola.id, "username": "ola", "avatar": null}])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_event_stream_requires_a_session_and_starts_ready(pool: PgPool) {
    let app = TestApp::new(pool);
    app.get("/api/events")
        .await
        .expect(StatusCode::UNAUTHORIZED);

    let ala = app.member("ala").await;
    let request = axum::http::Request::get("/api/events")
        .header("authorization", format!("Bearer {}", ala.token))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(
        server::http::router(server::AppState::new(app.ctx.clone())),
        request,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    let mut body = response.into_body().into_data_stream();
    let first = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&first),
        "event: ready\ndata: {}\n\n"
    );
}
