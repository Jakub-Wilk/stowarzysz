use serde::Serialize;
use utoipa::ToSchema;

use super::json::ApiJson;

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// Always `ok` when the process serves requests.
    #[schema(example = "ok")]
    pub status: &'static str,
}

/// Liveness probe used by the container health check and `scripts/deploy.sh`.
#[utoipa::path(
    get,
    path = "/health",
    tag = "meta",
    security(()),
    responses((status = OK, body = Health)),
)]
pub async fn health() -> ApiJson<Health> {
    ApiJson(Health { status: "ok" })
}
