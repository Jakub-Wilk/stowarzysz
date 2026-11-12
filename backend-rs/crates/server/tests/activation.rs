//! Admin-created accounts and their one-time activation links.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use chrono::TimeDelta;
use common::{Member, TestApp};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn create(app: &TestApp, admin: &Member, username: &str) -> i64 {
    let response = app
        .post_as(admin, "/api/users", json!({"username": username}))
        .await;
    response.expect(StatusCode::CREATED);
    response.json()["id"].as_i64().unwrap()
}

async fn link(app: &TestApp, admin: &Member, id: i64) -> Value {
    let response = app
        .call(
            Method::POST,
            &format!("/api/users/{id}/activation-link"),
            Some(&admin.token),
            None,
        )
        .await;
    response.expect(StatusCode::CREATED);
    response.json()
}

async fn public(app: &TestApp, path: &str, body: Value) -> common::TestResponse {
    app.call(Method::POST, path, None, Some(body)).await
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn creating_accounts_is_for_administrators(pool: PgPool) {
    let app = TestApp::new(pool);
    app.call(
        Method::POST,
        "/api/users",
        None,
        Some(json!({"username": "x"})),
    )
    .await
    .expect(StatusCode::UNAUTHORIZED);
    let member = app.member("ala").await;
    let response = app
        .post_as(&member, "/api/users", json!({"username": "x"}))
        .await;
    response.expect(StatusCode::FORBIDDEN);
    assert_eq!(response.code(), "admin_only");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn new_accounts_have_no_password(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let response = app
        .post_as(&admin, "/api/users", json!({"username": "nowy"}))
        .await;
    response.expect(StatusCode::CREATED);
    let user = response.json();
    assert_eq!(
        (user["has_password"].clone(), user["is_active"].clone()),
        (json!(false), json!(true))
    );
    // Not a member until activated.
    let people = app.get_as(&admin, "/api/people").await.json();
    assert!(
        people
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["username"] != "nowy")
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn activation_links_are_for_administrators_and_known_users(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let member = app.member("ala").await;
    let id = create(&app, &admin, "nowy").await;
    let path = format!("/api/users/{id}/activation-link");
    app.call(Method::POST, &path, None, None)
        .await
        .expect(StatusCode::UNAUTHORIZED);
    app.call(Method::POST, &path, Some(&member.token), None)
        .await
        .expect(StatusCode::FORBIDDEN);
    app.call(
        Method::POST,
        "/api/users/999999/activation-link",
        Some(&admin.token),
        None,
    )
    .await
    .expect(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_link_points_to_the_frontend(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let id = create(&app, &admin, "nowy").await;
    let link = link(&app, &admin, id).await;
    let token = link["token"].as_str().unwrap();
    assert_eq!(
        link["url"],
        format!("http://localhost:5173/activate/{token}")
    );
    let expires =
        chrono::DateTime::parse_from_rfc3339(link["expires_at"].as_str().unwrap()).unwrap();
    assert_eq!(expires.to_utc(), app.now() + TimeDelta::hours(72));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn validate_names_the_account_and_complete_sets_its_password(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let id = create(&app, &admin, "nowy").await;
    let token = link(&app, &admin, id).await["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let valid = public(&app, "/api/activation/validate", json!({"token": token})).await;
    assert_eq!(
        valid.expect(StatusCode::OK).json(),
        json!({"username": "nowy"})
    );

    let done = public(
        &app,
        "/api/activation/complete",
        json!({"token": token, "password": "Zażółć gęślą 9"}),
    )
    .await;
    done.expect(StatusCode::NO_CONTENT);
    assert!(
        done.body.is_empty(),
        "no tokens: the person signs in themselves"
    );
    let login = public(
        &app,
        "/api/auth/login",
        json!({"username": "nowy", "password": "Zażółć gęślą 9"}),
    )
    .await;
    login.expect(StatusCode::OK);

    // Used up.
    let reused = public(
        &app,
        "/api/activation/complete",
        json!({"token": token, "password": "Inne hasło 123"}),
    )
    .await;
    reused.expect(StatusCode::BAD_REQUEST);
    assert_eq!(reused.code(), "invalid_activation_token");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_new_link_invalidates_the_previous_one(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let id = create(&app, &admin, "nowy").await;
    let old = link(&app, &admin, id).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let new = link(&app, &admin, id).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    public(&app, "/api/activation/validate", json!({"token": old}))
        .await
        .expect(StatusCode::BAD_REQUEST);
    public(&app, "/api/activation/validate", json!({"token": new}))
        .await
        .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn expired_unknown_and_inactive_all_look_the_same(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let expiring = create(&app, &admin, "a").await;
    let expiring = link(&app, &admin, expiring).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let inactive = create(&app, &admin, "b").await;
    let inactive_token = link(&app, &admin, inactive).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    app.patch_as(
        &admin,
        &format!("/api/users/{inactive}"),
        json!({"is_active": false}),
    )
    .await
    .expect(StatusCode::OK);
    app.clock.advance(TimeDelta::hours(73));

    let mut bodies = Vec::new();
    for token in [expiring.as_str(), inactive_token.as_str(), "made-up"] {
        let response = public(
            &app,
            "/api/activation/complete",
            json!({"token": token, "password": "Zażółć gęślą 9"}),
        )
        .await;
        response.expect(StatusCode::BAD_REQUEST);
        bodies.push(response.json());
    }
    assert!(
        bodies.windows(2).all(|pair| pair[0] == pair[1]),
        "{bodies:?}"
    );
    assert_eq!(
        bodies[0]["errors"]["token"],
        json!(["Ten link aktywacyjny jest nieprawidłowy lub wygasł."])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn weak_passwords_are_refused_with_every_reason(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let id = create(&app, &admin, "kowalski").await;
    let token = link(&app, &admin, id).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = public(
        &app,
        "/api/activation/complete",
        json!({"token": token, "password": "1234"}),
    )
    .await;
    response.expect(StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json()["errors"]["password"],
        json!([
            "To hasło jest za krótkie. Musi zawierać co najmniej 8 znaków.",
            "To hasło jest zbyt powszechne.",
            "Hasło składa się wyłącznie z cyfr."
        ])
    );
    // The link was not used up by the failed attempt.
    public(&app, "/api/activation/validate", json!({"token": token}))
        .await
        .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn no_link_for_an_inactive_account(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let id = create(&app, &admin, "nowy").await;
    app.patch_as(
        &admin,
        &format!("/api/users/{id}"),
        json!({"is_active": false}),
    )
    .await
    .expect(StatusCode::OK);
    let response = app
        .call(
            Method::POST,
            &format!("/api/users/{id}/activation-link"),
            Some(&admin.token),
            None,
        )
        .await;
    response.expect(StatusCode::BAD_REQUEST);
    assert_eq!(response.code(), "user_inactive");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn activation_endpoints_are_rate_limited(pool: PgPool) {
    let app = TestApp::with_env(pool, &[("ACTIVATION_RATE", "2/min")]);
    for _ in 0..2 {
        public(&app, "/api/activation/validate", json!({"token": "x"}))
            .await
            .expect(StatusCode::BAD_REQUEST);
    }
    public(
        &app,
        "/api/activation/complete",
        json!({"token": "x", "password": "y"}),
    )
    .await
    .expect(StatusCode::TOO_MANY_REQUESTS);
}
