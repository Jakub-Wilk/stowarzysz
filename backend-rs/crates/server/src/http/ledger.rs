//! Kasa Skarbowa: the group's expenses, debts and payments.

use app::ledger::{
    self, Balances, EntryFilter, EntryPage, EntryUpdate, EntryView, Meta, NewEntry, RateView,
    Stats, StatsQuery,
};
use app::ocr::OcrResult;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::ledger::EntryKind;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::guard::{AuthUser, throttle};
use super::json::ApiJson;
use super::problem::{ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    pub kind: Option<EntryKind>,
    /// One pact's settlements (returned unpaged).
    pub pact_id: Option<i64>,
    /// From the previous page's `next_cursor`.
    pub cursor: Option<String>,
}

/// The feed, newest first (by the day it happened), 30 per page. Everyone reads every entry.
#[utoipa::path(get, path = "/ledger/entries", tag = "ledger", params(ListParams), responses((status = OK, body = EntryPage)))]
pub async fn list(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(params): Query<ListParams>,
) -> ApiResult<ApiJson<EntryPage>> {
    let filter = EntryFilter {
        kind: params.kind,
        pact_id: params.pact_id,
        cursor: params.cursor,
    };
    Ok(ApiJson(ledger::list(&state.ctx, &actor, &filter).await?))
}

/// Record an expense, an income, a debt or a payment (tagged by `kind`). Amounts are integers in
/// the currency's minor units (grosze) or, for goods, pieces.
#[utoipa::path(
    post, path = "/ledger/entries", tag = "ledger",
    request_body = NewEntry,
    responses(
        (status = CREATED, body = EntryView),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = SERVICE_UNAVAILABLE, description = "No exchange rate right now", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    ApiJson(request): ApiJson<NewEntry>,
) -> ApiResult<(StatusCode, ApiJson<EntryView>)> {
    let id = ledger::create(&state.ctx, &actor, request).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(ledger::detail(&state.ctx, &actor, id).await?),
    ))
}

