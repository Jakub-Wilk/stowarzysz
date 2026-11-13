//! Secret Santa. Everyone sees whether a round is on (and their own victims); superusers start,
//! adjust and end it.

use app::santa::{
    self, AnswerRequest, GiftNote, GiftView, HelpRequest, HistoryPage, SantaState, StartRequest,
    UpdateRequest,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;

use super::guard::{AdminUser, AuthUser};
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

/// The current round as you see it: only your own victims, only requests about you.
#[utoipa::path(get, path = "/secret-santa", tag = "secret-santa", responses((status = OK, body = SantaState)))]
pub async fn get(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<ApiJson<SantaState>> {
    Ok(ApiJson(santa::state(&state.ctx, &actor).await?))
}

/// Start a round and draw it (superusers). Every participant is pushed whom they drew.
#[utoipa::path(
    post, path = "/secret-santa", tag = "secret-santa",
    request_body = StartRequest,
    responses(
        (status = CREATED, body = SantaState),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, description = "A round is already on", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    ApiJson(request): ApiJson<StartRequest>,
) -> ApiResult<(StatusCode, ApiJson<SantaState>)> {
    santa::start(&state.ctx, &actor, &request).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(santa::state(&state.ctx, &actor).await?),
    ))
}

/// Change the deadline or the amounts of the current round (superusers); the draw stays.
#[utoipa::path(
    patch, path = "/secret-santa", tag = "secret-santa",
    request_body = UpdateRequest,
    responses(
        (status = OK, body = SantaState),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, description = "No round is on", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    ApiJson(request): ApiJson<UpdateRequest>,
) -> ApiResult<ApiJson<SantaState>> {
    santa::update(&state.ctx, &request).await?;
    Ok(ApiJson(santa::state(&state.ctx, &actor).await?))
}

/// End the current round (superusers): its pairings become public history.
#[utoipa::path(
    post, path = "/secret-santa/end", tag = "secret-santa",
    responses(
        (status = OK, body = SantaState),
        (status = NOT_FOUND, description = "No round is on", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn end(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
) -> ApiResult<ApiJson<SantaState>> {
    santa::end(&state.ctx).await?;
    Ok(ApiJson(santa::state(&state.ctx, &actor).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct HistoryParams {
    /// From the previous page's `next_cursor`.
    pub cursor: Option<String>,
}

/// Ended rounds with who gave whom what, newest first.
#[utoipa::path(get, path = "/secret-santa/history", tag = "secret-santa", params(HistoryParams), responses((status = OK, body = HistoryPage)))]
pub async fn history(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
    Query(params): Query<HistoryParams>,
) -> ApiResult<ApiJson<HistoryPage>> {
    Ok(ApiJson(
        santa::history(&state.ctx, params.cursor.as_deref()).await?,
    ))
}

/// Ask one of your victims, anonymously, for gift ideas.
#[utoipa::path(
    post, path = "/secret-santa/help", tag = "secret-santa",
    request_body = HelpRequest,
    responses(
        (status = CREATED, body = SantaState),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, description = "No round is on", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn ask_for_help(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    ApiJson(request): ApiJson<HelpRequest>,
) -> ApiResult<(StatusCode, ApiJson<SantaState>)> {
    santa::ask_for_help(&state.ctx, &actor, request.victim_id).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(santa::state(&state.ctx, &actor).await?),
    ))
}

/// Answer a request for ideas about you.
#[utoipa::path(
    put, path = "/secret-santa/help/{id}", tag = "secret-santa",
    request_body = AnswerRequest,
    responses(
        (status = OK, body = SantaState),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn answer_help(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<AnswerRequest>,
) -> ApiResult<ApiJson<SantaState>> {
    santa::answer_help(&state.ctx, &actor, id, &request.ideas).await?;
    Ok(ApiJson(santa::state(&state.ctx, &actor).await?))
}

/// Note what you gave (or, as a superuser, what anyone gave) in an ended round.
#[utoipa::path(
    patch, path = "/secret-santa/gifts/{id}", tag = "secret-santa",
    request_body = GiftNote,
    responses(
        (status = OK, body = GiftView),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn gift_note(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<GiftNote>,
) -> ApiResult<ApiJson<GiftView>> {
    Ok(ApiJson(
        santa::set_gift_note(&state.ctx, &actor, id, &request.note).await?,
    ))
}
