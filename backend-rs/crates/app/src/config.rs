//! Typed configuration from the environment (plus `.env` in development).
//!
//! Everything is parsed and validated once at startup; [`Config::from_lookup`] collects *every*
//! problem before failing, so a misconfigured deployment reports all of them at once. Secrets
//! have development defaults only when `APP_ENV=dev`; production refuses to start without them.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Config {
    pub env: Environment,
    pub bind_addr: SocketAddr,
    pub database_url: Secret,
    /// Public origin of the frontend, without a trailing slash (used in activation links).
    pub frontend_url: String,
    /// Only needed when the frontend is served from another origin (development without the
    /// Vite proxy); production is same-origin behind nginx.
    pub cors_allowed_origins: Vec<String>,
    /// How many reverse proxies in front of us append to `X-Forwarded-For` (nginx: 1).
    pub trusted_proxy_hops: u8,
    pub log_format: LogFormat,
    pub media_root: PathBuf,
    pub secrets: Secrets,
    /// Web Push; `None` disables notifications (the frontend hides the toggle).
    pub vapid: Option<Vapid>,
    /// ECB reference rates (Frankfurter API), without a trailing slash.
    pub rates_url: String,
    pub gemini: Gemini,
    /// Optional Vertex AI candidates for receipt OCR.
    pub vertex: Option<Vertex>,
    pub ocr: Ocr,
    pub images: Images,
    pub limits: Limits,
    /// Only read by `import-django`.
    pub legacy: Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Prod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone)]
pub struct Secrets {
    /// HMAC key for access tokens (≥ 32 bytes).
    pub jwt: Secret,
    /// HMAC key for signed media URLs (≥ 32 bytes).
    pub media_signing: Secret,
    /// XChaCha20-Poly1305 key sealing Secret Santa pairings (exactly 32 bytes, base64).
    pub santa_seal: SecretKey32,
}

#[derive(Debug, Clone)]
pub struct Vapid {
    pub public_key: String,
    pub private_key: Secret,
    pub subject: String,
}

#[derive(Debug, Clone)]
pub struct Gemini {
    /// Google AI Studio key; `None` disables the AI Studio candidates.
    pub api_key: Option<Secret>,
    /// The AI Studio API origin (overridable for tests).
    pub base_url: String,
    pub model: String,
    /// Tried after `model` on every enabled endpoint (each model has its own capacity pool).
    pub fallback_models: Vec<String>,
    /// Try the `priority` service tier first (needs a paid AI Studio project).
    pub priority: bool,
    /// Largest raw image we send inline; base64 adds a third on top.
    pub inline_budget_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Vertex {
    pub project: String,
    /// Path to a service-account JSON file.
    pub credentials: PathBuf,
    /// Preference order, e.g. `global`, `europe-central2`, `europe-west4`.
    pub locations: Vec<String>,
    /// Replaces `https://{location}-aiplatform.googleapis.com` for every location (tests).
    pub base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Ocr {
    /// Longest side of the image sent to the model.
    pub max_side: u32,
    /// Overall deadline for one OCR request, across all candidates and retries.
    pub deadline: Duration,
    /// Start the next candidate in parallel if the current one is slower than this.
    pub hedge_after: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct Images {
    /// libwebp lossless preset, 0 (fastest) to 9 (smallest). 2: measured on a 12 MP photo as fast
    /// as possible without growing the files (4 was as small and 1.7x slower).
    pub lossless_effort: u8,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub json_body_bytes: usize,
    pub upload_bytes: usize,
    pub activation_token_ttl: Duration,
    pub login: Rate,
    pub login_users: Rate,
    pub activation: Rate,
    pub uploads: Rate,
    pub ocr: Rate,
}

#[derive(Debug, Clone, Default)]
pub struct Legacy {
    pub database_url: Option<Secret>,
    /// Django `SECRET_KEY`: verifies `SimpleJWT` refresh tokens during the session exchange.
    pub secret_key: Option<Secret>,
    /// Django `SECRET_SANTA_KEY` (Fernet): opens active pairings so they can be re-sealed.
    pub santa_key: Option<Secret>,
}

/// `count` requests per `per`, e.g. `10/min`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    pub count: u32,
    pub per: Duration,
}

impl FromStr for Rate {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (count, unit) = value
            .split_once('/')
            .ok_or("expected N/unit, e.g. 10/min")?;
        let count: u32 = count
            .trim()
            .parse()
            .map_err(|_| "the count is not a number")?;
        if count == 0 {
            return Err("the count must be positive".into());
        }
        let per = match unit.trim() {
            "s" | "sec" | "second" => Duration::from_secs(1),
            "m" | "min" | "minute" => Duration::from_mins(1),
            "h" | "hour" => Duration::from_hours(1),
            "d" | "day" => Duration::from_hours(24),
            other => {
                return Err(format!(
                    "unknown unit {other:?} (use sec, min, hour or day)"
                ));
            }
        };
        Ok(Self { count, per })
    }
}

/// A secret string; `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// A 32-byte key; `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey32([u8; 32]);

impl SecretKey32 {
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKey32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey32(***)")
    }
}

