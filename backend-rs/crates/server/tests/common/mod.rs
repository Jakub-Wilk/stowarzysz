//! Test harness shared by the integration tests.
//!
//! Every test gets its own freshly migrated database from `#[sqlx::test(migrator =
//! "app::db::MIGRATOR")]` (needs `docker compose up -d db` and `DATABASE_URL`, see `.env`), and a
//! [`TestApp`]: the production router over that database, a [`FixedClock`], a private
//! `MEDIA_ROOT`, and helpers to create members and send requests as them.
#![allow(dead_code, clippy::unwrap_used, clippy::missing_panics_doc)] // each test binary uses a subset

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use app::Context;
use app::auth;
use app::clock::{Clock as _, FixedClock};
use app::config::Config;
use app::db::Uow;
use app::events::HubMessage;
use app::push::Push;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::Value;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;

pub mod pacts;

/// The password of every member made by [`TestApp::member`].
pub const PASSWORD: &str = "s3cret-pass-123";

pub struct TestApp {
    pub ctx: Arc<Context>,
    pub clock: Arc<FixedClock>,
    pub shutdown: CancellationToken,
    router: Router,
    _media: tempfile::TempDir,
}

/// A member with a live session.
#[derive(Debug, Clone)]
pub struct Member {
    pub id: i64,
    pub username: String,
    /// A valid access token.
    pub token: String,
    pub refresh: String,
}

impl TestApp {
    pub fn new(pool: PgPool) -> Self {
        Self::build(pool, Config::for_tests(), false)
    }

    /// With Web Push enabled (queued notifications land in `push_outbox`).
    pub fn with_push(pool: PgPool) -> Self {
        Self::build(pool, Config::for_tests(), true)
    }

    pub fn with_config(pool: PgPool, config: Config) -> Self {
        Self::build(pool, config, false)
    }

    /// The development configuration plus `pairs` (as environment variables would be).
    pub fn with_env(pool: PgPool, pairs: &[(&str, &str)]) -> Self {
        Self::with_config(pool, env_config(pairs))
    }

    /// Send `body` as the raw request body of a `POST` by `who`.
    pub async fn post_raw(
        &self,
        who: &Member,
        uri: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> TestResponse {
        let request = Request::post(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", who.token))
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body))
            .unwrap();
        self.send(request).await
    }
}

/// The development configuration plus `pairs` (as environment variables would be).
pub fn env_config(pairs: &[(&str, &str)]) -> Config {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect();
    Config::from_lookup(|key| {
        if key == "APP_ENV" {
            return Some("dev".into());
        }
        pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    })
    .unwrap()
}

impl TestApp {
    /// With exchange rates from `rates` (see [`rates_stub`]).
    pub fn with_rates(pool: PgPool, rates: &wiremock::MockServer) -> Self {
        let mut config = Config::for_tests();
        config.rates_url = rates.uri();
        Self::build(pool, config, false)
    }

    /// With `ocr` in place of the one the configuration would make.
    pub fn with_ocr(pool: PgPool, config: Config, ocr: app::ocr::Ocr) -> Self {
        Self::build_with(pool, config, false, |ctx| ctx.ocr = ocr)
    }

    fn build(pool: PgPool, config: Config, push: bool) -> Self {
        Self::build_with(pool, config, push, |_| {})
    }

    fn build_with(
        pool: PgPool,
        mut config: Config,
        push: bool,
        customize: impl FnOnce(&mut Context),
    ) -> Self {
        if config.rates_url.starts_with("https://api.frankfurter") {
            config.rates_url = "http://127.0.0.1:9".into(); // tests never call the real API
        }
        let media = tempfile::tempdir().unwrap();
        config.media_root = media.path().to_path_buf();
        for dir in ["avatars", "photos", app::media::TMP_DIR] {
            std::fs::create_dir_all(media.path().join(dir)).unwrap();
        }
        let clock = Arc::new(FixedClock::at_default());
        let mut ctx = Context::new(config, pool, Arc::clone(&clock) as _);
        if push {
            ctx.push = Push::new(true);
        }
        customize(&mut ctx);
        let ctx = Arc::new(ctx);
        let shutdown = CancellationToken::new();
        let state = server::AppState::with_shutdown(Arc::clone(&ctx), shutdown.clone());
        Self {
            router: server::http::router(state),
            ctx,
            clock,
            shutdown,
            _media: media,
        }
    }

