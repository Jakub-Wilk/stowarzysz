//! Sejmik: votes.

use app::polls::{self, ListFilter, NewPoll, PollDetail, PollPage};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::polls::BallotInput;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use super::guard::AuthUser;
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    /// `open` (all of them, unpaged) or `closed`; both when absent.
    pub status: Option<String>,
    /// Only polls you take part in.
    #[serde(default)]
    pub participating: bool,
    /// From the previous page's `next_cursor`.
    pub cursor: Option<String>,
}

/// Polls newest first. Everyone can read every poll; ballots stay hidden until a poll ends.
#[utoipa::path(get, path = "/polls", tag = "polls", params(ListParams), responses((status = OK, body = PollPage)))]
pub async fn list(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(params): Query<ListParams>,
) -> ApiResult<ApiJson<PollPage>> {
    let filter = ListFilter {
        status: params.status,
        participating: params.participating,
        cursor: params.cursor,
    };
    Ok(ApiJson(polls::list(&state.ctx, &actor, &filter).await?))
}

/// Call a vote. The request is tagged by `kind`; profile votes (`nickname`, `avatar`) take
/// every member as voters and get a generated title.
#[utoipa::path(
    post, path = "/polls", tag = "polls",
    request_body = NewPoll,
    responses(
        (status = CREATED, body = PollDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    ApiJson(request): ApiJson<NewPoll>,
) -> ApiResult<(StatusCode, ApiJson<PollDetail>)> {
    let id = polls::create(&state.ctx, &actor, request).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(polls::detail(&state.ctx, &actor, id).await?),
    ))
}

#[utoipa::path(
    get, path = "/polls/{id}", tag = "polls",
    responses((status = OK, body = PollDetail), (status = NOT_FOUND, body = Problem, content_type = "application/problem+json")),
)]
pub async fn get(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<PollDetail>> {
    Ok(ApiJson(polls::detail(&state.ctx, &actor, id).await?))
}

/// A ballot. Send the field of the poll's kind: `value` (-5..5) for score and profile votes,
/// `choice` / `choices` / `ranking` (option ids) for option votes, `upheld` or `kept` for pact
/// votes. With an open option pool, `new_option` votes for a new option (added if new).
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct BallotRequest {
    pub value: Option<i64>,
    pub choice: Option<String>,
    pub choices: Option<Vec<String>>,
    pub ranking: Option<Vec<String>>,
    pub upheld: Option<bool>,
    pub kept: Option<bool>,
    pub new_option: Option<String>,
}

impl From<BallotRequest> for BallotInput {
    fn from(r: BallotRequest) -> Self {
        Self {
            value: r.value,
            choice: r.choice,
            choices: r.choices,
            ranking: r.ranking,
            upheld: r.upheld,
            kept: r.kept,
            new_option: r.new_option,
        }
    }
}

/// Cast or change your ballot (replaces an abstention; a veto is final).
#[utoipa::path(
    put, path = "/polls/{id}/ballot", tag = "polls",
    request_body = BallotRequest,
    responses(
        (status = OK, body = PollDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, description = "Not a participant", body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, description = "The poll has ended", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn ballot(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<BallotRequest>,
) -> ApiResult<ApiJson<PollDetail>> {
    polls::cast_ballot(&state.ctx, &actor, id, &request.into()).await?;
    Ok(ApiJson(polls::detail(&state.ctx, &actor, id).await?))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ReasonRequest {
    /// Optional, up to 300 characters; shown once the poll has ended.
    #[serde(default)]
    pub reason: String,
}

/// Veto (score votes): counts as -5 and marks the result vetoed. Cannot be undone.
#[utoipa::path(
    post, path = "/polls/{id}/veto", tag = "polls",
    request_body = ReasonRequest,
    responses((status = OK, body = PollDetail), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn veto(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<ReasonRequest>,
) -> ApiResult<ApiJson<PollDetail>> {
    polls::veto(&state.ctx, &actor, id, &request.reason).await?;
    Ok(ApiJson(polls::detail(&state.ctx, &actor, id).await?))
}

/// Abstain: you are left out of the result (not the same as a zero). Voting later replaces it.
#[utoipa::path(
    post, path = "/polls/{id}/abstain", tag = "polls",
    request_body = ReasonRequest,
    responses((status = OK, body = PollDetail), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn abstain(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<ReasonRequest>,
) -> ApiResult<ApiJson<PollDetail>> {
    polls::abstain(&state.ctx, &actor, id, &request.reason).await?;
    Ok(ApiJson(polls::detail(&state.ctx, &actor, id).await?))
}

/// End the vote now (its creator; profile and pact votes cannot be ended early).
#[utoipa::path(
    post, path = "/polls/{id}/close", tag = "polls",
    responses((status = OK, body = PollDetail), (status = FORBIDDEN, body = Problem, content_type = "application/problem+json")),
)]
pub async fn close(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<PollDetail>> {
    polls::close_early(&state.ctx, &actor, id).await?;
    Ok(ApiJson(polls::detail(&state.ctx, &actor, id).await?))
}

/// Call a follow-up vote between the options a finished vote tied on (its creator). Asking
/// again returns the existing follow-up (200).
#[utoipa::path(
    post, path = "/polls/{id}/tie-break", tag = "polls",
    responses(
        (status = CREATED, description = "The follow-up vote", body = PollDetail),
        (status = OK, description = "The follow-up called earlier", body = PollDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn tie_break(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<(StatusCode, ApiJson<PollDetail>)> {
    let (follow_up, created) = polls::tie_break(&state.ctx, &actor, id).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        ApiJson(polls::detail(&state.ctx, &actor, follow_up).await?),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReactionRequest {
    /// One of ❤️ 🔥 😭 👎 🤣.
    pub emoji: String,
}

/// React to a finished vote (unthrottled by design; everyone gets a `poll.reaction` event).
#[utoipa::path(
    post, path = "/polls/{id}/reactions", tag = "polls",
    request_body = ReactionRequest,
    responses((status = NO_CONTENT), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn react(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<ReactionRequest>,
) -> ApiResult<StatusCode> {
    polls::react(&state.ctx, &actor, id, &request.emoji).await?;
    Ok(StatusCode::NO_CONTENT)
}
