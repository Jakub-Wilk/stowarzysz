//! Who is calling: bearer authentication, administrators, the client's address (for rate
//! limits) and its user agent.

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;

use app::AppError;
use app::auth::{self, Actor};
use app::config::{Config, Rate};
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header;
use axum::http::request::Parts;
use governor::clock::{Clock as _, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};

use super::problem::ApiError;
use crate::AppState;

/// The signed-in caller.
#[derive(Debug, Clone)]
pub struct AuthUser(pub Actor);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let header = parts
            .headers
            .get(header::AUTHORIZATION)
            .ok_or_else(auth::not_authenticated)?;
        let token = header
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(auth::invalid_token)?;
        Ok(Self(auth::authenticate(&state.ctx, token).await?))
    }
}

/// A signed-in administrator.
#[derive(Debug, Clone)]
pub struct AdminUser(pub Actor);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let AuthUser(actor) = AuthUser::from_request_parts(parts, state).await?;
        if !actor.is_superuser {
            return Err(
                AppError::forbidden("admin_only", "Tylko administrator może to zrobić.").into(),
            );
        }
        Ok(Self(actor))
    }
}

/// The client's address: the `TRUSTED_PROXY_HOPS`-th entry from the right of
/// `X-Forwarded-For` (what our own proxy appended; entries further left are client-supplied),
/// else the TCP peer.
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self::from_parts(parts, state)))
    }
}

impl ClientIp {
    fn from_parts(parts: &Parts, state: &AppState) -> Self {
        let hops = usize::from(state.ctx.config.trusted_proxy_hops);
        let forwarded = (hops > 0)
            .then(|| parts.headers.get_all("x-forwarded-for"))
            .and_then(|values| {
                let all: Vec<&str> = values
                    .iter()
                    .filter_map(|v| v.to_str().ok())
                    .flat_map(|v| v.split(','))
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .collect();
                all.len()
                    .checked_sub(hops)
                    .map(|index| all[index].to_owned())
            })
            .filter(|ip| ip.parse::<IpAddr>().is_ok());
        let peer = || {
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map_or_else(
                    || "unknown".to_owned(),
                    |ConnectInfo(addr)| addr.ip().to_string(),
                )
        };
        Self(forwarded.unwrap_or_else(peer))
    }
}

/// The `User-Agent` header (empty if missing).
#[derive(Debug, Clone)]
pub struct UserAgent(pub String);

impl<S: Send + Sync> FromRequestParts<S> for UserAgent {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let agent = parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        std::future::ready(Ok(Self(agent.to_owned())))
    }
}

/// In-memory rate limits (GCRA), per process.
#[derive(Debug)]
pub struct Limiters {
    pub login_ip: DefaultKeyedRateLimiter<String>,
    pub login_username: DefaultKeyedRateLimiter<String>,
    pub login_users: DefaultKeyedRateLimiter<String>,
    pub activation: DefaultKeyedRateLimiter<String>,
    pub uploads: DefaultKeyedRateLimiter<String>,
}

impl Limiters {
    pub fn new(config: &Config) -> Self {
        let limits = &config.limits;
        Self {
            login_ip: limiter(limits.login),
            login_username: limiter(limits.login),
            login_users: limiter(limits.login_users),
            activation: limiter(limits.activation),
            uploads: limiter(limits.uploads),
        }
    }
}

fn limiter(rate: Rate) -> DefaultKeyedRateLimiter<String> {
    let count = NonZeroU32::new(rate.count).unwrap_or(NonZeroU32::MIN);
    let quota = Quota::with_period(rate.per / count.get())
        .unwrap_or_else(|| Quota::per_second(count))
        .allow_burst(count);
    RateLimiter::keyed(quota)
}

/// Count one request against `limiter` for `key`.
pub fn throttle(limiter: &DefaultKeyedRateLimiter<String>, key: &str) -> Result<(), ApiError> {
    limiter.check_key(&key.to_owned()).map_err(|not_until| {
        ApiError(AppError::RateLimited {
            retry_after: Some(not_until.wait_time_from(DefaultClock::default().now())),
        })
    })?;
    if limiter.len() > 10_000 {
        limiter.retain_recent(); // forget clients that have not been seen for a while
    }
    Ok(())
}
