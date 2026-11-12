//! Account management, for administrators.

use app::accounts::ManagedUser;
use app::accounts::activation::{self, ActivationLink};
use app::accounts::users::{self, NewUser, UserChanges};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::guard::AdminUser;
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

/// Every account, by username, with statistics.
#[utoipa::path(get, path = "/users", tag = "users", responses((status = OK, body = Vec<ManagedUser>)))]
pub async fn list(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> ApiResult<ApiJson<Vec<ManagedUser>>> {
    Ok(ApiJson(users::list(&state.ctx).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUser {
    pub username: String,
    #[serde(default = "yes")]
    pub is_active: bool,
    #[serde(default)]
    pub is_superuser: bool,
    #[serde(default)]
    pub birthday: Option<NaiveDate>,
}

fn yes() -> bool {
    true
}

/// Create an account. It has no password: send the person an activation link.
#[utoipa::path(
    post, path = "/users", tag = "users",
    request_body = CreateUser,
    responses(
        (status = CREATED, body = ManagedUser),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    ApiJson(request): ApiJson<CreateUser>,
) -> ApiResult<(StatusCode, ApiJson<ManagedUser>)> {
    let user = users::create(
        &state.ctx,
        NewUser {
            username: request.username,
            is_active: request.is_active,
            is_superuser: request.is_superuser,
            birthday: request.birthday,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson(user)))
}

#[utoipa::path(
    get, path = "/users/{id}", tag = "users",
    responses((status = OK, body = ManagedUser), (status = NOT_FOUND, body = Problem, content_type = "application/problem+json")),
)]
pub async fn get(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<ManagedUser>> {
    Ok(ApiJson(users::get(&state.ctx, id).await?))
}

/// Fields left out stay as they are; `birthday: null` clears it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateUser {
    pub username: Option<String>,
    pub is_active: Option<bool>,
    pub is_superuser: Option<bool>,
    #[allow(clippy::option_option)] // absent (keep) vs null (clear): the serde idiom
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<NaiveDate>, nullable)]
    pub birthday: Option<Option<NaiveDate>>,
}

/// Tells a field that is `null` (`Some(None)`) from one that is missing (`None`).
#[allow(clippy::option_option)]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Change an account. Administrators cannot deactivate or demote themselves; deactivating
/// signs the person out everywhere.
#[utoipa::path(
    patch, path = "/users/{id}", tag = "users",
    request_body = UpdateUser,
    responses(
        (status = OK, body = ManagedUser),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<UpdateUser>,
) -> ApiResult<ApiJson<ManagedUser>> {
    let changes = UserChanges {
        username: request.username,
        is_active: request.is_active,
        is_superuser: request.is_superuser,
        birthday: request.birthday,
    };
    Ok(ApiJson(
        users::update(&state.ctx, &actor, id, changes).await?,
    ))
}

/// Delete an account that has no history. One that took part in anything answers 409:
/// deactivate it instead.
#[utoipa::path(
    delete, path = "/users/{id}", tag = "users",
    responses(
        (status = NO_CONTENT),
        (status = CONFLICT, description = "The account has history", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    users::delete(&state.ctx, &actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Issue a one-time activation link; earlier unused links of this account stop working.
#[utoipa::path(
    post, path = "/users/{id}/activation-link", tag = "users",
    responses((status = CREATED, body = ActivationLink), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn activation_link(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<(StatusCode, ApiJson<ActivationLink>)> {
    let link = activation::issue(&state.ctx, actor.user_id, id).await?;
    Ok((StatusCode::CREATED, ApiJson(link)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetAvatar {
    /// An `avatar` upload of the caller (`POST /api/uploads?purpose=avatar`).
    pub upload_id: Uuid,
}

/// Set an account's profile picture from an upload.
#[utoipa::path(
    put, path = "/users/{id}/avatar", tag = "users",
    request_body = SetAvatar,
    responses((status = OK, body = ManagedUser), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn set_avatar(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<SetAvatar>,
) -> ApiResult<ApiJson<ManagedUser>> {
    Ok(ApiJson(
        users::set_avatar(&state.ctx, &actor, id, request.upload_id).await?,
    ))
}

/// Remove an account's profile picture.
#[utoipa::path(delete, path = "/users/{id}/avatar", tag = "users", responses((status = OK, body = ManagedUser)))]
pub async fn clear_avatar(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<ManagedUser>> {
    Ok(ApiJson(users::clear_avatar(&state.ctx, id).await?))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Revoked {
    /// How many sessions were ended.
    pub revoked: u64,
}

/// Sign an account out of every device.
#[utoipa::path(post, path = "/users/{id}/revoke-sessions", tag = "users", responses((status = OK, body = Revoked)))]
pub async fn revoke_sessions(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<Revoked>> {
    Ok(ApiJson(Revoked {
        revoked: users::revoke_sessions(&state.ctx, id).await?,
    }))
}