/// Every problem found while reading the configuration.
#[derive(Debug, thiserror::Error)]
#[error("invalid configuration:\n  - {}", .0.join("\n  - "))]
pub struct ConfigError(pub Vec<String>);

// Development-only defaults. They are public in the repository, so production rejects them.
const DEV_JWT_SECRET: &str = "dev-insecure-jwt-secret-0123456789abcdef";
const DEV_MEDIA_SIGNING_KEY: &str = "dev-insecure-media-signing-key-0123456789";
const DEV_SANTA_SEAL_KEY: &str = "ZGV2LWluc2VjdXJlLXNhbnRhLXNlYWwta2V5LTAxMjM="; // 32 bytes
const DEV_DATABASE_URL: &str = "postgres://postgres:postgres@localhost:5434/stowarzysz2";

impl Config {
    /// Read `.env` (if present; never overriding real environment variables), then the
    /// environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        // A missing .env is normal (production, CI); a malformed one is a real error.
        if let Err(error) = dotenvy::dotenv()
            && !error.not_found()
        {
            return Err(ConfigError(vec![format!(".env: {error}")]));
        }
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from any key lookup (the environment, or a map in tests).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let mut r = Reader {
            lookup: &lookup,
            errors: Vec::new(),
        };

        let env = match r.text("APP_ENV").as_deref() {
            None | Some("prod" | "production") => Environment::Prod,
            Some("dev" | "development") => Environment::Dev,
            Some(other) => {
                r.error(
                    "APP_ENV",
                    format!("unknown value {other:?} (use dev or prod)"),
                );
                Environment::Prod
            }
        };
        let dev = env == Environment::Dev;

        let bind_addr = r
            .parsed("BIND_ADDR")
            .unwrap_or_else(|| ([0, 0, 0, 0], 8000).into());
        let database_url = r.secret_or_dev("DATABASE_URL", dev, DEV_DATABASE_URL);
        let frontend_url = r
            .text("FRONTEND_URL")
            .unwrap_or_else(|| "http://localhost:5173".into())
            .trim_end_matches('/')
            .to_owned();
        let cors_allowed_origins = r.list("CORS_ALLOWED_ORIGINS");
        let trusted_proxy_hops = r.parsed("TRUSTED_PROXY_HOPS").unwrap_or(1);
        let log_format = match r.text("LOG_FORMAT").as_deref() {
            None => {
                if dev {
                    LogFormat::Pretty
                } else {
                    LogFormat::Json
                }
            }
            Some("pretty") => LogFormat::Pretty,
            Some("json") => LogFormat::Json,
            Some(other) => {
                r.error(
                    "LOG_FORMAT",
                    format!("unknown value {other:?} (use pretty or json)"),
                );
                LogFormat::Json
            }
        };
        let media_root = r
            .text("MEDIA_ROOT")
            .map_or_else(|| PathBuf::from("media"), PathBuf::from);

        let jwt = r.secret_or_dev("JWT_SECRET", dev, DEV_JWT_SECRET);
        r.min_len("JWT_SECRET", jwt.as_ref(), 32);
        let media_signing = r.secret_or_dev("MEDIA_SIGNING_KEY", dev, DEV_MEDIA_SIGNING_KEY);
        r.min_len("MEDIA_SIGNING_KEY", media_signing.as_ref(), 32);
        let santa_seal = r
            .secret_or_dev("SANTA_SEAL_KEY", dev, DEV_SANTA_SEAL_KEY)
            .and_then(|raw| r.key32("SANTA_SEAL_KEY", raw.expose()));
        if !dev {
            for (key, value, default) in [
                ("JWT_SECRET", jwt.as_ref(), DEV_JWT_SECRET),
                (
                    "MEDIA_SIGNING_KEY",
                    media_signing.as_ref(),
                    DEV_MEDIA_SIGNING_KEY,
                ),
            ] {
                if value.is_some_and(|v| v.expose() == default) {
                    r.error(
                        key,
                        "the development default must not be used in production",
                    );
                }
            }
        }

