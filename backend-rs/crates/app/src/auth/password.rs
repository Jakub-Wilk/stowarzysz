//! Password hashing: argon2id for new hashes; Django's `pbkdf2_sha256` is still verified so
//! migrated accounts can sign in, and is replaced by argon2id at that first sign-in.
//!
//! Hashing is CPU-heavy, so it runs on the blocking pool behind a semaphore: a burst of logins
//! cannot starve the async workers or the CPU.

use std::sync::{Arc, LazyLock};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use subtle::ConstantTimeEq as _;
use tokio::sync::Semaphore;

use crate::AppResult;

const DJANGO_PREFIX: &str = "pbkdf2_sha256$";

/// A hash nobody's password matches, verified when the account does not exist so that a failed
/// login takes as long either way (no username probing through timing).
static DUMMY: LazyLock<String> =
    LazyLock::new(|| hash_now("not the password of anyone").unwrap_or_default());

#[derive(Debug, Clone)]
pub struct Passwords {
    permits: Arc<Semaphore>,
}

/// The result of checking a password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    No,
    Yes,
    /// Correct, and the stored hash should be replaced by this one (legacy or outdated).
    YesRehash(String),
}

impl Passwords {
    pub fn new(concurrency: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(concurrency.max(1))),
        }
    }

    pub async fn hash(&self, password: &str) -> AppResult<String> {
        let password = password.to_owned();
        self.blocking(move || hash_now(&password)).await
    }

    /// Check `password` against `stored` (`None`: no account, or not activated).
    pub async fn verify(&self, password: &str, stored: Option<&str>) -> AppResult<Verified> {
        let password = password.to_owned();
        let stored = stored.map(str::to_owned);
        self.blocking(move || {
            if let Some(stored) = stored {
                verify_now(&password, &stored)
            } else {
                let _ = verify_now(&password, &DUMMY);
                Ok(Verified::No)
            }
        })
        .await
    }

    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    ) -> AppResult<T> {
        let _permit = self.permits.acquire().await.map_err(anyhow::Error::from)?;
        Ok(tokio::task::spawn_blocking(work)
            .await
            .map_err(anyhow::Error::from)??)
    }
}

impl Default for Passwords {
    fn default() -> Self {
        Self::new(std::thread::available_parallelism().map_or(2, |n| n.get().min(4)))
    }
}

/// argon2id with the OWASP-recommended baseline (19 MiB, 2 passes, 1 lane).
fn argon2() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

fn hash_now(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(argon2()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!("argon2: {error}"))?
        .to_string())
}

fn verify_now(password: &str, stored: &str) -> anyhow::Result<Verified> {
    if stored.starts_with(DJANGO_PREFIX) {
        return Ok(if verify_django(password, stored) {
            Verified::YesRehash(hash_now(password)?)
        } else {
            Verified::No
        });
    }
    let Ok(parsed) = PasswordHash::new(stored) else {
        tracing::error!("a stored password hash is malformed");
        return Ok(Verified::No);
    };
    if argon2()
        .verify_password(password.as_bytes(), &parsed)
        .is_err()
    {
        return Ok(Verified::No);
    }
    // Hashed with other parameters (an older default): upgrade while we have the password.
    let current = Params::default();
    let outdated = Params::try_from(&parsed).map_or(true, |params| {
        (params.m_cost(), params.t_cost(), params.p_cost())
            != (current.m_cost(), current.t_cost(), current.p_cost())
    });
    Ok(if outdated {
        Verified::YesRehash(hash_now(password)?)
    } else {
        Verified::Yes
    })
}

/// Django's `pbkdf2_sha256$<iterations>$<salt>$<base64 hash>`.
fn verify_django(password: &str, stored: &str) -> bool {
    let mut parts = stored.splitn(4, '$');
    let (Some(_), Some(iterations), Some(salt), Some(expected)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let (Ok(iterations), Ok(expected)) = (iterations.parse::<u32>(), STANDARD.decode(expected))
    else {
        return false;
    };
    if iterations == 0 || expected.len() != 32 {
        return false;
    }
    let mut derived = [0_u8; 32];
    if pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
        password.as_bytes(),
        salt.as_bytes(),
        iterations,
        &mut derived,
    )
    .is_err()
    {
        return false;
    }
    derived.ct_eq(expected.as_slice()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made by Django 6: `make_password("s3cret-pass-123", salt="fixedsaltfixedsalt12")`.
    const DJANGO: &str =
        "pbkdf2_sha256$1500000$fixedsaltfixedsalt12$Ko7BwSPmIe4CpYnNmje8vTQd0RyY7kihGbAZ0kUpvM8=";

    #[test]
    fn verifies_django_hashes_and_asks_for_a_rehash() {
        match verify_now("s3cret-pass-123", DJANGO).unwrap() {
            Verified::YesRehash(new) => {
                assert!(new.starts_with("$argon2id$"), "{new}");
                assert_eq!(verify_now("s3cret-pass-123", &new).unwrap(), Verified::Yes);
            }
            other => panic!("expected a rehash, got {other:?}"),
        }
        assert_eq!(verify_now("wrong", DJANGO).unwrap(), Verified::No);
    }

    #[test]
    fn rejects_malformed_django_hashes() {
        for bad in [
            "pbkdf2_sha256$",
            "pbkdf2_sha256$x$s$h",
            "pbkdf2_sha256$0$s$AAAA",
            "pbkdf2_sha256$1$s$AAAA",
        ] {
            assert_eq!(verify_now("pw", bad).unwrap(), Verified::No, "{bad}");
        }
    }

    #[test]
    fn argon2_round_trip() {
        let hash = hash_now("Zażółć gęślą 9").unwrap();
        assert_eq!(verify_now("Zażółć gęślą 9", &hash).unwrap(), Verified::Yes);
        assert_eq!(verify_now("zażółć gęślą 9", &hash).unwrap(), Verified::No);
    }

    #[test]
    fn outdated_argon2_parameters_are_upgraded() {
        let weak = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(8192, 1, 1, None).unwrap(),
        )
        .hash_password(b"pw", &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string();
        assert!(matches!(
            verify_now("pw", &weak).unwrap(),
            Verified::YesRehash(_)
        ));
    }

    #[tokio::test]
    async fn missing_accounts_never_verify() {
        assert_eq!(
            Passwords::new(1).verify("anything", None).await.unwrap(),
            Verified::No
        );
    }
}
