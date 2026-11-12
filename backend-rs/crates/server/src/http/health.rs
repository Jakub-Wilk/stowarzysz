use std::time::Duration;

use app::AppError;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// `ok` when the API and its database answer.
    #[schema(example = "ok")]
    pub status: &'static str,
}

/// Readiness probe used by the container health check and `scripts/deploy.sh`: the process is
/// up and the database answers.
#[utoipa::path(
    get,
    path = "/health",
    tag = "meta",
    security(()),
    responses(
        (status = OK, body = Health),
        (status = SERVICE_UNAVAILABLE, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn health(State(state): State<AppState>) -> ApiResult<ApiJson<Health>> {
    let ping = sqlx::query("select 1").execute(&state.ctx.db);
    match tokio::time::timeout(Duration::from_secs(2), ping).await {
        Ok(Ok(_)) => Ok(ApiJson(Health { status: "ok" })),
        Ok(Err(error)) => {
            tracing::warn!(%error, "health check: database error");
            Err(unavailable().into())
        }
        Err(_) => {
            tracing::warn!("health check: database timed out");
            Err(unavailable().into())
        }
    }
}

fn unavailable() -> AppError {
    AppError::unavailable(
        "database_unavailable",
        "Baza danych jest chwilowo niedostępna.",
    )
}
