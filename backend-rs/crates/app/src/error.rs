use std::borrow::Cow;
use std::time::Duration;

use domain::DomainError;

pub type AppResult<T> = Result<T, AppError>;

/// Every way a use case can fail. The server maps each variant to one HTTP status and an
/// RFC 9457 problem document; messages are Polish because they reach the user.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The input broke one or more business rules (400).
    #[error("invalid input: {0:?}")]
    Invalid(Vec<DomainError>),
    /// No valid credentials (401).
    #[error("unauthorized: {message}")]
    Unauthorized {
        code: &'static str,
        message: Cow<'static, str>,
    },
    /// Authenticated, but not allowed to do this (403).
    #[error("forbidden: {message}")]
    Forbidden {
        code: &'static str,
        message: Cow<'static, str>,
    },
    /// The resource does not exist or is not visible to the caller (404).
    #[error("not found")]
    NotFound,
    /// The request clashes with the current state, e.g. a stale `version` (409).
    #[error("conflict: {message}")]
    Conflict {
        code: &'static str,
        message: Cow<'static, str>,
    },
    /// The request body is larger than allowed (413).
    #[error("payload too large: {message}")]
    TooLarge { message: Cow<'static, str> },
    /// The body is not of an accepted media type (415).
    #[error("unsupported media type: {message}")]
    UnsupportedMedia { message: Cow<'static, str> },
    /// Rate limit exceeded (429).
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    /// A dependency (exchange rates, OCR) is unavailable right now (503).
    #[error("unavailable: {message}")]
    Unavailable {
        code: &'static str,
        message: Cow<'static, str>,
    },
    /// A bug or an infrastructure failure (500). Details are logged, never shown.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn forbidden(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::Forbidden {
            code,
            message: message.into(),
        }
    }

    pub fn conflict(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::Unauthorized {
            code,
            message: message.into(),
        }
    }

    pub fn unavailable(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self::Unavailable {
            code,
            message: message.into(),
        }
    }
}

impl From<DomainError> for AppError {
    fn from(error: DomainError) -> Self {
        Self::Invalid(vec![error])
    }
}

impl From<Vec<DomainError>> for AppError {
    fn from(errors: Vec<DomainError>) -> Self {
        Self::Invalid(errors)
    }
}
