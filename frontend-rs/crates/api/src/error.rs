//! Errors as the UI shows them: backend-rs sends RFC 9457 problem documents with Polish text.

use std::collections::HashMap;
use std::fmt;

use crate::types::Problem;

/// Shown when nothing better is known.
pub const FALLBACK_MESSAGE: &str = "Coś poszło nie tak. Spróbuj ponownie.";
const NETWORK_MESSAGE: &str = "Brak połączenia z serwerem. Sprawdź internet i spróbuj ponownie.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// The server answered with an error (a problem document, or something we could not read).
    Http {
        status: u16,
        /// Stable machine key (`not_found`, `invalid_credentials`, ...); empty if unknown.
        code: String,
        /// Polish text for the user.
        detail: String,
        /// Polish messages per input field.
        errors: HashMap<String, Vec<String>>,
    },
    /// No answer: offline, DNS, TLS, a dropped connection.
    Network(String),
    /// An answer we could not decode (a contract mismatch: refresh `openapi.json`).
    Decode(String),
    /// There is no session (never signed in, signed out, or the session expired).
    SignedOut,
}

impl ApiError {
    /// Build from a non-success response body.
    pub(crate) fn from_body(status: u16, body: &[u8]) -> Self {
        match serde_json::from_slice::<Problem>(body) {
            Ok(problem) => Self::Http {
                status,
                code: problem.code,
                detail: problem.detail,
                errors: problem.errors.unwrap_or_default(),
            },
            Err(_) => Self::Http {
                status,
                code: String::new(),
                detail: FALLBACK_MESSAGE.to_owned(),
                errors: HashMap::new(),
            },
        }
    }

    pub(crate) fn network(error: &reqwest::Error) -> Self {
        Self::Network(error.to_string())
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// The problem's `code`, if the server sent one.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Http { code, .. } if !code.is_empty() => Some(code),
            _ => None,
        }
    }

    /// The first message for `field`, for showing under a form input.
    pub fn field(&self, field: &str) -> Option<&str> {
        match self {
            Self::Http { errors, .. } => errors.get(field)?.first().map(String::as_str),
            _ => None,
        }
    }

    /// Polish text for the user.
    pub fn message(&self) -> &str {
        match self {
            Self::Http { detail, .. } if !detail.is_empty() => detail,
            Self::Network(_) => NETWORK_MESSAGE,
            Self::SignedOut => "Sesja wygasła. Zaloguj się ponownie.",
            _ => FALLBACK_MESSAGE,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http { status, code, .. } => {
                write!(f, "HTTP {status} {code}: {}", self.message())
            }
            Self::Network(error) => write!(f, "network error: {error}"),
            Self::Decode(error) => write!(f, "unexpected response: {error}"),
            Self::SignedOut => f.write_str("signed out"),
        }
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_problem_document() {
        let body =
            r#"{"type":"urn:stowarzysz:problem:invalid","title":"Nieprawidlowe dane","status":400,
            "detail":"Popraw zaznaczone pola.","code":"invalid","errors":{"title":["Za krótki."]}}"#
                .as_bytes();
        let error = ApiError::from_body(400, body);
        assert_eq!(error.status(), Some(400));
        assert_eq!(error.code(), Some("invalid"));
        assert_eq!(error.message(), "Popraw zaznaczone pola.");
        assert_eq!(error.field("title"), Some("Za krótki."));
        assert_eq!(error.field("other"), None);
    }

    #[test]
    fn falls_back_on_an_unreadable_body() {
        let error = ApiError::from_body(502, b"<html>Bad Gateway</html>");
        assert_eq!(error.status(), Some(502));
        assert_eq!(error.code(), None);
        assert_eq!(error.message(), FALLBACK_MESSAGE);
    }
}
