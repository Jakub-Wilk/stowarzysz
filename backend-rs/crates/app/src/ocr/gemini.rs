//! Receipt OCR through Gemini, spread over every configured way of reaching it so a busy
//! endpoint costs a failover, not a failed scan.
//!
//! A *candidate* is (endpoint, model, service tier), in preference order:
//! 1. AI Studio, `GEMINI_MODEL`, priority tier (`GEMINI_PRIORITY`; Google downgrades it to
//!    standard under load instead of refusing),
//! 2. AI Studio, standard tier,
//! 3. Vertex AI at each of `VERTEX_LOCATIONS` (`global` routes dynamically),
//! 4. the same for each of `GEMINI_FALLBACK_MODELS` (every model has its own capacity).
//!
//! A call walks the list, skipping candidates whose breaker is open ([`super::health`]). A
//! retryable answer (408, 429, 5xx, timeout, connection error) moves straight to the next
//! candidate; only when a whole round failed does it back off (1 s doubling, with jitter,
//! honouring `Retry-After`) and go round again, until `OCR_DEADLINE`. A 400 is never retried;
//! 401, 403 and 404 mean a candidate is misconfigured and rest it the longest. With
//! `OCR_HEDGE_AFTER`, a slow request gets company: the next candidate starts too and the first
//! answer wins.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;

use super::health::Health;
use super::receipt::{PROMPT, Receipt, response_schema};
use super::vertex::VertexAuth;
use crate::config::Config;

/// Content failures (no usable JSON) after which the image is deemed unreadable.
const MAX_CONTENT_FAILURES: u32 = 2;
const MAX_BACKOFF: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
enum Endpoint {
    Studio {
        base_url: String,
        key: String,
    },
    /// Up to `.../publishers/google/models`.
    Vertex {
        models_url: String,
    },
}

#[derive(Debug, Clone)]
pub struct Candidate {
    /// For logs and progress events, e.g. `studio/gemini-3.6-flash/priority`.
    pub id: String,
    endpoint: Endpoint,
    model: String,
    priority: bool,
}

/// Sent before each request goes out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub number: u32,
    pub candidate: String,
}

/// A receipt read by a candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Served {
    pub receipt: Receipt,
    pub candidate: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("no OCR candidates are configured")]
    NotConfigured,
    #[error("no candidate answered in time")]
    Unavailable,
    /// The request itself was refused (400) or the models returned nothing usable.
    #[error("the image could not be read")]
    Unreadable,
}

/// How one request went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// Busy, down or slow: try another candidate.
    Retryable { retry_after: Option<Duration> },
    /// Bad key, no access, unknown model.
    Misconfigured,
    /// The answer had no usable receipt (not the candidate's availability).
    Content,
    /// The request was refused as invalid (400): no candidate will do better.
    Refused,
}

type Flight<'a> =
    Pin<Box<dyn Future<Output = (usize, Result<Receipt, Failure>, Duration)> + Send + 'a>>;

/// The candidates and their health, shared by every request of this process.
pub struct Router {
    candidates: Vec<Candidate>,
    health: Vec<Mutex<Health>>,
    client: reqwest::Client,
    vertex: Option<Arc<VertexAuth>>,
    deadline: Duration,
    hedge_after: Option<Duration>,
    backoff: Duration,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<&str> = self.candidates.iter().map(|c| c.id.as_str()).collect();
        f.debug_struct("Router")
            .field("candidates", &ids)
            .finish_non_exhaustive()
    }
}

impl Router {
    /// The candidates `config` enables; Vertex ones only with `vertex` credentials.
    pub fn new(config: &Config, vertex: Option<VertexAuth>) -> Self {
        let gemini = &config.gemini;
        let mut candidates = Vec::new();
        let models = std::iter::once(&gemini.model).chain(&gemini.fallback_models);
        for (index, model) in models.enumerate() {
            if let Some(key) = &gemini.api_key {
                let studio = Endpoint::Studio {
                    base_url: gemini.base_url.clone(),
                    key: key.expose().to_owned(),
                };
                if index == 0 && gemini.priority {
                    candidates.push(Candidate {
                        id: format!("studio/{model}/priority"),
                        endpoint: studio.clone(),
                        model: model.clone(),
                        priority: true,
                    });
                }
                candidates.push(Candidate {
                    id: format!("studio/{model}"),
                    endpoint: studio,
                    model: model.clone(),
                    priority: false,
                });
            }
            if let (Some(settings), Some(_)) = (&config.vertex, &vertex) {
                for location in &settings.locations {
                    let origin = settings.base_url.clone().unwrap_or_else(|| {
                        if location == "global" {
                            "https://aiplatform.googleapis.com".to_owned()
                        } else {
                            format!("https://{location}-aiplatform.googleapis.com")
                        }
                    });
                    candidates.push(Candidate {
                        id: format!("vertex-{location}/{model}"),
                        endpoint: Endpoint::Vertex {
                            models_url: format!(
                                "{origin}/v1/projects/{}/locations/{location}/publishers/google/models",
                                settings.project
                            ),
                        },
                        model: model.clone(),
                        priority: false,
                    });
                }
            }
        }
        Self {
            health: candidates.iter().map(|_| Mutex::default()).collect(),
            candidates,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            vertex: vertex.map(Arc::new),
            deadline: config.ocr.deadline,
            hedge_after: config.ocr.hedge_after,
            backoff: Duration::from_secs(1),
        }
    }

