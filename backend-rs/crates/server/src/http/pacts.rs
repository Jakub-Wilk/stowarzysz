//! Umowy: bets, group bets, resolutions and shared goals.
//!
//! Every change answers with the pact as the caller now sees it. What the caller may do next is
//! in its `actions` block; clients should not re-derive it.

use app::pacts::{self, ClaimRequest, ListFilter, NewPact, PactDetail, PactPage, Respond};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::pacts::{Status, TermsInput};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::guard::AuthUser;
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    pub status: Option<Status>,
    /// Only pacts you are (or were) part of.
    #[serde(default)]
    pub mine: bool,
    /// From the previous page's `next_cursor`.
    pub cursor: Option<String>,
}

/// Pacts newest first. Every member can read every pact.
#[utoipa::path(get, path = "/pacts", tag = "pacts", params(ListParams), responses((status = OK, body = PactPage)))]
pub async fn list(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(params): Query<ListParams>,
) -> ApiResult<ApiJson<PactPage>> {
    let filter = ListFilter {
        status: params.status,
        mine: params.mine,
        cursor: params.cursor,
    };
    Ok(ApiJson(pacts::list(&state.ctx, &actor, &filter).await?))
}

/// Make a pact. Invitees get a push; a resolution starts at once and invites nobody.
#[utoipa::path(
    post, path = "/pacts", tag = "pacts",
    request_body = NewPact,
    responses(
        (status = CREATED, body = PactDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    ApiJson(request): ApiJson<NewPact>,
) -> ApiResult<(StatusCode, ApiJson<PactDetail>)> {
    let id = pacts::create(&state.ctx, &actor, request).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(pacts::detail(&state.ctx, &actor, id).await?),
    ))
}

#[utoipa::path(
    get, path = "/pacts/{id}", tag = "pacts",
    responses((status = OK, body = PactDetail), (status = NOT_FOUND, body = Problem, content_type = "application/problem+json")),
)]
pub async fn get(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<PactDetail>> {
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Accept or decline your invite (group bets: with your side and optional stake).
#[utoipa::path(
    post, path = "/pacts/{id}/respond", tag = "pacts",
    request_body = Respond,
    responses(
        (status = OK, body = PactDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, description = "Not invited", body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, description = "Already answered, or the pact is over", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn respond(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<Respond>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::respond(&state.ctx, &actor, id, &request).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Call off a pact that has not started (its creator).
#[utoipa::path(
    post, path = "/pacts/{id}/cancel", tag = "pacts",
    responses(
        (status = OK, body = PactDetail),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, description = "It has started", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn cancel(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::cancel(&state.ctx, &actor, id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Ask to join, with your terms (a bet: your stake; a group bet: your side). The host approves
/// for a bet, everyone in it for the other kinds; resolutions take nobody.
#[utoipa::path(
    post, path = "/pacts/{id}/join", tag = "pacts",
    request_body = TermsInput,
    responses(
        (status = OK, body = PactDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn join(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<TermsInput>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::join(&state.ctx, &actor, id, &request).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Take back your join request.
#[utoipa::path(
    post, path = "/pacts/{id}/withdraw", tag = "pacts",
    responses((status = OK, body = PactDetail), (status = CONFLICT, body = Problem, content_type = "application/problem+json")),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::withdraw(&state.ctx, &actor, id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecisionRequest {
    pub approve: bool,
}

/// Approve or reject a join request (one "no" rejects it; it is accepted once every approver
/// said yes).
#[utoipa::path(
    post, path = "/pacts/{id}/participants/{participant_id}/decide", tag = "pacts",
    request_body = DecisionRequest,
    responses(
        (status = OK, body = PactDetail),
        (status = FORBIDDEN, description = "Not yours to decide", body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn decide(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((id, participant_id)): Path<(i64, i64)>,
    ApiJson(request): ApiJson<DecisionRequest>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::decide(&state.ctx, &actor, id, participant_id, request.approve).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Claim how a wager (bet) or the whole pact ended; the other side confirms. Before the
/// deadline only calling it off (`{"void": true}`) and the early claims a kind allows (a
/// shared goal's `{"done": true}`) are possible. A new claim replaces a pending or disputed
/// one.
#[utoipa::path(
    post, path = "/pacts/{id}/claims", tag = "pacts",
    request_body = ClaimRequest,
    responses(
        (status = CREATED, body = PactDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn claim(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<ClaimRequest>,
) -> ApiResult<(StatusCode, ApiJson<PactDetail>)> {
    pacts::claim(&state.ctx, &actor, id, &request).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(pacts::detail(&state.ctx, &actor, id).await?),
    ))
}

/// Agree with a claim. Once everyone it needs agreed, it is settled (debts go to the ledger).
#[utoipa::path(
    post, path = "/pacts/{id}/claims/{claim_id}/confirm", tag = "pacts",
    responses(
        (status = OK, body = PactDetail),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn confirm(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((id, claim_id)): Path<(i64, i64)>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::confirm(&state.ctx, &actor, id, claim_id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Disagree with a claim: it is parked until someone claims again or puts it to a vote.
#[utoipa::path(
    post, path = "/pacts/{id}/claims/{claim_id}/dispute", tag = "pacts",
    responses(
        (status = OK, body = PactDetail),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn dispute(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((id, claim_id)): Path<(i64, i64)>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::dispute(&state.ctx, &actor, id, claim_id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Put a disputed claim to a Sejmik vote (`pact_ruling`) of the members outside the pact; a
/// strict majority upholds it.
#[utoipa::path(
    post, path = "/pacts/{id}/claims/{claim_id}/escalate", tag = "pacts",
    responses(
        (status = OK, body = PactDetail),
        (status = BAD_REQUEST, description = "Nobody impartial can vote", body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn escalate(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((id, claim_id)): Path<(i64, i64)>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::escalate(&state.ctx, &actor, id, claim_id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

/// Put a resolution to a Sejmik vote (`resolution_judgment`) of every other member. Anyone
/// may once its deadline has passed; before it only its author.
#[utoipa::path(
    post, path = "/pacts/{id}/judgment", tag = "pacts",
    responses(
        (status = OK, body = PactDetail),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn judgment(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::call_judgment(&state.ctx, &actor, id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttachmentRequest {
    /// An upload of purpose `photo`.
    pub upload_id: Uuid,
    /// Up to 200 characters.
    #[serde(default)]
    pub caption: String,
}

/// Add a picture to the pact's notes (any participant, at any stage): up to 10.
#[utoipa::path(
    post, path = "/pacts/{id}/attachments", tag = "pacts",
    request_body = AttachmentRequest,
    responses(
        (status = CREATED, body = PactDetail),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn add_attachment(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<AttachmentRequest>,
) -> ApiResult<(StatusCode, ApiJson<PactDetail>)> {
    pacts::add_attachment(&state.ctx, &actor, id, request.upload_id, &request.caption).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(pacts::detail(&state.ctx, &actor, id).await?),
    ))
}

/// Remove a picture (its uploader or the pact's creator).
#[utoipa::path(
    delete, path = "/pacts/{id}/attachments/{attachment_id}", tag = "pacts",
    responses(
        (status = OK, body = PactDetail),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn delete_attachment(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((id, attachment_id)): Path<(i64, i64)>,
) -> ApiResult<ApiJson<PactDetail>> {
    pacts::delete_attachment(&state.ctx, &actor, id, attachment_id).await?;
    Ok(ApiJson(pacts::detail(&state.ctx, &actor, id).await?))
}
