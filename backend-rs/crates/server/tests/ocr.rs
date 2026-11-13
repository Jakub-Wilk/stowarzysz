//! Receipt OCR: compression, candidate failover, breakers, hedging, deadlines and progress
//! events, with Gemini (and Vertex AI) stood in for by a local mock server.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::{Duration, Instant};

use app::events::{Audience, Event, HubMessage};
use app::ocr::{Ocr, Router, VertexAuth};
use axum::http::StatusCode;
use base64::Engine as _;
use common::{Member, TestApp, TestResponse, env_config, png};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn studio(model: &str) -> String {
    format!("/v1beta/models/{model}:generateContent")
}

/// Gemini's answer carrying `receipt` as the model's JSON text.
fn answer(receipt: &Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "candidates": [{"content": {"parts": [{"text": receipt.to_string()}]}, "finishReason": "STOP"}]
    }))
}

async fn mock(server: &MockServer, url: String, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(url))
        .respond_with(response)
        .mount(server)
        .await;
}

/// An app whose OCR talks to `gemini` with `pairs` on top (rounds back off for 20 ms).
fn app_with(pool: PgPool, gemini: &MockServer, pairs: &[(&str, &str)], vertex: bool) -> TestApp {
    let uri = gemini.uri();
    let mut all = vec![
        ("GEMINI_BASE_URL", uri.as_str()),
        ("GEMINI_MODEL", "m1"),
        ("OCR_DEADLINE", "10s"),
    ];
    all.extend_from_slice(pairs);
    if !all.iter().any(|(k, _)| *k == "GEMINI_API_KEY") && !vertex {
        all.push(("GEMINI_API_KEY", "test-key"));
    }
    let config = env_config(&all);
    let auth = vertex.then(|| VertexAuth::with_token("vertex-token"));
    let router = Router::new(&config, auth).with_backoff(Duration::from_millis(20));
    let ocr = Ocr::with_router(&config, router);
    TestApp::with_ocr(pool, config, ocr)
}

async fn scan_as(app: &TestApp, who: &Member, job: Uuid, image: Vec<u8>) -> TestResponse {
    app.post_raw(
        who,
        &format!("/api/ledger/receipt-ocr?job_id={job}"),
        "image/png",
        image,
    )
    .await
}

async fn scan(app: &TestApp, who: &Member) -> TestResponse {
    scan_as(app, who, Uuid::new_v4(), png(64, 48)).await
}