    pub fn db(&self) -> &PgPool {
        &self.ctx.db
    }

    /// A member (active, with [`PASSWORD`]) signed in on one device.
    pub async fn member(&self, username: &str) -> Member {
        self.account(username, false).await
    }

    /// An administrator signed in on one device.
    pub async fn admin(&self, username: &str) -> Member {
        self.account(username, true).await
    }

    async fn account(&self, username: &str, superuser: bool) -> Member {
        let id: i64 = sqlx::query_scalar(
            "insert into users (username, password_hash, is_superuser) values ($1, $2, $3) returning id",
        )
        .bind(username)
        .bind(password_hash(&self.ctx).await)
        .bind(superuser)
        .fetch_one(self.db())
        .await
        .unwrap();
        let tokens = self.session(id).await;
        Member {
            id,
            username: username.into(),
            token: tokens.access,
            refresh: tokens.refresh,
        }
    }

    /// A new session for an existing account.
    pub async fn session(&self, user_id: i64) -> auth::TokenPair {
        let mut uow = Uow::begin(self.db()).await.unwrap();
        let tokens = auth::open_session(&mut uow, &self.ctx, user_id, "tests")
            .await
            .unwrap();
        uow.commit().await.unwrap();
        tokens
    }

    pub async fn send(&self, request: Request<Body>) -> TestResponse {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        TestResponse {
            status,
            headers,
            body,
        }
    }

    /// A JSON request, as `token`'s owner if given.
    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> TestResponse {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        };
        self.send(request.unwrap()).await
    }

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.call(Method::GET, uri, None, None).await
    }

    pub async fn get_as(&self, who: &Member, uri: &str) -> TestResponse {
        self.call(Method::GET, uri, Some(&who.token), None).await
    }

    pub async fn post_as(&self, who: &Member, uri: &str, body: Value) -> TestResponse {
        self.call(Method::POST, uri, Some(&who.token), Some(body))
            .await
    }

    pub async fn patch_as(&self, who: &Member, uri: &str, body: Value) -> TestResponse {
        self.call(Method::PATCH, uri, Some(&who.token), Some(body))
            .await
    }

    pub async fn put_as(&self, who: &Member, uri: &str, body: Value) -> TestResponse {
        self.call(Method::PUT, uri, Some(&who.token), Some(body))
            .await
    }

    pub async fn delete_as(&self, who: &Member, uri: &str) -> TestResponse {
        self.call(Method::DELETE, uri, Some(&who.token), None).await
    }

    /// Upload `bytes` as `who`; returns the response (201 with the upload on success).
    pub async fn upload(
        &self,
        who: &Member,
        purpose: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> TestResponse {
        let request = Request::post(format!("/api/uploads?purpose={purpose}"))
            .header(header::AUTHORIZATION, format!("Bearer {}", who.token))
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(bytes))
            .unwrap();
        self.send(request).await
    }

    /// Run this process's NOTIFY listener until the returned guard is dropped. Returns once it
    /// is connected, so notifications sent afterwards are guaranteed to arrive.
    pub async fn start_listener(&self) -> ListenerGuard {
        let mut hub = self.ctx.hub.subscribe();
        let cancel = self.shutdown.child_token();
        tokio::spawn(app::listener::run(
            self.ctx.db.clone(),
            self.ctx.hub.clone(),
            self.ctx.signals.clone(),
            cancel.clone(),
        ));
        // The listener announces every (re)connection with a resync.
        let connected = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(HubMessage::Resync) = hub.recv().await {
                    return;
                }
            }
        });
        connected.await.expect("the listener connects");
        ListenerGuard(cancel)
    }

    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }

    pub async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(self.db()).await.unwrap()
    }
}

