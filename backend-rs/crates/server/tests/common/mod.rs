//! Test harness shared by the integration tests.
//!
//! Every test gets its own freshly migrated database from `#[sqlx::test(migrator =
//! "app::db::MIGRATOR")]` (needs `docker compose up -d db` and `DATABASE_URL`, see `.env`), and a
//! [`TestApp`]: the production router over that database, a [`FixedClock`] and helpers to send
//! requests.
#![allow(dead_code, clippy::unwrap_used)] // each test binary uses a different subset

use std::sync::Arc;
use std::time::Duration;

use app::Context;
use app::clock::FixedClock;
use app::config::Config;
use app::events::HubMessage;
use app::push::Push;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;

pub struct TestApp {
    pub ctx: Arc<Context>,
    pub clock: Arc<FixedClock>,
    pub shutdown: CancellationToken,
    router: Router,
}

impl TestApp {
    pub fn new(pool: PgPool) -> Self {
        Self::build(pool, Config::for_tests(), false)
    }

    /// With Web Push enabled (deliveries go wherever the test sends them).
    pub fn with_push(pool: PgPool) -> Self {
        Self::build(pool, Config::for_tests(), true)
    }

    pub fn with_config(pool: PgPool, config: Config) -> Self {
        Self::build(pool, config, false)
    }

    fn build(pool: PgPool, config: Config, push: bool) -> Self {
        let clock = Arc::new(FixedClock::at_default());
        let mut ctx = Context::new(config, pool, Arc::clone(&clock) as _);
        if push {
            ctx.push = Push::new(true);
        }
        let ctx = Arc::new(ctx);
        let shutdown = CancellationToken::new();
        let state = server::AppState::with_shutdown(Arc::clone(&ctx), shutdown.clone());
        Self {
            router: server::http::router(state),
            ctx,
            clock,
            shutdown,
        }
    }

    pub fn db(&self) -> &PgPool {
        &self.ctx.db
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

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.send(Request::get(uri).body(Body::empty()).unwrap())
            .await
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
                "not JSON ({error}): {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .map_or("", |value| value.to_str().unwrap())
    }
}

/// A member (active, activated) created straight in the database.
pub async fn insert_user(db: &PgPool, username: &str) -> i64 {
    sqlx::query_scalar("insert into users (username, password_hash) values ($1, 'x') returning id")
        .bind(username)
        .fetch_one(db)
        .await
        .unwrap()
}
