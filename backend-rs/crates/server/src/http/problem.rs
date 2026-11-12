//! RFC 9457 problem documents: the single error format of the API.

use std::borrow::Cow;
use std::collections::BTreeMap;

use app::AppError;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use domain::DomainError;
use serde::Serialize;
use utoipa::ToSchema;

pub const CONTENT_TYPE: &str = "application/problem+json";

/// The body of every error response.
#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    /// `urn:stowarzysz:problem:<code>`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Short Polish summary of the status.
    pub title: String,
    pub status: u16,
    /// Polish explanation for the user.
    pub detail: String,
    /// Stable machine-readable key, e.g. `not_found` or `poll_closed`.
    pub code: String,
    /// Polish messages per input field (only for invalid input).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<BTreeMap<String, Vec<String>>>,
}

impl Problem {
    pub fn new(status: StatusCode, code: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: format!("urn:stowarzysz:problem:{code}"),
            title: title(status).to_owned(),
            status: status.as_u16(),
            detail: detail.into(),
            code: code.to_owned(),
            errors: None,
        }
    }

    /// The generic problem for a bare status (used for errors produced outside handlers).
    pub fn for_status(status: StatusCode) -> Self {
        let (code, detail) = match status {
            StatusCode::NOT_FOUND => ("not_found", "Nie znaleziono."),
            StatusCode::METHOD_NOT_ALLOWED => (
                "method_not_allowed",
                "Ta metoda nie jest obsługiwana dla tego adresu.",
            ),
            StatusCode::REQUEST_TIMEOUT => (
                "request_timeout",
                "Przekroczono czas obsługi żądania. Spróbuj ponownie.",
            ),
            StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "Treść żądania jest za duża."),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => (
                "unsupported_media_type",
                "Nieobsługiwany format treści żądania.",
            ),
            StatusCode::TOO_MANY_REQUESTS => (
                "rate_limited",
                "Zbyt wiele żądań. Spróbuj ponownie za chwilę.",
            ),
            s if s.is_server_error() => ("internal", "Wystąpił nieoczekiwany błąd serwera."),
            _ => ("bad_request", "Nieprawidłowe żądanie."),
        };
        Self::new(status, code, detail)
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, axum::Json(self)).into_response();
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE));
        response
    }
}

fn title(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 => "Nieprawidłowe dane",
        401 => "Brak uwierzytelnienia",
        403 => "Brak uprawnień",
        404 => "Nie znaleziono",
        405 => "Niedozwolona metoda",
        408 => "Przekroczono czas",
        409 => "Konflikt",
        413 => "Za duże żądanie",
        415 => "Nieobsługiwany format",
        429 => "Zbyt wiele żądań",
        503 => "Usługa niedostępna",
        s if s >= 500 => "Błąd serwera",
        _ => "Błąd żądania",
    }
}

/// The error type of every handler: wraps [`AppError`] and renders it as a [`Problem`].
#[derive(Debug)]
pub struct ApiError(pub AppError);

pub type ApiResult<T> = Result<T, ApiError>;

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self(error)
    }
}

impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self.0 {
            AppError::Invalid(errors) => invalid(errors).into_response(),
            AppError::Unauthorized { code, message } => {
                let mut response =
                    Problem::new(StatusCode::UNAUTHORIZED, code, message).into_response();
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
                response
            }
            AppError::Forbidden { code, message } => {
                Problem::new(StatusCode::FORBIDDEN, code, message).into_response()
            }
            AppError::NotFound => Problem::for_status(StatusCode::NOT_FOUND).into_response(),
            AppError::Conflict { code, message } => {
                Problem::new(StatusCode::CONFLICT, code, message).into_response()
            }
            AppError::TooLarge { message } => {
                Problem::new(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", message)
                    .into_response()
            }
            AppError::UnsupportedMedia { message } => Problem::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                message,
            )
            .into_response(),
            AppError::RateLimited { retry_after } => {
                let mut response =
                    Problem::for_status(StatusCode::TOO_MANY_REQUESTS).into_response();
                if let Some(wait) = retry_after {
                    let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
                    response
                        .headers_mut()
                        .insert(header::RETRY_AFTER, seconds.into());
                }
                response
            }
            AppError::Unavailable { code, message } => {
                Problem::new(StatusCode::SERVICE_UNAVAILABLE, code, message).into_response()
            }
            AppError::Internal(error) => {
                tracing::error!(error = format!("{error:#}"), "internal error");
                Problem::for_status(StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
        }
    }
}

/// One 400 problem for any number of rule violations: field messages go to `errors`,
/// the rest to `detail`.
fn invalid(errors: Vec<DomainError>) -> Problem {
    let code = match errors.as_slice() {
        [single] => single.code,
        _ => "invalid",
    };
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut general: Vec<Cow<'static, str>> = Vec::new();
    for error in errors {
        match error.field {
            Some(field) => fields
                .entry(field.into_owned())
                .or_default()
                .push(error.message.into()),
            None => general.push(error.message),
        }
    }
    let detail = if general.is_empty() {
        "Popraw zaznaczone pola.".to_owned()
    } else {
        general.join(" ")
    };
    let mut problem = Problem::new(StatusCode::BAD_REQUEST, code, detail);
    if !fields.is_empty() {
        problem.errors = Some(fields);
    }
    problem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_splits_field_and_general_messages() {
        let problem = invalid(vec![
            DomainError::new("title_required", "Podaj tytuł.").on("title"),
            DomainError::new("too_many", "Za dużo opcji."),
        ]);
        assert_eq!(problem.status, 400);
        assert_eq!(problem.code, "invalid");
        assert_eq!(problem.detail, "Za dużo opcji.");
        assert_eq!(problem.errors.unwrap()["title"], ["Podaj tytuł."]);
    }

    #[test]
    fn a_single_violation_keeps_its_code() {
        let problem = invalid(vec![
            DomainError::new("title_required", "Podaj tytuł.").on("title"),
        ]);
        assert_eq!(problem.code, "title_required");
        assert_eq!(problem.kind, "urn:stowarzysz:problem:title_required");
        assert_eq!(problem.detail, "Popraw zaznaczone pola.");
    }
}
