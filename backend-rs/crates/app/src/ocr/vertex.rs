//! Access tokens for Vertex AI from a service-account key (OAuth 2.0 JWT bearer grant), cached
//! until five minutes before they expire.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const REFRESH_EARLY: Duration = Duration::from_mins(5);

/// The parts of a service-account JSON key that matter here (Google's field names).
#[derive(Deserialize)]
#[allow(clippy::struct_field_names)]
struct Key {
    client_email: String,
    private_key: String,
    token_uri: String,
}

pub struct VertexAuth {
    email: String,
    signing: jsonwebtoken::EncodingKey,
    token_uri: String,
    cached: tokio::sync::Mutex<Option<(String, Instant)>>,
}

impl std::fmt::Debug for VertexAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VertexAuth")
            .field("email", &self.email)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    expires_in: u64,
}

impl VertexAuth {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let key: Key = serde_json::from_str(text)?;
        Ok(Self {
            signing: jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())?,
            email: key.client_email,
            token_uri: key.token_uri,
            cached: tokio::sync::Mutex::new(None),
        })
    }

    /// Already holding `token` (valid for an hour); for tests, which have no real key.
    #[cfg(any(test, feature = "test-util"))]
    pub fn with_token(token: &str) -> Self {
        Self {
            email: "tests@example.invalid".into(),
            signing: jsonwebtoken::EncodingKey::from_secret(b"unused"),
            token_uri: "http://127.0.0.1:9/token".into(),
            cached: tokio::sync::Mutex::new(Some((
                token.to_owned(),
                Instant::now() + Duration::from_hours(1),
            ))),
        }
    }

    /// A valid access token, fetched if the cached one is (nearly) expired.
    pub async fn token(&self, client: &reqwest::Client) -> anyhow::Result<String> {
        let mut cached = self.cached.lock().await;
        if let Some((token, until)) = cached.as_ref()
            && Instant::now() < *until
        {
            return Ok(token.clone());
        }
        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            iss: &self.email,
            scope: SCOPE,
            aud: &self.token_uri,
            iat: now,
            exp: now + 3600,
        };
        let assertion = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &claims,
            &self.signing,
        )?;
        // The assertion is base64url and dots: nothing to escape.
        let body = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
        );
        let answer: TokenAnswer = client
            .post(&self.token_uri)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let lifetime = Duration::from_secs(answer.expires_in).saturating_sub(REFRESH_EARLY);
        *cached = Some((answer.access_token.clone(), Instant::now() + lifetime));
        Ok(answer.access_token)
    }
}
