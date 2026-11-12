//! HTTP-level tests of the scaffold: health, problem documents, OpenAPI, middleware.
#![allow(clippy::unwrap_used)] // test helpers may panic

use app::config::Config;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt as _;

fn app() -> Router {
    server::http::router(server::AppState::new(Config::for_tests()))
}

async fn send(request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = app().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, body.to_vec())
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn json(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap()
}

#[tokio::test]
async fn health_answers_ok() {
    let (status, headers, body) = send(get("/api/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    assert_eq!(json(&body), serde_json::json!({"status": "ok"}));
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let (_, headers, _) = send(get("/api/health")).await;
    assert!(!headers["x-request-id"].is_empty());

    let request = Request::get("/api/health")
        .header("x-request-id", "abc-123")
        .body(Body::empty())
        .unwrap();
    let (_, headers, _) = send(request).await;
    assert_eq!(headers["x-request-id"], "abc-123");
}

#[tokio::test]
async fn unknown_routes_are_problem_documents() {
    let (status, headers, body) = send(get("/api/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    let problem = json(&body);
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["code"], "not_found");
    assert_eq!(problem["type"], "urn:stowarzysz:problem:not_found");
    assert_eq!(problem["title"], "Nie znaleziono");
    assert_eq!(problem["detail"], "Nie znaleziono.");
}

#[tokio::test]
async fn wrong_method_is_a_problem_document_with_allow() {
    let request = Request::delete("/api/health").body(Body::empty()).unwrap();
    let (status, headers, body) = send(request).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    assert!(headers.contains_key(header::ALLOW));
    assert_eq!(json(&body)["code"], "method_not_allowed");
}

#[tokio::test]
async fn openapi_document_lists_every_route() {
    let (status, headers, body) = send(get("/api/openapi.json")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    let spec = json(&body);
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
#[tokio::test]
async fn every_documented_path_is_routed() {
    let (_, body) = {
        let (status, _, body) = send(get("/api/openapi.json")).await;
        (status, body)
    };
    let spec = json(&body);
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
        let (status, _, _) = send(request).await;
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{path} is documented but not routed"
        );
    }
}

#[tokio::test]
async fn docs_ui_is_served() {
    let (status, headers, body) = send(get("/api/docs")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(String::from_utf8_lossy(&body).contains("scalar"));
}

#[tokio::test]
async fn cors_is_off_unless_configured() {
    let request = Request::options("/api/health")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let (_, headers, _) = send(request).await;
    assert!(!headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

#[tokio::test]
async fn cors_allows_configured_origins() {
    let config = Config::from_lookup(|key| match key {
        "APP_ENV" => Some("dev".into()),
        "CORS_ALLOWED_ORIGINS" => Some("http://localhost:5173".into()),
        _ => None,
    })
    .unwrap();
    let app = server::http::router(server::AppState::new(config));
    let request = Request::options("/api/health")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "http://localhost:5173"
    );
}
