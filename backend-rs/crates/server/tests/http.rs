//! HTTP basics: health, problem documents, OpenAPI, middleware.
#![allow(clippy::unwrap_used)]

mod common;

use app::config::Config;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::TestApp;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn health_answers_ok_when_the_database_does(pool: PgPool) {
    let response = TestApp::new(pool).get("/api/health").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.header("content-type"), "application/json");
    assert_eq!(response.json(), serde_json::json!({"status": "ok"}));
}

#[tokio::test]
async fn health_is_unavailable_without_the_database() {
    let dead = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(500))
        .connect_lazy("postgres://nobody@127.0.0.1:1/nothing")
        .unwrap();
    let response = TestApp::new(dead).get("/api/health").await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.header("content-type"), "application/problem+json");
    assert_eq!(response.json()["code"], "database_unavailable");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_response_carries_a_request_id(pool: PgPool) {
    let app = TestApp::new(pool);
    let generated = app.get("/api/health").await;
    assert_ne!(
        generated.header("x-request-id"),
        "",
        "a request id is generated"
    );

    let request = Request::get("/api/health")
        .header("x-request-id", "abc-123")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send(request).await.header("x-request-id"), "abc-123");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn unknown_routes_are_problem_documents(pool: PgPool) {
    let response = TestApp::new(pool).get("/api/nope").await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(response.header("content-type"), "application/problem+json");
    insta::assert_json_snapshot!(response.json(), @r#"
    {
      "code": "not_found",
      "detail": "Nie znaleziono.",
      "status": 404,
      "title": "Nie znaleziono",
      "type": "urn:stowarzysz:problem:not_found"
    }
    "#);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn wrong_method_is_a_problem_document_with_allow(pool: PgPool) {
    let request = Request::delete("/api/health").body(Body::empty()).unwrap();
    let response = TestApp::new(pool).send(request).await;
    assert_eq!(response.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.header("content-type"), "application/problem+json");
    assert!(response.headers.contains_key(header::ALLOW));
    assert_eq!(response.json()["code"], "method_not_allowed");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn openapi_document_describes_the_api(pool: PgPool) {
    let response = TestApp::new(pool).get("/api/openapi.json").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.header("content-type"), "application/json");
    let spec = response.json();
    assert_eq!(spec["info"]["title"], "stowarzysz API");
    assert!(spec["components"]["schemas"]["Problem"].is_object());
    assert!(spec["components"]["securitySchemes"]["bearer"].is_object());
    // Health is public: it opts out of the default bearer requirement.
    assert_eq!(
        spec["paths"]["/api/health"]["get"]["security"],
        serde_json::json!([{}])
    );
}

/// Routes are registered through `utoipa_axum::routes!`, so every handler is documented by
/// construction. This checks the other direction: every documented path is really routed.
/// An unsupported method yields 405 on a routed path and 404 on an unknown one.
#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_documented_path_is_routed(pool: PgPool) {
    let app = TestApp::new(pool);
    let spec = app.get("/api/openapi.json").await.json();
    let paths = spec["paths"].as_object().unwrap();
    assert!(!paths.is_empty());
    for path in paths.keys() {
        let concrete = path
            .split('/')
            .map(|segment| {
                if segment.starts_with('{') {
                    "1"
                } else {
                    segment
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        let request = Request::builder()
            .method("TRACE")
            .uri(&concrete)
            .body(Body::empty())
            .unwrap();
        let status = app.send(request).await.status;
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{path} is documented but not routed"
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn docs_ui_is_served(pool: PgPool) {
    let response = TestApp::new(pool).get("/api/docs").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.header("content-type").starts_with("text/html"));
    assert!(String::from_utf8_lossy(&response.body).contains("scalar"));
}

fn preflight() -> Request<Body> {
    Request::options("/api/health")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn cors_is_off_unless_configured(pool: PgPool) {
    let response = TestApp::new(pool).send(preflight()).await;
    assert!(
        !response
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn cors_allows_configured_origins(pool: PgPool) {
    let config = Config::from_lookup(|key| match key {
        "APP_ENV" => Some("dev".into()),
        "CORS_ALLOWED_ORIGINS" => Some("http://localhost:5173".into()),
        _ => None,
    })
    .unwrap();
    let response = TestApp::with_config(pool, config).send(preflight()).await;
    assert_eq!(
        response.header("access-control-allow-origin"),
        "http://localhost:5173"
    );
}
