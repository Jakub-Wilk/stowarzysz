//! `ApiJson<T>`: axum's JSON extractor/response, with rejections rendered as problems.

use app::AppError;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::problem::{ApiError, Problem};

/// Use instead of `axum::Json` in handlers so malformed bodies produce problem documents.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    axum::Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(reject(&rejection)),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiJson<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

fn reject(rejection: &JsonRejection) -> Response {
    match rejection {
        JsonRejection::MissingJsonContentType(_) => ApiError(AppError::UnsupportedMedia {
            message: "Wymagany jest nagłówek Content-Type: application/json.".into(),
        })
        .into_response(),
        JsonRejection::JsonSyntaxError(_) => Problem::new(
            StatusCode::BAD_REQUEST,
            "malformed_json",
            "Treść żądania nie jest poprawnym JSON-em.",
        )
        .into_response(),
        JsonRejection::JsonDataError(error) => {
            // serde's explanation is English and meant for client developers; the Polish
            // `detail` is what a user could see.
            let mut problem = Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_body",
                "Treść żądania ma nieprawidłową strukturę.",
            );
            problem.errors = Some([("body".to_owned(), vec![error.body_text()])].into());
            problem.into_response()
        }
        other if other.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            Problem::for_status(StatusCode::PAYLOAD_TOO_LARGE).into_response()
        }
        other => Problem::for_status(other.status()).into_response(),
    }
}