        let vapid = match (r.secret("VAPID_PRIVATE_KEY"), r.text("VAPID_PUBLIC_KEY")) {
            (Some(private_key), Some(public_key)) => Some(Vapid {
                public_key,
                private_key,
                subject: r
                    .text("VAPID_SUBJECT")
                    .unwrap_or_else(|| "mailto:admin@localhost".into()),
            }),
            (None, None) => None,
            _ => {
                r.error(
                    "VAPID_*",
                    "set both VAPID_PUBLIC_KEY and VAPID_PRIVATE_KEY, or neither",
                );
                None
            }
        };

        let rates_url = r
            .text("FRANKFURTER_URL")
            .unwrap_or_else(|| "https://api.frankfurter.dev/v1".into())
            .trim_end_matches('/')
            .to_owned();
        let gemini = Gemini {
            api_key: r.secret("GEMINI_API_KEY"),
            base_url: r
                .text("GEMINI_BASE_URL")
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".into())
                .trim_end_matches('/')
                .to_owned(),
            model: r
                .text("GEMINI_MODEL")
                .unwrap_or_else(|| "gemini-3.6-flash".into()),
            fallback_models: r.list("GEMINI_FALLBACK_MODELS"),
            priority: r.flag("GEMINI_PRIORITY").unwrap_or(false),
            inline_budget_bytes: r.parsed("GEMINI_INLINE_BUDGET_BYTES").unwrap_or(14 * MIB),
        };

        let vertex = match (r.text("VERTEX_PROJECT"), r.text("VERTEX_CREDENTIALS")) {
            (Some(project), Some(credentials)) => {
                let mut locations = r.list("VERTEX_LOCATIONS");
                if locations.is_empty() {
                    locations.push("global".into());
                }
                Some(Vertex {
                    project,
                    credentials: PathBuf::from(credentials),
                    locations,
                    base_url: r
                        .text("VERTEX_BASE_URL")
                        .map(|url| url.trim_end_matches('/').to_owned()),
                })
            }
            (None, None) => None,
            _ => {
                r.error(
                    "VERTEX_*",
                    "set both VERTEX_PROJECT and VERTEX_CREDENTIALS, or neither",
                );
                None
            }
        };

        let ocr = Ocr {
            max_side: r.parsed("OCR_MAX_SIDE").unwrap_or(3072),
            deadline: r
                .duration("OCR_DEADLINE")
                .unwrap_or(Duration::from_secs(90)),
            hedge_after: r.duration("OCR_HEDGE_AFTER"),
        };
        if ocr.max_side < 1536 {
            r.error(
                "OCR_MAX_SIDE",
                "must be at least 1536 (the compressor's floor)",
            );
        }

        let lossless_effort = r.parsed("IMAGE_LOSSLESS_EFFORT").unwrap_or(2);
        if lossless_effort > 9 {
            r.error("IMAGE_LOSSLESS_EFFORT", "must be between 0 and 9");
        }

        let limits = Limits {
            json_body_bytes: r.parsed("JSON_BODY_LIMIT_BYTES").unwrap_or(1024 * 1024),
            upload_bytes: r.parsed("UPLOAD_LIMIT_BYTES").unwrap_or(50 * 1024 * 1024),
            activation_token_ttl: r
                .duration("ACTIVATION_TOKEN_TTL")
                .unwrap_or(Duration::from_hours(72)),
            login: r.rate("LOGIN_RATE", "10/min"),
            login_users: r.rate("LOGIN_USERS_RATE", "60/min"),
            activation: r.rate("ACTIVATION_RATE", "10/min"),
            uploads: r.rate("UPLOAD_RATE", "30/min"),
            ocr: r.rate("OCR_RATE", "10/min"),
        };

        let legacy = Legacy {
            database_url: r.secret("LEGACY_DATABASE_URL"),
            secret_key: r.secret("LEGACY_SECRET_KEY"),
            santa_key: r.secret("LEGACY_SANTA_KEY"),
        };