#[utoipa::path(
    get, path = "/ledger/entries/{id}", tag = "ledger",
    responses((status = OK, body = EntryView), (status = NOT_FOUND, body = Problem, content_type = "application/problem+json")),
)]
pub async fn get(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<EntryView>> {
    Ok(ApiJson(ledger::detail(&state.ctx, &actor, id).await?))
}

/// Replace an entry's content (expenses and incomes: anyone; manual debts: the person owed).
/// Send the `version` you edited; if somebody saved in between you get a 409 and should reload.
#[utoipa::path(
    put, path = "/ledger/entries/{id}", tag = "ledger",
    request_body = EntryUpdate,
    responses(
        (status = OK, body = EntryView),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, description = "Outdated `version`, or not editable", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<EntryUpdate>,
) -> ApiResult<ApiJson<EntryView>> {
    ledger::update(&state.ctx, &actor, id, request).await?;
    Ok(ApiJson(ledger::detail(&state.ctx, &actor, id).await?))
}

/// Confirm a payment you received: it now counts.
#[utoipa::path(
    post, path = "/ledger/entries/{id}/confirm", tag = "ledger",
    responses((status = OK, body = EntryView), (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"), (status = CONFLICT, body = Problem, content_type = "application/problem+json")),
)]
pub async fn confirm(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<EntryView>> {
    ledger::confirm(&state.ctx, &actor, id).await?;
    Ok(ApiJson(ledger::detail(&state.ctx, &actor, id).await?))
}

/// Reject a payment you never received: it will never count.
#[utoipa::path(
    post, path = "/ledger/entries/{id}/reject", tag = "ledger",
    responses((status = OK, body = EntryView), (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"), (status = CONFLICT, body = Problem, content_type = "application/problem+json")),
)]
pub async fn reject(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<EntryView>> {
    ledger::reject(&state.ctx, &actor, id).await?;
    Ok(ApiJson(ledger::detail(&state.ctx, &actor, id).await?))
}

/// Delete an expense or a manual debt, or take back your payment. The entry stays, cancelled.
#[utoipa::path(
    post, path = "/ledger/entries/{id}/cancel", tag = "ledger",
    responses((status = OK, body = EntryView), (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"), (status = CONFLICT, body = Problem, content_type = "application/problem+json")),
)]
pub async fn cancel(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<ApiJson<EntryView>> {
    ledger::cancel(&state.ctx, &actor, id).await?;
    Ok(ApiJson(ledger::detail(&state.ctx, &actor, id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttachmentRequest {
    /// An upload of purpose `photo`.
    pub upload_id: Uuid,
}

/// Add a picture (a receipt, proof of a transfer): up to 10 per entry.
#[utoipa::path(
    post, path = "/ledger/entries/{id}/attachments", tag = "ledger",
    request_body = AttachmentRequest,
    responses((status = CREATED, body = EntryView), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"), (status = FORBIDDEN, body = Problem, content_type = "application/problem+json")),
)]
pub async fn add_attachment(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    ApiJson(request): ApiJson<AttachmentRequest>,
) -> ApiResult<(StatusCode, ApiJson<EntryView>)> {
    ledger::add_attachment(&state.ctx, &actor, id, request.upload_id).await?;
    Ok((
        StatusCode::CREATED,
        ApiJson(ledger::detail(&state.ctx, &actor, id).await?),
    ))
}

/// Remove a picture (its uploader or the entry's author).
#[utoipa::path(
    delete, path = "/ledger/entries/{id}/attachments/{attachment_id}", tag = "ledger",
    responses((status = OK, body = EntryView), (status = FORBIDDEN, body = Problem, content_type = "application/problem+json"), (status = NOT_FOUND, body = Problem, content_type = "application/problem+json")),
)]
pub async fn delete_attachment(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((id, attachment_id)): Path<(i64, i64)>,
) -> ApiResult<ApiJson<EntryView>> {
    ledger::delete_attachment(&state.ctx, &actor, id, attachment_id).await?;
    Ok(ApiJson(ledger::detail(&state.ctx, &actor, id).await?))
}

/// The group's Bilans: everyone's net (confirmed entries) and who should pay whom.
#[utoipa::path(get, path = "/ledger/balances", tag = "ledger", responses((status = OK, body = Balances)))]
pub async fn balances(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
) -> ApiResult<ApiJson<Balances>> {
    Ok(ApiJson(ledger::balances(&state.ctx).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RateParams {
    pub currency: String,
    /// Defaults to today.
    pub date: Option<NaiveDate>,
}

/// The rate an entry in `currency` on `date` will use. Asking early also warms the cache, so
/// saving does not wait on the network.
#[utoipa::path(
    get, path = "/ledger/rates", tag = "ledger", params(RateParams),
    responses(
        (status = OK, body = RateView),
        (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json"),
        (status = SERVICE_UNAVAILABLE, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn rates(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
    Query(params): Query<RateParams>,
) -> ApiResult<ApiJson<RateView>> {
    let day = params
        .date
        .unwrap_or_else(|| app::clock::local_today(state.ctx.clock.now()));
    Ok(ApiJson(
        ledger::rate(&state.ctx, &params.currency, day).await?,
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct StatsParams {
    /// First day; omit for the beginning.
    pub start: Option<NaiveDate>,
    /// Last day; omit for the latest entry.
    pub end: Option<NaiveDate>,
}

/// Spending in a period: totals, by category, by person and over time (all time without
/// dates). In grosze.
#[utoipa::path(
    get, path = "/ledger/stats", tag = "ledger", params(StatsParams),
    responses((status = OK, body = Stats), (status = BAD_REQUEST, body = Problem, content_type = "application/problem+json")),
)]
pub async fn stats(
    State(state): State<AppState>,
    AuthUser(_caller): AuthUser,
    Query(params): Query<StatsParams>,
) -> ApiResult<ApiJson<Stats>> {
    let query = StatsQuery {
        start: params.start,
        end: params.end,
    };
    Ok(ApiJson(ledger::stats(&state.ctx, query).await?))
}

/// What the forms offer: currencies (with their minor-unit digits) and categories.
#[utoipa::path(get, path = "/ledger/meta", tag = "ledger", responses((status = OK, body = Meta)))]
pub async fn meta(AuthUser(_caller): AuthUser) -> ApiJson<Meta> {
    ApiJson(ledger::meta())
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OcrParams {
    /// Chosen by the client, so it can tell which `ledger.ocr.request` events are its own.
    pub job_id: Uuid,
}

/// Read a receipt photo (the raw image as the body, at most 50 MB) into structured fields;
/// nothing is stored. Blocks until the result is ready (up to `OCR_DEADLINE`). Each request to
/// a model is announced to the caller over SSE as `ledger.ocr.request {job_id, attempt,
/// candidate}`; the response is authoritative.
#[utoipa::path(
    post, path = "/ledger/receipt-ocr", tag = "ledger",
    params(OcrParams),
    request_body(content = Vec<u8>, content_type = "image/*", description = "The receipt photo"),
    responses(
        (status = OK, body = OcrResult),
        (status = BAD_REQUEST, description = "Not an image, or unreadable", body = Problem, content_type = "application/problem+json"),
        (status = CONFLICT, description = "Your previous scan is still running", body = Problem, content_type = "application/problem+json"),
        (status = PAYLOAD_TOO_LARGE, body = Problem, content_type = "application/problem+json"),
        (status = TOO_MANY_REQUESTS, body = Problem, content_type = "application/problem+json"),
        (status = SERVICE_UNAVAILABLE, description = "No model answered in time, or OCR is not configured", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn receipt_ocr(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(params): Query<OcrParams>,
    request: Request,
) -> ApiResult<ApiJson<OcrResult>> {
    throttle(&state.limiters.ocr, &actor.user_id.to_string())?;
    let path = super::media::receive_image(&state, request).await?;
    Ok(ApiJson(
        app::ocr::scan(&state.ctx, &actor, params.job_id, path).await?,
    ))
}
