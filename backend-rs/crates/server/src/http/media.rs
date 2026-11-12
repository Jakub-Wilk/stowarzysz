//! Uploading images and serving stored ones.
#![allow(unused_qualifications)] // emitted inside utoipa's `params(...)` expansion

use app::AppError;
use app::media::{self, Purpose, UploadView, Variant};
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt as _;
use serde::Deserialize;
use tokio::io::AsyncWriteExt as _;
use utoipa::IntoParams;

use super::guard::{AuthUser, throttle};
use super::json::ApiJson;
use super::problem::{ApiError, ApiResult, Problem};
use crate::AppState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct UploadParams {
    /// `avatar` (square) or `photo` (aspect ratio kept).
    pub purpose: Purpose,
}

/// Upload an image (the raw file as the body, at most 50 MB; JPEG, PNG, WebP or GIF). It is
/// stored as a lossless WebP plus smaller previews, and kept for 30 minutes unless used by id
/// (e.g. `upload_id` when setting an avatar or adding an attachment).
#[utoipa::path(
    post, path = "/uploads", tag = "media",
    params(UploadParams),
    request_body(content = Vec<u8>, content_type = "image/*", description = "The image file"),
    responses(
        (status = CREATED, body = UploadView),
        (status = BAD_REQUEST, description = "Not a valid image", body = Problem, content_type = "application/problem+json"),
        (status = PAYLOAD_TOO_LARGE, body = Problem, content_type = "application/problem+json"),
        (status = UNSUPPORTED_MEDIA_TYPE, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn upload(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(params): Query<UploadParams>,
    request: Request,
) -> ApiResult<(StatusCode, ApiJson<UploadView>)> {
    throttle(&state.limiters.uploads, &actor.user_id.to_string())?;
    let limit = state.ctx.config.limits.upload_bytes;
    let too_large = || {
        ApiError(AppError::TooLarge {
            message: format!("Obraz jest za duży (maks. {} MB).", limit / (1024 * 1024)).into(),
        })
    };
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !(content_type.starts_with("image/") || content_type.starts_with("application/octet-stream"))
    {
        return Err(ApiError(AppError::UnsupportedMedia {
            message: "Prześlij plik obrazu (nagłówek Content-Type: image/...).".into(),
        }));
    }
    let declared = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|length| length > limit) {
        return Err(too_large());
    }

    // Stream the body to a scratch file (deleted when dropped, whatever happens) so a 50 MB
    // upload never sits in memory.
    let scratch =
        tempfile::NamedTempFile::new_in(state.ctx.media.tmp_dir()).map_err(anyhow::Error::from)?;
    let (file, path) = scratch.into_parts();
    let mut file = tokio::fs::File::from_std(file);
    let mut received = 0_usize;
    let mut body = request.into_body().into_data_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| {
            ApiError(AppError::from(domain::DomainError::new(
                "upload_interrupted",
                "Przesyłanie zostało przerwane.",
            )))
        })?;
        received += chunk.len();
        if received > limit {
            return Err(too_large());
        }
        file.write_all(&chunk).await.map_err(anyhow::Error::from)?;
    }
    if received == 0 {
        return Err(ApiError(
            domain::DomainError::new("image_missing", "Prześlij plik obrazu.").into(),
        ));
    }
    file.flush().await.map_err(anyhow::Error::from)?;
    drop(file);

    let view = media::store_upload(&state.ctx, actor.user_id, params.purpose, path).await?;
    Ok((StatusCode::CREATED, ApiJson(view)))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct Signature {
    /// Expiry (days since the Unix epoch).
    pub e: i64,
    /// The signature.
    pub s: String,
}

/// A stored image file. The URL (with its signature) comes from the API, e.g. `avatar.sm`; it
/// stays valid for at least a day and needs no `Authorization` header, so it works in `<img>`.
#[utoipa::path(
    get, path = "/media/{prefix}/{id}/{file}", tag = "media", security(()),
    params(
        ("prefix" = String, Path, description = "`avatars` or `photos`"),
        ("id" = String, Path),
        ("file" = String, Path, description = "`master.webp`, `md.webp` or `sm.webp`"),
        Signature,
    ),
    responses(
        (status = OK, content_type = "image/webp", body = Vec<u8>),
        (status = FORBIDDEN, description = "Bad or expired signature", body = Problem, content_type = "application/problem+json"),
        (status = NOT_FOUND, body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn serve(
    State(state): State<AppState>,
    Path((prefix, id, file)): Path<(String, String, String)>,
    Query(signature): Query<Signature>,
) -> ApiResult<Response> {
    let (Some(key), Some(variant)) = (media::parse_key(&prefix, &id), Variant::from_file(&file))
    else {
        return Err(AppError::NotFound.into());
    };
    let now = state.ctx.clock.now();
    let valid_for = state
        .ctx
        .media
        .verify(&key, variant.file(), signature.e, &signature.s, now)
        .ok_or_else(|| {
            AppError::forbidden(
                "invalid_media_link",
                "Link do pliku jest nieprawidłowy lub wygasł.",
            )
        })?;
    let path = state.ctx.media.file(&key, variant);
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::NotFound.into());
        }
        Err(error) => return Err(anyhow::Error::from(error).into()),
    };
    let length = file.metadata().await.map_err(anyhow::Error::from)?.len();
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    headers.insert(header::CONTENT_LENGTH, length.into());
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // The bytes behind a key never change; the URL changes daily with its signature.
    let max_age = valid_for.clamp(0, 2 * 86_400);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_str(&format!("private, max-age={max_age}, immutable"))
            .map_err(anyhow::Error::from)?,
    );
    Ok(response)
}