async fn requests_to(server: &MockServer, url: &str) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == url)
        .collect()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn without_configuration_or_valid_input_nothing_is_sent(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let unconfigured = scan(&app, &ala).await;
    assert_eq!(
        (unconfigured.status, unconfigured.code().as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, "ocr_not_configured")
    );
    let no_job = app
        .post_raw(&ala, "/api/ledger/receipt-ocr", "image/png", png(8, 8))
        .await;
    no_job.expect(StatusCode::BAD_REQUEST);
    let anonymous = app
        .call(
            axum::http::Method::POST,
            &format!("/api/ledger/receipt-ocr?job_id={}", Uuid::new_v4()),
            None,
            None,
        )
        .await;
    anonymous.expect(StatusCode::UNAUTHORIZED);
    let text = app
        .post_raw(
            &ala,
            &format!("/api/ledger/receipt-ocr?job_id={}", Uuid::new_v4()),
            "text/plain",
            b"hi".to_vec(),
        )
        .await;
    text.expect(StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_receipt_is_read_and_each_request_announced_to_the_caller(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(
        &gemini,
        studio("m1"),
        answer(&json!({"total": 12.5, "merchant": {"name": "Żabka"}, "currency": "PLN"})),
    )
    .await;
    let app = app_with(pool, &gemini, &[], false);
    let ala = app.member("ala").await;
    let _listener = app.start_listener().await;
    let mut hub = app.ctx.hub.subscribe();
    let job = Uuid::new_v4();
    let body = scan_as(&app, &ala, job, png(64, 48))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(body["receipt"]["total"], 12.5);
    assert_eq!(body["receipt"]["merchant"]["name"], "Żabka");
    assert_eq!(
        (&body["served_by"], &body["attempts"]),
        (&json!("studio/m1"), &json!(1))
    );

    let request = &requests_to(&gemini, &studio("m1")).await[0];
    assert_eq!(request.headers.get("x-goog-api-key").unwrap(), "test-key");
    let sent: Value = serde_json::from_slice(&request.body).unwrap();
    let inline = &sent["contents"][0]["parts"][0]["inlineData"];
    assert_eq!(inline["mimeType"], "image/webp");
    let image = base64::engine::general_purpose::STANDARD
        .decode(inline["data"].as_str().unwrap())
        .unwrap();
    assert!(image.starts_with(b"RIFF"), "sent as WebP");
    assert_eq!(
        sent["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert!(sent["generationConfig"]["responseSchema"]["properties"]["total"].is_object());
    assert!(sent.get("service_tier").is_none());

    let message = tokio::time::timeout(Duration::from_secs(5), hub.recv())
        .await
        .unwrap()
        .unwrap();
    let HubMessage::Event(envelope) = message else {
        panic!("expected an event")
    };
    assert_eq!(envelope.audience, Audience::User(ala.id));
    assert_eq!(
        envelope.event,
        Event::LedgerOcrRequest {
            job_id: job,
            attempt: 1,
            candidate: "studio/m1".into()
        }
    );
    // nothing about the scan is kept
    assert_eq!(
        std::fs::read_dir(app.ctx.media.tmp_dir()).unwrap().count(),
        0
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_overloaded_model_fails_over_to_the_next_candidate_at_once(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(&gemini, studio("m1"), ResponseTemplate::new(503)).await;
    mock(&gemini, studio("m2"), answer(&json!({"total": 3.0}))).await;
    let app = app_with(pool, &gemini, &[("GEMINI_FALLBACK_MODELS", "m2")], false);
    let ala = app.member("ala").await;
    let started = Instant::now();
    let body = scan(&app, &ala).await.expect(StatusCode::OK).json();
    assert_eq!(
        (&body["served_by"], &body["attempts"]),
        (&json!("studio/m2"), &json!(2))
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "no backoff between candidates"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_priority_tier_is_tried_first_and_a_429_falls_back_to_standard(pool: PgPool) {
    let gemini = MockServer::start().await;
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = std::sync::Arc::clone(&calls);
    Mock::given(method("POST"))
        .and(path(studio("m1")))
        .respond_with(move |request: &Request| {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            if body["service_tier"] == "priority" {
                ResponseTemplate::new(429)
            } else {
                answer(&json!({"total": 1.0}))
            }
        })
        .mount(&gemini)
        .await;
    let app = app_with(pool, &gemini, &[("GEMINI_PRIORITY", "true")], false);
    let ala = app.member("ala").await;
    let body = scan(&app, &ala).await.expect(StatusCode::OK).json();
    assert_eq!(body["served_by"], "studio/m1");
    assert_eq!(body["attempts"], 2);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    let tiers: Vec<String> = app
        .ctx
        .ocr
        .router()
        .candidates()
        .map(str::to_owned)
        .collect();
    assert_eq!(tiers, ["studio/m1/priority", "studio/m1"]);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_failing_candidate_is_rested_after_three_failures(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(&gemini, studio("m1"), ResponseTemplate::new(500)).await;
    mock(&gemini, studio("m2"), answer(&json!({"total": 1.0}))).await;
    let app = app_with(pool, &gemini, &[("GEMINI_FALLBACK_MODELS", "m2")], false);
    let ala = app.member("ala").await;
    for _ in 0..4 {
        scan(&app, &ala).await.expect(StatusCode::OK);
    }
    assert_eq!(
        requests_to(&gemini, &studio("m1")).await.len(),
        3,
        "skipped once open"
    );
    assert_eq!(requests_to(&gemini, &studio("m2")).await.len(), 4);
    assert!(app.ctx.ocr.router().is_open("studio/m1"));
    assert!(!app.ctx.ocr.router().is_open("studio/m2"));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_refused_request_is_not_retried_and_a_bad_key_moves_on(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(&gemini, studio("m1"), ResponseTemplate::new(400)).await;
    let app = app_with(pool, &gemini, &[], false);
    let ala = app.member("ala").await;
    let refused = scan(&app, &ala).await;
    assert_eq!(
        (refused.status, refused.code().as_str()),
        (StatusCode::BAD_REQUEST, "receipt_unreadable")
    );
    assert_eq!(requests_to(&gemini, &studio("m1")).await.len(), 1);

    let other = MockServer::start().await;
    mock(&other, studio("m1"), ResponseTemplate::new(401)).await;
    mock(&other, studio("m2"), answer(&json!({"total": 2.0}))).await;
    let app = app_with(
        app.db().clone(),
        &other,
        &[("GEMINI_FALLBACK_MODELS", "m2")],
        false,
    );
    let ola = app.member("ola").await;
    let body = scan(&app, &ola).await.expect(StatusCode::OK).json();
    assert_eq!(body["served_by"], "studio/m2");
    assert!(
        app.ctx.ocr.router().is_open("studio/m1"),
        "misconfigured: rested at once"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn answers_that_are_not_receipts_give_up_without_blaming_the_endpoint(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(
        &gemini,
        studio("m1"),
        ResponseTemplate::new(200)
            .set_body_json(json!({"candidates": [{"finishReason": "SAFETY"}]})),
    )
    .await;
    let app = app_with(pool, &gemini, &[], false);
    let ala = app.member("ala").await;
    let response = scan(&app, &ala).await;
    assert_eq!(
        (response.status, response.code().as_str()),
        (StatusCode::BAD_REQUEST, "receipt_unreadable")
    );
    assert_eq!(requests_to(&gemini, &studio("m1")).await.len(), 2);
    assert!(!app.ctx.ocr.router().is_open("studio/m1"));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_deadline_is_respected(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(
        &gemini,
        studio("m1"),
        ResponseTemplate::new(503).insert_header("retry-after", "0"),
    )
    .await;
    let app = app_with(pool, &gemini, &[("OCR_DEADLINE", "1s")], false);
    let ala = app.member("ala").await;
    let started = Instant::now();
    let response = scan(&app, &ala).await;
    assert_eq!(
        (response.status, response.code().as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, "ocr_unavailable")
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "gave up at the deadline"
    );
    assert!(
        requests_to(&gemini, &studio("m1")).await.len() >= 2,
        "retried after backing off"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_slow_request_is_hedged_and_the_first_answer_wins(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(
        &gemini,
        studio("m1"),
        answer(&json!({"total": 1.0})).set_delay(Duration::from_secs(5)),
    )
    .await;
    mock(&gemini, studio("m2"), answer(&json!({"total": 2.0}))).await;
    let app = app_with(
        pool,
        &gemini,
        &[
            ("GEMINI_FALLBACK_MODELS", "m2"),
            ("OCR_HEDGE_AFTER", "200ms"),
        ],
        false,
    );
    let ala = app.member("ala").await;
    let started = Instant::now();
    let body = scan(&app, &ala).await.expect(StatusCode::OK).json();
    assert_eq!(
        (&body["served_by"], &body["receipt"]["total"]),
        (&json!("studio/m2"), &json!(2.0))
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "did not wait for the slow one"
    );
    assert!(
        !app.ctx.ocr.router().is_open("studio/m1"),
        "a slow answer is not a failure"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn vertex_candidates_use_their_location_and_a_bearer_token(pool: PgPool) {
    let vertex = MockServer::start().await;
    let global = "/v1/projects/proj/locations/global/publishers/google/models/m1:generateContent";
    let warsaw =
        "/v1/projects/proj/locations/europe-central2/publishers/google/models/m1:generateContent";
    Mock::given(method("POST"))
        .and(path(global))
        .respond_with(ResponseTemplate::new(503))
        .mount(&vertex)
        .await;
    Mock::given(method("POST"))
        .and(path(warsaw))
        .and(header("authorization", "Bearer vertex-token"))
        .respond_with(answer(&json!({"total": 9.99})))
        .mount(&vertex)
        .await;
    let uri = vertex.uri();
    let pairs = [
        ("VERTEX_PROJECT", "proj"),
        ("VERTEX_CREDENTIALS", "unused.json"),
        ("VERTEX_LOCATIONS", "global,europe-central2"),
        ("VERTEX_BASE_URL", uri.as_str()),
    ];
    let app = app_with(pool, &vertex, &pairs, true);
    let ala = app.member("ala").await;
    let body = scan(&app, &ala).await.expect(StatusCode::OK).json();
    assert_eq!(body["served_by"], "vertex-europe-central2/m1");
    assert_eq!(body["receipt"]["total"], 9.99);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_huge_photo_is_compressed_under_the_inline_budget(pool: PgPool) {
    let gemini = MockServer::start().await;
    mock(&gemini, studio("m1"), answer(&json!({"total": 1.0}))).await;
    let app = app_with(
        pool,
        &gemini,
        &[("GEMINI_INLINE_BUDGET_BYTES", "4000000")],
        false,
    );
    let ala = app.member("ala").await;
    // ~22 MB of incompressible pixels: neither lossless nor lossy WebP fits at full size.
    let mut state = 0x2545_f491_u32;
    let noise = image::RgbImage::from_fn(3000, 2500, |_, _| {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        };
        image::Rgb([next(), next(), next()])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(noise)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = bytes.into_inner();
    assert!(bytes.len() > 20_000_000);
    scan_as(&app, &ala, Uuid::new_v4(), bytes)
        .await
        .expect(StatusCode::OK);
    let request = &requests_to(&gemini, &studio("m1")).await[0];
    let sent: Value = serde_json::from_slice(&request.body).unwrap();
    let data = sent["contents"][0]["parts"][0]["inlineData"]["data"]
        .as_str()
        .unwrap();
    let image = base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap();
    assert!(image.len() <= 4_000_000, "{} bytes", image.len());
    let decoded = image::load_from_memory(&image).unwrap();
    assert!(
        decoded.width() >= 1536,
        "still legible: {}",
        decoded.width()
    );
}