        let errors = r.errors;
        if !errors.is_empty() {
            return Err(ConfigError(errors));
        }
        // Every `None` below has a matching entry in `errors`, so these cannot fail here.
        let (Some(database_url), Some(jwt), Some(media_signing), Some(santa_seal)) =
            (database_url, jwt, media_signing, santa_seal)
        else {
            return Err(ConfigError(vec!["internal: missing required value".into()]));
        };

        Ok(Self {
            env,
            bind_addr,
            database_url,
            frontend_url,
            cors_allowed_origins,
            trusted_proxy_hops,
            log_format,
            media_root,
            secrets: Secrets {
                jwt,
                media_signing,
                santa_seal,
            },
            vapid,
            rates_url,
            gemini,
            vertex,
            ocr,
            images: Images { lossless_effort },
            limits,
            legacy,
        })
    }

    /// A development configuration with every default; for tests.
    pub fn for_tests() -> Self {
        Self::from_lookup(|key| (key == "APP_ENV").then(|| "dev".to_owned()))
            .expect("the development defaults are valid")
    }

    pub fn is_dev(&self) -> bool {
        self.env == Environment::Dev
    }
}

/// Reads keys and records every problem instead of stopping at the first.
struct Reader<'a> {
    lookup: &'a dyn Fn(&str) -> Option<String>,
    errors: Vec<String>,
}

