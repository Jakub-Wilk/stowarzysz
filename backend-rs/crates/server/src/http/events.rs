//! The live event stream.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};

use super::guard::AuthUser;
use crate::AppState;
use crate::sse::{self, SessionCheck, Subscriber};

/// Server-sent events for the signed-in user (`Authorization: Bearer`, so use a fetch-based
/// SSE client). Starts with `ready`, then carries `poll.*`, `pact.*`, `ledger.*`, `santa.*` and
/// `session.revoked` events (ids only: refetch what changed), `resync` when events may have been
/// missed (refetch everything shown), and a comment heartbeat every 15 s. Ends when the session
/// is revoked or the server restarts; reconnect with backoff.
#[utoipa::path(
    get, path = "/events", tag = "events",
    responses((status = OK, content_type = "text/event-stream", description = "The event stream; event payloads are described by `Event`")),
)]
pub async fn events(State(state): State<AppState>, AuthUser(actor): AuthUser) -> Response {
    let subscriber = Subscriber {
        user_id: actor.user_id,
        session_id: actor.session_id,
    };
    let ctx = Arc::clone(&state.ctx);
    let check = SessionCheck::new(move || {
        let ctx = Arc::clone(&ctx);
        async move {
            match app::auth::session_live(&ctx, subscriber.user_id, subscriber.session_id).await {
                Ok(live) => live,
                Err(error) => {
                    // Keep the stream: a database hiccup should not sign anyone out.
                    tracing::warn!(error = %error, "cannot recheck an event stream's session");
                    true
                }
            }
        }
    });
    let mut response = sse::response(
        state.ctx.hub.subscribe(),
        subscriber,
        check,
        state.shutdown.clone(),
    )
    .into_response();
    // nginx would otherwise buffer the stream.
    response.headers_mut().insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    response
}