/// One argon2 hash of [`PASSWORD`] per test process (hashing is deliberately slow).
async fn password_hash(ctx: &Context) -> String {
    static HASH: OnceLock<String> = OnceLock::new();
    if let Some(hash) = HASH.get() {
        return hash.clone();
    }
    let hash = ctx.passwords.hash(PASSWORD).await.unwrap();
    HASH.get_or_init(|| hash).clone()
}

/// Stops the listener started by [`TestApp::start_listener`] when dropped.
pub struct ListenerGuard(CancellationToken);

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Debug)]
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "not JSON ({error}), status {}: {}",
                self.status,
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .map_or("", |value| value.to_str().unwrap())
    }

    /// Assert the status, printing the body if it differs.
    #[track_caller]
    pub fn expect(&self, status: StatusCode) -> &Self {
        assert_eq!(
            self.status,
            status,
            "body: {}",
            String::from_utf8_lossy(&self.body)
        );
        self
    }

    /// The problem `code` of an error response.
    pub fn code(&self) -> String {
        self.json()["code"].as_str().unwrap_or_default().to_owned()
    }
}

/// A member (active, activated) created straight in the database, without a session.
pub async fn insert_user(db: &PgPool, username: &str) -> i64 {
    sqlx::query_scalar("insert into users (username, password_hash) values ($1, 'x') returning id")
        .bind(username)
        .fetch_one(db)
        .await
        .unwrap()
}

/// A PNG of `width` x `height` pixels.
pub fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        #[allow(clippy::cast_possible_truncation)]
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

/// Give each of `users` one push subscription (endpoint `https://push.test/<id>`).
pub async fn subscribe(db: &PgPool, users: &[i64]) {
    for user in users {
        sqlx::query("insert into push_subscriptions (user_id, endpoint, p256dh, auth) values ($1, 'https://push.test/' || $1, 'k', 'a')")
            .bind(user)
            .execute(db)
            .await
            .unwrap();
    }
}

/// Queued notifications as `(user id, title, body)`, oldest first; then the outbox is emptied.
pub async fn take_pushes(db: &PgPool) -> Vec<(i64, String, String)> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "select s.user_id, o.payload->>'title', o.payload->>'body'
         from push_outbox o join push_subscriptions s on s.id = o.subscription_id order by o.id",
    )
    .fetch_all(db)
    .await
    .unwrap();
    sqlx::query("delete from push_outbox")
        .execute(db)
        .await
        .unwrap();
    rows
}

/// A Frankfurter stand-in: `rate` PLN per unit of any currency, published on the day asked for.
/// Keep it alive for the test (dropping it stops the server).
pub async fn rates_stub(rate: &str) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    let rate = rate.to_owned();
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(move |request: &wiremock::Request| {
            let day = request.url.path().trim_start_matches('/').to_owned();
            wiremock::ResponseTemplate::new(200).set_body_raw(
                format!(r#"{{"amount":1.0,"base":"X","date":"{day}","rates":{{"PLN":{rate}}}}}"#),
                "application/json",
            )
        })
        .mount(&server)
        .await;
    server
}

/// A bare pact row for ledger entries to settle.
pub async fn insert_pact(db: &PgPool, creator: i64) -> i64 {
    sqlx::query_scalar(
        "insert into pacts (creator_id, kind, title, condition) values ($1, 'bet', 'Zakład', 'x') returning id",
    )
    .bind(creator)
    .fetch_one(db)
    .await
    .unwrap()
}

/// Record `debts` as pact `pact_id`'s settlement, the way pacts will; returns the entry id.
pub async fn record_debts(
    app: &TestApp,
    pact_id: i64,
    debts: Vec<domain::ledger::money::Debt>,
) -> Option<i64> {
    let mut uow = Uow::begin(app.db()).await.unwrap();
    let today = app::clock::local_today(app.now());
    let id = app::ledger::record_debts(&mut uow, &app.ctx, debts, "Zakład", pact_id, today)
        .await
        .unwrap();
    uow.commit().await.unwrap();
    id
}
