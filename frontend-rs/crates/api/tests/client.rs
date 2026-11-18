//! The session state machine against a mock backend.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use api::types::Me;
use api::{ApiError, Client, LockGuard, TokenStore};
use futures_util::future::{LocalBoxFuture, join};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct MemoryStore {
    refresh: RefCell<Option<String>>,
}

impl TokenStore for MemoryStore {
    fn load(&self) -> Option<String> {
        self.refresh.borrow().clone()
    }

    fn save(&self, refresh: Option<&str>) {
        *self.refresh.borrow_mut() = refresh.map(str::to_owned);
    }

    fn lock(&self) -> LocalBoxFuture<'_, LockGuard> {
        Box::pin(async { LockGuard::none() })
    }
}

fn me() -> serde_json::Value {
    json!({"id": 1, "username": "ania", "is_superuser": false})
}

fn pair(access: &str, refresh: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({"access": access, "access_expires_in": 900, "refresh": refresh}))
}

fn problem(status: u16, code: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .insert_header("content-type", "application/problem+json")
        .set_body_json(json!({
            "type": format!("urn:stowarzysz:problem:{code}"), "title": "x", "status": status,
            "detail": "Sesja wygasła.", "code": code,
        }))
}

async fn setup(stored: Option<&str>) -> (MockServer, Rc<MemoryStore>, Client) {
    let server = MockServer::start().await;
    let store = Rc::new(MemoryStore::default());
    store.save(stored);
    let client = Client::new(&server.uri(), store.clone(), "test");
    (server, store, client)
}

#[tokio::test]
async fn login_keeps_the_tokens() {
    let (server, store, client) = setup(None).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .and(body_json(json!({"username": "ania", "password": "tajne"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access": "a1", "access_expires_in": 900, "refresh": "r1", "user": me()}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/me"))
        .and(header("authorization", "Bearer a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me()))
        .expect(1)
        .mount(&server)
        .await;

    let user = client.login("ania", "tajne").await.expect("login");
    assert_eq!(user.username, "ania");
    assert_eq!(store.load().as_deref(), Some("r1"));
    let again: Me = client.get("/api/me").await.expect("me");
    assert_eq!(again.id, 1);
}

#[tokio::test]
async fn wrong_password_is_a_problem() {
    let (server, store, client) = setup(None).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .respond_with(problem(401, "invalid_credentials"))
        .mount(&server)
        .await;
    let error = client.login("ania", "zle").await.expect_err("rejected");
    assert_eq!(error.code(), Some("invalid_credentials"));
    assert_eq!(store.load(), None);
}

#[tokio::test]
async fn a_stored_session_refreshes_before_the_first_request() {
    let (server, store, client) = setup(Some("r0")).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .and(body_json(json!({"refresh": "r0"})))
        .respond_with(pair("a1", "r1"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/me"))
        .and(header("authorization", "Bearer a1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me()))
        .mount(&server)
        .await;

    let _: Me = client.get("/api/me").await.expect("me");
    assert_eq!(store.load().as_deref(), Some("r1"));
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh() {
    let (server, _store, client) = setup(Some("r0")).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .respond_with(pair("a1", "r1"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me()))
        .expect(2)
        .mount(&server)
        .await;

    let (a, b) = join(client.get::<Me>("/api/me"), client.get::<Me>("/api/me")).await;
    a.expect("first");
    b.expect("second");
}

#[tokio::test]
async fn an_expired_access_token_is_refreshed_and_retried() {
    let (server, store, client) = setup(Some("r0")).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .and(body_json(json!({"refresh": "r0"})))
        .respond_with(pair("a1", "r1"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .and(body_json(json!({"refresh": "r1"})))
        .respond_with(pair("a2", "r2"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/me"))
        .and(header("authorization", "Bearer a1"))
        .respond_with(problem(401, "invalid_token"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/me"))
        .and(header("authorization", "Bearer a2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me()))
        .mount(&server)
        .await;

    let _: Me = client.get("/api/me").await.expect("me after retry");
    assert_eq!(store.load().as_deref(), Some("r2"));
}

#[tokio::test]
async fn a_rejected_refresh_signs_out() {
    let (server, store, client) = setup(Some("r0")).await;
    let signed_out = Rc::new(Cell::new(0));
    let counter = signed_out.clone();
    client.on_signed_out(move || counter.set(counter.get() + 1));
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .respond_with(problem(401, "session_expired"))
        .mount(&server)
        .await;

    let error = client.get::<Me>("/api/me").await.expect_err("signed out");
    assert_eq!(error, ApiError::SignedOut);
    assert_eq!(store.load(), None);
    assert_eq!(signed_out.get(), 1);
    assert!(!client.has_session());
}

#[tokio::test]
async fn a_failing_server_keeps_the_session() {
    let (server, store, client) = setup(Some("r0")).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .respond_with(problem(503, "unavailable"))
        .mount(&server)
        .await;

    let error = client.get::<Me>("/api/me").await.expect_err("unavailable");
    assert_eq!(error.status(), Some(503));
    assert_eq!(store.load().as_deref(), Some("r0"));
}

#[tokio::test]
async fn without_a_session_nothing_is_sent() {
    let (server, _store, client) = setup(None).await;
    let error = client.get::<Me>("/api/me").await.expect_err("no session");
    assert_eq!(error, ApiError::SignedOut);
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

#[tokio::test]
async fn logout_tells_the_server_and_forgets() {
    let (server, store, client) = setup(None).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access": "a1", "access_expires_in": 900, "refresh": "r1", "user": me()}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/logout"))
        .and(header("authorization", "Bearer a1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client.login("ania", "tajne").await.expect("login");
    client.logout().await;
    assert_eq!(store.load(), None);
}

#[tokio::test]
async fn media_urls_get_the_origin() {
    let (server, _store, client) = setup(None).await;
    assert_eq!(
        client.url("/api/media/ab/cd/sm.webp?e=1&s=x"),
        format!("{}/api/media/ab/cd/sm.webp?e=1&s=x", server.uri())
    );
    assert_eq!(client.url("https://x.example/a"), "https://x.example/a");
}
