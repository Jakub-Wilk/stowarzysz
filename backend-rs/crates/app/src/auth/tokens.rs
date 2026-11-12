//! Access tokens (short-lived JWTs), opaque refresh tokens, and the legacy SimpleJWT refresh
//! tokens that may be exchanged once after the migration.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, TimeDelta, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::config::Secret;

pub const ACCESS_TTL: TimeDelta = TimeDelta::minutes(15);
/// Small clock differences between devices and servers are tolerated.
const LEEWAY: TimeDelta = TimeDelta::seconds(10);
const REFRESH_PREFIX: &str = "s1_";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccessClaims {
    /// The user id.
    sub: String,
    /// The session id.
    sid: Uuid,
    iat: i64,
    exp: i64,
}

/// Who an access token says the caller is; still to be checked against the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessToken {
    pub user_id: i64,
    pub session_id: Uuid,
}

/// Signs and checks access tokens. Expiry is judged by the application clock.
#[derive(Clone)]
pub struct Signer {
    encoding: EncodingKey,
    decoding: DecodingKey,
    legacy: Option<DecodingKey>,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signer")
            .field("legacy", &self.legacy.is_some())
            .finish_non_exhaustive()
    }
}

impl Signer {
    pub fn new(secret: &Secret, legacy_secret: Option<&Secret>) -> Self {
        let key = secret.expose().as_bytes();
        Self {
            encoding: EncodingKey::from_secret(key),
            decoding: DecodingKey::from_secret(key),
            legacy: legacy_secret.map(|s| DecodingKey::from_secret(s.expose().as_bytes())),
        }
    }

    pub fn issue(
        &self,
        user_id: i64,
        session_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<String> {
        let claims = AccessClaims {
            sub: user_id.to_string(),
            sid: session_id,
            iat: now.timestamp(),
            exp: (now + ACCESS_TTL).timestamp(),
        };
        Ok(jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &self.encoding,
        )?)
    }

    /// `None` for anything that is not a valid, unexpired token signed by us.
    pub fn verify(&self, token: &str, now: DateTime<Utc>) -> Option<AccessToken> {
        let claims = decode::<AccessClaims>(token, &self.decoding)?;
        if DateTime::from_timestamp(claims.exp, 0)? + LEEWAY < now {
            return None;
        }
        Some(AccessToken {
            user_id: claims.sub.parse().ok()?,
            session_id: claims.sid,
        })
    }

    /// A SimpleJWT refresh token from the Django backend: `(jti, user id)` if it is validly
    /// signed with the old `SECRET_KEY` and unexpired. Whether it may still be exchanged is
    /// decided by the database (`legacy_refresh_tokens`).
    pub fn verify_legacy_refresh(&self, token: &str, now: DateTime<Utc>) -> Option<(String, i64)> {
        #[derive(Deserialize)]
        struct Claims {
            token_type: String,
            jti: String,
            exp: i64,
            user_id: serde_json::Value, // a string in recent SimpleJWT, a number before
        }
        let claims = decode::<Claims>(token, self.legacy.as_ref()?)?;
        if claims.token_type != "refresh" || DateTime::from_timestamp(claims.exp, 0)? + LEEWAY < now
        {
            return None;
        }
        let user_id = match claims.user_id {
            serde_json::Value::Number(n) => n.as_i64()?,
            serde_json::Value::String(s) => s.parse().ok()?,
            _ => return None,
        };
        Some((claims.jti, user_id))
    }
}

fn decode<T: serde::de::DeserializeOwned>(token: &str, key: &DecodingKey) -> Option<T> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false; // checked against the application clock instead
    validation.required_spec_claims.clear();
    jsonwebtoken::decode::<T>(token, key, &validation)
        .ok()
        .map(|data| data.claims)
}

/// A new opaque refresh token and the hash stored in its place.
pub fn new_refresh_token() -> (String, Vec<u8>) {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = format!("{REFRESH_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
    let hash = hash_token(&token);
    (token, hash)
}

/// SHA-256 of a token: what is stored and looked up (tokens are random, so no salt is needed).
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Whether a refresh token looks like a legacy SimpleJWT one (a JWT has two dots).
pub fn is_legacy_refresh(token: &str) -> bool {
    !token.starts_with(REFRESH_PREFIX) && token.matches('.').count() == 2
}

/// A random URL-safe token for one-time links (activation).
pub fn new_link_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> Signer {
        Signer::new(
            &Secret::new("a-secret-that-is-at-least-32-bytes!!"),
            Some(&Secret::new("django-secret")),
        )
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    #[test]
    fn access_tokens_round_trip_until_they_expire() {
        let signer = signer();
        let session = Uuid::new_v4();
        let token = signer.issue(42, session, now()).unwrap();
        assert_eq!(
            signer.verify(&token, now()),
            Some(AccessToken {
                user_id: 42,
                session_id: session
            })
        );
        assert!(
            signer
                .verify(&token, now() + ACCESS_TTL + TimeDelta::seconds(5))
                .is_some(),
            "leeway"
        );
        assert_eq!(
            signer.verify(&token, now() + ACCESS_TTL + TimeDelta::seconds(11)),
            None
        );
    }

    #[test]
    fn tokens_signed_with_another_key_are_rejected() {
        let other = Signer::new(&Secret::new("another-secret-that-is-32-bytes-long"), None);
        let token = other.issue(1, Uuid::new_v4(), now()).unwrap();
        assert_eq!(signer().verify(&token, now()), None);
        assert_eq!(signer().verify("garbage", now()), None);
    }

    #[test]
    fn legacy_refresh_tokens_are_recognised() {
        #[derive(Serialize)]
        struct Legacy<'a> {
            token_type: &'a str,
            exp: i64,
            iat: i64,
            jti: &'a str,
            user_id: &'a str,
        }
        let sign = |token_type, exp: i64| {
            let claims = Legacy {
                token_type,
                exp,
                iat: 0,
                jti: "abc123",
                user_id: "7",
            };
            jsonwebtoken::encode(
                &Header::new(Algorithm::HS256),
                &claims,
                &EncodingKey::from_secret(b"django-secret"),
            )
            .unwrap()
        };
        let valid = sign("refresh", now().timestamp() + 60);
        assert!(is_legacy_refresh(&valid));
        assert_eq!(
            signer().verify_legacy_refresh(&valid, now()),
            Some(("abc123".into(), 7))
        );
        assert_eq!(
            signer().verify_legacy_refresh(&sign("access", now().timestamp() + 60), now()),
            None
        );
        assert_eq!(
            signer().verify_legacy_refresh(&sign("refresh", now().timestamp() - 60), now()),
            None
        );
        // Without the old key configured, nothing is accepted.
        let without = Signer::new(&Secret::new("a-secret-that-is-at-least-32-bytes!!"), None);
        assert_eq!(without.verify_legacy_refresh(&valid, now()), None);
    }

    #[test]
    fn refresh_tokens_are_random_and_hashed() {
        let (a, hash_a) = new_refresh_token();
        let (b, _) = new_refresh_token();
        assert_ne!(a, b);
        assert!(a.starts_with("s1_") && !is_legacy_refresh(&a));
        assert_eq!(hash_token(&a), hash_a);
        assert_eq!(hash_a.len(), 32);
    }
}