    /// The first backoff between rounds (tests shorten it).
    #[must_use]
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    pub fn candidates(&self) -> impl Iterator<Item = &str> {
        self.candidates.iter().map(|c| c.id.as_str())
    }

    pub fn is_configured(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Whether candidate `id`'s breaker is open now (for tests and diagnostics).
    pub fn is_open(&self, id: &str) -> bool {
        let now = std::time::Instant::now();
        self.candidates
            .iter()
            .position(|c| c.id == id)
            .is_some_and(|index| self.health(index).is_open(now))
    }

    fn health(&self, index: usize) -> std::sync::MutexGuard<'_, Health> {
        self.health[index]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The next candidate from `cursor` on that may be tried now.
    fn next_admitted(&self, cursor: &mut usize) -> Option<usize> {
        let now = std::time::Instant::now();
        while *cursor < self.candidates.len() {
            let index = *cursor;
            *cursor += 1;
            if self.health(index).admit(now) {
                return Some(index);
            }
        }
        None
    }

    /// Read the receipt in `webp`. `progress` hears about each request before it goes out.
    pub async fn read(
        &self,
        webp: &[u8],
        progress: &UnboundedSender<Attempt>,
    ) -> Result<Served, ReadError> {
        if self.candidates.is_empty() {
            return Err(ReadError::NotConfigured);
        }
        let bodies = Bodies::new(webp);
        let deadline = Instant::now() + self.deadline;
        let mut attempts = 0_u32;
        let mut content_failures = 0_u32;
        for round in 0_u32.. {
            let mut cursor = 0;
            let mut in_flight: Vec<usize> = Vec::new();
            let mut flights: FuturesUnordered<Flight<'_>> = FuturesUnordered::new();
            let mut hedge_at: Option<Instant> = None;
            let mut retry_after: Option<Duration> = None;
            loop {
                if in_flight.is_empty() {
                    let Some(index) = self.next_admitted(&mut cursor) else {
                        break;
                    };
                    attempts += 1;
                    self.launch(index, attempts, &bodies, progress, &mut flights);
                    in_flight.push(index);
                    hedge_at = self.hedge_after.map(|after| Instant::now() + after);
                }
                let hedge = async {
                    match hedge_at {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    Some((index, result, latency)) = flights.next() => {
                        in_flight.retain(|i| *i != index);
                        match result {
                            Ok(receipt) => {
                                self.health(index).succeeded(latency);
                                for other in &in_flight {
                                    self.health(*other).abandoned();
                                }
                                let candidate = self.candidates[index].id.clone();
                                tracing::info!(%candidate, attempts, ?latency, "receipt read");
                                return Ok(Served { receipt, candidate, attempts });
                            }
                            Err(Failure::Refused) => {
                                self.health(index).abandoned();
                                for other in &in_flight {
                                    self.health(*other).abandoned();
                                }
                                return Err(ReadError::Unreadable);
                            }
                            Err(Failure::Content) => {
                                self.health(index).abandoned();
                                content_failures += 1;
                                if content_failures >= MAX_CONTENT_FAILURES {
                                    for other in &in_flight {
                                        self.health(*other).abandoned();
                                    }
                                    return Err(ReadError::Unreadable);
                                }
                            }
                            Err(Failure::Misconfigured) => {
                                self.health(index).misconfigured(std::time::Instant::now());
                            }
                            Err(Failure::Retryable { retry_after: wait }) => {
                                self.health(index).failed(std::time::Instant::now(), rand::random());
                                retry_after = retry_after.max(wait);
                            }
                        }
                        if in_flight.is_empty() {
                            hedge_at = None;
                        }
                    }
                    () = hedge => {
                        hedge_at = None;
                        if let Some(index) = self.next_admitted(&mut cursor) {
                            attempts += 1;
                            tracing::info!(candidate = %self.candidates[index].id, "hedging a slow OCR request");
                            self.launch(index, attempts, &bodies, progress, &mut flights);
                            in_flight.push(index);
                        }
                    }
                    () = tokio::time::sleep_until(deadline) => {
                        for index in &in_flight {
                            self.health(*index).abandoned();
                        }
                        tracing::warn!(attempts, "OCR deadline passed");
                        return Err(ReadError::Unavailable);
                    }
                }
            }
            // A whole round failed: back off, then go round again if there is time.
            let now = Instant::now();
            let factor = 1u32 << round.min(4);
            let jitter = 0.8 + f64::from(rand::random::<u32>() % 401) / 1000.0;
            let mut wait = (self.backoff * factor).mul_f64(jitter).min(MAX_BACKOFF);
            wait = wait.max(retry_after.unwrap_or_default());
            // Everything resting: wait for the first breaker to let a probe through.
            let std_now = std::time::Instant::now();
            let soonest = (0..self.candidates.len())
                .map(|i| self.health(i).available_at(std_now) - std_now)
                .min()
                .unwrap_or_default();
            wait = wait.max(soonest);
            if now + wait >= deadline {
                tracing::warn!(
                    attempts,
                    "OCR gave up: no candidate available before the deadline"
                );
                return Err(ReadError::Unavailable);
            }
            tokio::time::sleep(wait).await;
        }
        Err(ReadError::Unavailable)
    }

    fn launch<'a>(
        &'a self,
        index: usize,
        number: u32,
        bodies: &'a Bodies,
        progress: &UnboundedSender<Attempt>,
        flights: &mut FuturesUnordered<Flight<'a>>,
    ) {
        let candidate = &self.candidates[index];
        let _ = progress.send(Attempt {
            number,
            candidate: candidate.id.clone(),
        });
        flights.push(Box::pin(async move {
            let started = Instant::now();
            let result = self.request(candidate, bodies).await;
            (index, result, started.elapsed())
        }));
    }

    async fn request(&self, candidate: &Candidate, bodies: &Bodies) -> Result<Receipt, Failure> {
        let request = match &candidate.endpoint {
            Endpoint::Studio { base_url, key } => self
                .client
                .post(format!(
                    "{base_url}/v1beta/models/{}:generateContent",
                    candidate.model
                ))
                .header("x-goog-api-key", key),
            Endpoint::Vertex { models_url } => {
                let Some(auth) = &self.vertex else {
                    return Err(Failure::Misconfigured);
                };
                let token = auth.token(&self.client).await.map_err(|error| {
                    tracing::error!(%error, candidate = %candidate.id, "no Vertex AI access token");
                    Failure::Retryable { retry_after: None }
                })?;
                self.client
                    .post(format!("{models_url}/{}:generateContent", candidate.model))
                    .bearer_auth(token)
            }
        };
        let body = if candidate.priority {
            bodies.priority.clone()
        } else {
            bodies.standard.clone()
        };
        let response = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, candidate = %candidate.id, "OCR request failed");
                Failure::Retryable { retry_after: None }
            })?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let detail = response.text().await.unwrap_or_default();
            let detail: String = detail.chars().take(500).collect();
            tracing::warn!(candidate = %candidate.id, %status, %detail, "OCR request refused");
            return Err(match status.as_u16() {
                400 => Failure::Refused,
                401 | 403 | 404 => Failure::Misconfigured,
                _ => Failure::Retryable { retry_after },
            });
        }
        if let Some(tier) = response.headers().get("x-gemini-service-tier") {
            tracing::info!(candidate = %candidate.id, tier = ?tier, "served tier");
        }
        let answer: Value = response.json().await.map_err(|error| {
            tracing::warn!(%error, candidate = %candidate.id, "OCR answer cut short");
            Failure::Retryable { retry_after: None }
        })?;
        let text: String = answer["candidates"][0]["content"]["parts"]
            .as_array()
            .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect())
            .unwrap_or_default();
        serde_json::from_str::<Receipt>(&text).map_err(|error| {
            let finish = &answer["candidates"][0]["finishReason"];
            let blocked = &answer["promptFeedback"]["blockReason"];
            tracing::warn!(%error, candidate = %candidate.id, %finish, %blocked, "OCR answer is not a receipt");
            Failure::Content
        })
    }
}

/// The request body, built once per scan (the image dominates it).
struct Bodies {
    standard: Vec<u8>,
    priority: Vec<u8>,
}

impl Bodies {
    fn new(webp: &[u8]) -> Self {
        let mut body = json!({
            "contents": [{"role": "user", "parts": [
                {"inlineData": {"mimeType": "image/webp", "data": STANDARD.encode(webp)}},
                {"text": PROMPT},
            ]}],
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseSchema": response_schema(),
                "temperature": 0,
                "mediaResolution": "MEDIA_RESOLUTION_HIGH",
            },
        });
        let standard = serde_json::to_vec(&body).unwrap_or_default();
        body["service_tier"] = json!("priority");
        let priority = serde_json::to_vec(&body).unwrap_or_default();
        Self { standard, priority }
    }
}
