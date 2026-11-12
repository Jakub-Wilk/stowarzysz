//! The HTTP API: routes, OpenAPI document and middleware stack.

mod health;
pub mod json;
pub mod problem;

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes, HttpBody};
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{BoxError, Router};
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_scalar::{Scalar, Servable};

use crate::AppState;
use problem::Problem;

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
/// Sent by SSE clients when reconnecting.
const LAST_EVENT_ID: HeaderName = HeaderName::from_static("last-event-id");
/// Default deadline for a request; long-running routes (uploads, OCR, SSE) get their own.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(OpenApi)]
#[openapi(
    info(
        title = "stowarzysz API",
        version = "2.0.0",
        description = "JSON API of stowarzysz. Every error is an RFC 9457 `application/problem+json` \
                       document (`Problem`). Authenticate with `Authorization: Bearer <access token>`."
    ),
    modifiers(&BearerAuth),
    components(schemas(Problem)),
    tags((name = "meta", description = "Health and API metadata")),
)]
struct ApiDoc;

/// Bearer JWT is the default for every operation; public ones opt out with `security(())`.
struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
        openapi.security = Some(vec![SecurityRequirement::new(
            "bearer",
            Vec::<String>::new(),
        )]);
    }
}

/// Every documented route, mounted under `/api`. Routes are registered together with their
/// OpenAPI operation, so the document cannot drift from the router.
pub fn api() -> OpenApiRouter<AppState> {
    let routes = OpenApiRouter::new().routes(routes!(health::health));
    OpenApiRouter::with_openapi(ApiDoc::openapi()).nest("/api", routes)
}

/// The complete application: API, OpenAPI document, docs UI and middleware.
pub fn router(state: AppState) -> Router {
    let config = Arc::clone(&state.config);
    let (api, openapi) = api().split_for_parts();
    let spec: Arc<str> = openapi
        .to_json()
        .expect("the OpenAPI document serialises")
        .into();

    let mut router = api
        .route(
            "/api/openapi.json",
            get(move || {
                let spec = Arc::clone(&spec);
                async move {
                    (
                        [(header::CONTENT_TYPE, "application/json")],
                        spec.to_string(),
                    )
                }
            }),
        )
        .merge(Scalar::with_url("/api/docs", openapi))
        .fallback(|| async { Problem::for_status(StatusCode::NOT_FOUND) })
        .layer(DefaultBodyLimit::max(config.limits.json_body_bytes))
        .with_state(state);

    // Outermost first. The problem normaliser sits outside the panic catcher and the timeout so
    // it also rewrites their bare responses (and axum's own 405/413 rejections).
    router = router.layer(
        ServiceBuilder::new()
            .layer(CompressionLayer::new().compress_when(
                DefaultPredicate::new().and(NotForContentType::new("text/event-stream")),
            ))
            .layer(axum::middleware::map_response(problemize))
            .layer(CatchPanicLayer::new())
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                REQUEST_TIMEOUT,
            )),
    );
    if let Some(cors) = cors(&config.cors_allowed_origins) {
        router = router.layer(cors);
    }
    router.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(|request: &Request| {
                        let request_id = request
                            .headers()
                            .get(&REQUEST_ID)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or("-");
                        tracing::info_span!(
                            "http",
                            method = %request.method(),
                            path = %request.uri().path(),
                            request_id = %request_id,
                        )
                    })
                    // One line per request (status + latency); 5xx are logged at ERROR.
                    .on_request(())
                    .on_response(DefaultOnResponse::new().level(Level::INFO)),
            )
            .layer(PropagateRequestIdLayer::new(REQUEST_ID)),
    )
}

/// Only for development across origins; production is same-origin behind nginx.
fn cors(origins: &[String]) -> Option<CorsLayer> {
    let origins: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    if origins.is_empty() {
        return None;
    }
    Some(
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, LAST_EVENT_ID])
            .expose_headers([REQUEST_ID, header::RETRY_AFTER])
            .max_age(Duration::from_secs(3600)),
    )
}

/// Turn any error response that is not already a problem document (axum's plain-text
/// rejections, timeouts, panics, 405s) into one, keeping the headers that matter.
async fn problemize<B>(response: Response<B>) -> Response
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    let status = response.status();
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| {
            value
                .as_bytes()
                .starts_with(problem::CONTENT_TYPE.as_bytes())
        });
    if is_problem || !(status.is_client_error() || status.is_server_error()) {
        return response.map(Body::new);
    }
    let (parts, _) = response.into_parts();
    let mut replaced = Problem::for_status(status).into_response();
    for name in [header::ALLOW, header::RETRY_AFTER, header::WWW_AUTHENTICATE] {
        if let Some(value) = parts.headers.get(&name) {
            replaced.headers_mut().insert(name, value.clone());
        }
    }
    replaced
}
