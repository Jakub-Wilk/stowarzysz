//! Signing in and out, activation, and what members see about each other.

use app::accounts::{LoginUser, Me, Person, activation, profile};
use app::auth::{self, TokenPair};
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::guard::{AuthUser, ClientIp, UserAgent, throttle};
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    #[serde(flatten)]
    pub tokens: TokenPair,
    pub user: Me,
}

/// Sign in. Rate-limited per address and per username.
#[utoipa::path(
    post, path = "/auth/login", tag = "auth", security(()),
    request_body = LoginRequest,
    responses(
        (status = OK, body = LoginResponse),
        (status = UNAUTHORIZED, description = "Wrong username or password", body = Problem, content_type = "application/problem+json"),
        (status = TOO_MANY_REQUESTS, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    UserAgent(agent): UserAgent,
    ApiJson(request): ApiJson<LoginRequest>,
) -> ApiResult<ApiJson<LoginResponse>> {
    throttle(&state.limiters.login_ip, &ip)?;
    throttle(
        &state.limiters.login_username,
        &request.username.trim().to_lowercase(),
    )?;
    let (tokens, user_id) =
        auth::login(&state.ctx, &request.username, &request.password, &agent).await?;
    let user = profile::me(&state.ctx, user_id).await?;
    Ok(ApiJson(LoginResponse { tokens, user }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh: String,
}

/// Exchange a refresh token for new tokens. The refresh token rotates, but the previous one
/// stays valid until the new one has been used, so a client that lost the response can retry.
/// A refresh token of the previous (Django) backend is accepted once.
#[utoipa::path(
    post, path = "/auth/refresh", tag = "auth", security(()),
    request_body = RefreshRequest,
    responses(
        (status = OK, body = TokenPair),
        (status = UNAUTHORIZED, description = "The session ended: sign in again", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn refresh(
    State(state): State<AppState>,
    UserAgent(agent): UserAgent,
    ApiJson(request): ApiJson<RefreshRequest>,
) -> ApiResult<ApiJson<TokenPair>> {
    Ok(ApiJson(
        auth::refresh(&state.ctx, request.refresh.trim(), &agent).await?,
    ))
}

/// End this session (this device).
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = NO_CONTENT)))]
pub async fn logout(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<StatusCode> {
    auth::logout(&state.ctx, &actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ActivationToken {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActivationTarget {
    pub username: String,
}

/// Check an activation link before asking for a password. Rate-limited per address.
#[utoipa::path(
    post, path = "/activation/validate", tag = "auth", security(()),
    request_body = ActivationToken,
    responses(
        (status = OK, body = ActivationTarget),
        (status = BAD_REQUEST, description = "Invalid or expired link", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn validate_activation(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(request): ApiJson<ActivationToken>,
) -> ApiResult<ApiJson<ActivationTarget>> {
    throttle(&state.limiters.activation, &ip)?;
    let username = activation::validate(&state.ctx, request.token.trim()).await?;
    Ok(ApiJson(ActivationTarget { username }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CompleteActivation {
    pub token: String,
    pub password: String,
}

/// Set the password of an account through its activation link (the link is used up). The
/// person then signs in normally.
#[utoipa::path(
    post, path = "/activation/complete", tag = "auth", security(()),
    request_body = CompleteActivation,
    responses(
        (status = NO_CONTENT),
        (status = BAD_REQUEST, description = "Invalid link or a weak password", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn complete_activation(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(request): ApiJson<CompleteActivation>,
) -> ApiResult<StatusCode> {
    throttle(&state.limiters.activation, &ip)?;
    activation::complete(&state.ctx, request.token.trim(), &request.password).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The signed-in user.
#[utoipa::path(get, path = "/me", tag = "people", responses((status = OK, body = Me)))]
pub async fn me(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<ApiJson<Me>> {
    Ok(ApiJson(profile::me(&state.ctx, actor.user_id).await?))
}

/// Members, for pickers (who takes part in a vote, who paid).
#[utoipa::path(get, path = "/people", tag = "people", responses((status = OK, body = Vec<Person>)))]
pub async fn people(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
) -> ApiResult<ApiJson<Vec<Person>>> {
    Ok(ApiJson(profile::people(&state.ctx).await?))
}

/// Members whose birthday is today (Warsaw time).
#[utoipa::path(get, path = "/birthdays/today", tag = "people", responses((status = OK, body = Vec<Person>)))]
pub async fn birthdays_today(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
) -> ApiResult<ApiJson<Vec<Person>>> {
    Ok(ApiJson(profile::birthdays_today(&state.ctx).await?))
}

/// Public by design: the login screen lists members (a small, closed group). Rate-limited.
#[utoipa::path(
    get, path = "/login-users", tag = "auth", security(()),
    responses((status = OK, body = Vec<LoginUser>)),
)]
pub async fn login_users(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> ApiResult<ApiJson<Vec<LoginUser>>> {
    throttle(&state.limiters.login_users, &ip)?;
    Ok(ApiJson(profile::login_users(&state.ctx).await?))
}