impl Reader<'_> {
    fn error(&mut self, key: &str, message: impl fmt::Display) {
        self.errors.push(format!("{key}: {message}"));
    }

    /// The trimmed value; empty counts as unset.
    fn text(&self, key: &str) -> Option<String> {
        (self.lookup)(key)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    }

    fn secret(&self, key: &str) -> Option<Secret> {
        self.text(key).map(Secret)
    }

    fn secret_or_dev(&mut self, key: &str, dev: bool, default: &str) -> Option<Secret> {
        match self.secret(key) {
            Some(secret) => Some(secret),
            None if dev => Some(Secret(default.to_owned())),
            None => {
                self.error(
                    key,
                    "is required (set APP_ENV=dev for development defaults)",
                );
                None
            }
        }
    }

    fn min_len(&mut self, key: &str, value: Option<&Secret>, min: usize) {
        if value.is_some_and(|v| v.expose().len() < min) {
            self.error(key, format!("must be at least {min} bytes long"));
        }
    }

    fn key32(&mut self, key: &str, raw: &str) -> Option<SecretKey32> {
        let decoded = [STANDARD, URL_SAFE, URL_SAFE_NO_PAD]
            .iter()
            .find_map(|e| e.decode(raw).ok());
        match decoded.map(<[u8; 32]>::try_from) {
            Some(Ok(bytes)) => Some(SecretKey32(bytes)),
            Some(Err(bytes)) => {
                self.error(key, format!("must decode to 32 bytes, got {}", bytes.len()));
                None
            }
            None => {
                self.error(key, "is not valid base64");
                None
            }
        }
    }

    fn parsed<T: FromStr>(&mut self, key: &str) -> Option<T>
    where
        T::Err: fmt::Display,
    {
        let raw = self.text(key)?;
        match raw.parse() {
            Ok(value) => Some(value),
            Err(error) => {
                self.error(key, format!("{raw:?}: {error}"));
                None
            }
        }
    }

    fn duration(&mut self, key: &str) -> Option<Duration> {
        let raw = self.text(key)?;
        match humantime::parse_duration(&raw) {
            Ok(value) => Some(value),
            Err(error) => {
                self.error(key, format!("{raw:?}: {error} (e.g. 90s, 15min, 72h)"));
                None
            }
        }
    }

    fn flag(&mut self, key: &str) -> Option<bool> {
        let raw = self.text(key)?;
        match raw.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => {
                self.error(key, format!("{raw:?} is not a boolean (true/false)"));
                None
            }
        }
    }

    fn rate(&mut self, key: &str, default: &str) -> Rate {
        let fallback = || default.parse().expect("built-in rates are valid");
        self.parsed(key).unwrap_or_else(fallback)
    }

    /// Comma-separated values; blanks are dropped.
    fn list(&self, key: &str) -> Vec<String> {
        self.text(key)
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|key| map.get(key).cloned())
    }

    fn prod_secrets() -> Vec<(&'static str, &'static str)> {
        vec![
            ("DATABASE_URL", "postgres://u:p@db/stowarzysz2"),
            ("JWT_SECRET", "a-production-secret-that-is-long-enough"),
            (
                "MEDIA_SIGNING_KEY",
                "another-production-secret-long-enough!",
            ),
            (
                "SANTA_SEAL_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]
    }

    #[test]
    fn dev_defaults_are_valid() {
        let config = Config::for_tests();
        assert!(config.is_dev());
        assert_eq!(config.limits.upload_bytes, 50 * 1024 * 1024);
        assert_eq!(
            config.limits.login,
            Rate {
                count: 10,
                per: Duration::from_secs(60)
            }
        );
        assert!(config.vapid.is_none() && config.vertex.is_none());
        assert_eq!(config.log_format, LogFormat::Pretty);
    }

    #[test]
    fn production_reports_every_missing_secret_at_once() {
        let error = config(&[]).unwrap_err();
        let text = error.to_string();
        for key in [
            "DATABASE_URL",
            "JWT_SECRET",
            "MEDIA_SIGNING_KEY",
            "SANTA_SEAL_KEY",
        ] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
    }

    #[test]
    fn production_accepts_real_secrets() {
        let config = config(&prod_secrets()).unwrap();
        assert_eq!(config.env, Environment::Prod);
        assert_eq!(config.log_format, LogFormat::Json);
        assert_eq!(
            config.secrets.santa_seal.expose(),
            b"0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn production_rejects_development_defaults_and_short_keys() {
        let mut pairs = prod_secrets();
        pairs[1] = ("JWT_SECRET", DEV_JWT_SECRET);
        pairs[2] = ("MEDIA_SIGNING_KEY", "short");
        pairs[3] = ("SANTA_SEAL_KEY", "c2hvcnQ=");
        let text = config(&pairs).unwrap_err().to_string();
        assert!(
            text.contains("JWT_SECRET: the development default"),
            "{text}"
        );
        assert!(
            text.contains("MEDIA_SIGNING_KEY: must be at least 32"),
            "{text}"
        );
        assert!(
            text.contains("SANTA_SEAL_KEY: must decode to 32 bytes, got 5"),
            "{text}"
        );
    }

    #[test]
    fn optional_groups_must_be_complete() {
        let text = config(&[
            ("APP_ENV", "dev"),
            ("VAPID_PUBLIC_KEY", "pub"),
            ("VERTEX_PROJECT", "proj"),
        ])
        .unwrap_err()
        .to_string();
        assert!(
            text.contains("VAPID_*") && text.contains("VERTEX_*"),
            "{text}"
        );
    }

    #[test]
    fn parses_values_and_lists() {
        let config = config(&[
            ("APP_ENV", "dev"),
            ("FRONTEND_URL", "https://example.org/"),
            ("GEMINI_FALLBACK_MODELS", "a, ,b"),
            ("GEMINI_PRIORITY", "yes"),
            ("VERTEX_PROJECT", "proj"),
            ("VERTEX_CREDENTIALS", "/run/secrets/vertex.json"),
            ("OCR_DEADLINE", "2min"),
            ("OCR_HEDGE_AFTER", "20s"),
            ("LOGIN_RATE", "5/hour"),
        ])
        .unwrap();
        assert_eq!(config.frontend_url, "https://example.org");
        assert_eq!(config.gemini.fallback_models, ["a", "b"]);
        assert!(config.gemini.priority);
        assert_eq!(config.vertex.unwrap().locations, ["global"]);
        assert_eq!(config.ocr.deadline, Duration::from_secs(120));
        assert_eq!(config.ocr.hedge_after, Some(Duration::from_secs(20)));
        assert_eq!(
            config.limits.login,
            Rate {
                count: 5,
                per: Duration::from_secs(3600)
            }
        );
    }

    #[test]
    fn reports_malformed_values() {
        let text = config(&[
            ("APP_ENV", "dev"),
            ("TRUSTED_PROXY_HOPS", "many"),
            ("OCR_DEADLINE", "soon"),
            ("LOGIN_RATE", "10/fortnight"),
            ("IMAGE_LOSSLESS_EFFORT", "12"),
        ])
        .unwrap_err()
        .to_string();
        for key in [
            "TRUSTED_PROXY_HOPS",
            "OCR_DEADLINE",
            "LOGIN_RATE",
            "IMAGE_LOSSLESS_EFFORT",
        ] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let config = Config::for_tests();
        let debug = format!("{config:?}");
        assert!(!debug.contains(DEV_JWT_SECRET) && debug.contains("Secret(***)"));
    }
}
