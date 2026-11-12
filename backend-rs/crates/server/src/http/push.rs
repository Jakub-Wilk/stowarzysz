//! Web Push subscriptions.

use app::push::{self, Subscription};
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::guard::{AuthUser, UserAgent};
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicKey {
    /// The VAPID public key for `pushManager.subscribe`; `null` when push is not configured
    /// (then hide the notifications toggle).
    pub public_key: Option<String>,
}

#[utoipa::path(get, path = "/push/public-key", tag = "push", responses((status = OK, body = PublicKey)))]
pub async fn public_key(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
) -> ApiJson<PublicKey> {
    ApiJson(PublicKey {
        public_key: state
            .ctx
            .config
            .vapid
            .as_ref()
            .map(|v| v.public_key.clone()),
    })
}

/// Register this browser for notifications (idempotent; an endpoint follows whoever registered
/// it last).
#[utoipa::path(
    put, path = "/push/subscription", tag = "push",
    request_body = Subscription,
    responses((status = NO_CONTENT), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    UserAgent(agent): UserAgent,
    ApiJson(subscription): ApiJson<Subscription>,
) -> ApiResult<StatusCode> {
    push::subscribe(&state.ctx, actor.user_id, &subscription, &agent).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Unsubscribe {
    pub endpoint: String,
}

/// Stop notifications for this browser.
#[utoipa::path(delete, path = "/push/subscription", tag = "push", request_body = Unsubscribe, responses((status = NO_CONTENT)))]
pub async fn unsubscribe(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    ApiJson(request): ApiJson<Unsubscribe>,
) -> ApiResult<StatusCode> {
    push::unsubscribe(&state.ctx, actor.user_id, &request.endpoint).await?;
    Ok(StatusCode::NO_CONTENT)
}
