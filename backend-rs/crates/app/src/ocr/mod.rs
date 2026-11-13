//! Receipt OCR: a photo of up to 50 MB in, a structured [`Receipt`] out. Nothing is stored: the
//! upload is a temporary file deleted when the request ends, and the compressed copy sent to the
//! model only ever lives in memory.
//!
//! At most one scan per member at a time and four in the whole process; each request to a model
//! is announced to the caller over SSE (`ledger.ocr.request`) so the form can show progress.

pub mod compress;
mod gemini;
mod health;
pub mod receipt;
mod vertex;

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tempfile::TempPath;
use tokio::sync::Semaphore;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::Actor;
use crate::config::Config;
use crate::events::{Audience, Event};
use crate::media::Media;
use crate::{AppError, AppResult, Context};

pub use compress::{CompressError, Compressed};
pub use gemini::{Attempt, ReadError, Router, Served};
pub use receipt::Receipt;
pub use vertex::VertexAuth;

/// What a scan returns.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct OcrResult {
    pub receipt: Receipt,
    /// The candidate that read it (endpoint/model[/tier]).
    pub served_by: String,
    /// Requests it took, across candidates.
    pub attempts: u32,
}

/// The OCR service of this process.
#[derive(Debug)]
pub struct Ocr {
    router: Router,
    permits: Semaphore,
    busy: Arc<Mutex<HashSet<i64>>>,
    max_side: u32,
    budget: u64,
}

impl Ocr {
    /// From the configuration; Vertex credentials are read from their file (a broken file is
    /// logged and leaves the Vertex candidates out).
    pub fn new(config: &Config) -> Self {
        let vertex = config.vertex.as_ref().and_then(|vertex| {
            VertexAuth::from_file(&vertex.credentials)
                .inspect_err(|error| {
                    tracing::error!(%error, path = %vertex.credentials.display(), "unusable Vertex AI credentials");
                })
                .ok()
        });
        Self::with_router(config, Router::new(config, vertex))
    }

    pub fn with_router(config: &Config, router: Router) -> Self {
        Self {
            router,
            permits: Semaphore::new(4),
            busy: Arc::default(),
            max_side: config.ocr.max_side,
            budget: config.gemini.inline_budget_bytes,
        }
    }

    pub fn router(&self) -> &Router {
        &self.router
    }

    pub fn is_configured(&self) -> bool {
        self.router.is_configured()
    }

    /// Mark `user_id` as scanning until the guard is dropped (one scan at a time per member).
    fn claim(&self, user_id: i64) -> Option<Busy> {
        let mut busy = self
            .busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        busy.insert(user_id).then(|| Busy {
            set: Arc::clone(&self.busy),
            user_id,
        })
    }
}

struct Busy {
    set: Arc<Mutex<HashSet<i64>>>,
    user_id: i64,
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.user_id);
    }
}

fn unavailable() -> AppError {
    AppError::unavailable(
        "ocr_unavailable",
        "Nie udało się odczytać paragonu, spróbuj ponownie.",
    )
}

/// Shrink the photo at `path` under the budget (on the blocking pool, sharing the image
/// pipeline's memory permits).
pub async fn compress_file(
    media: &Media,
    path: impl AsRef<Path> + Send + 'static,
    max_side: u32,
    budget: u64,
) -> AppResult<Compressed> {
    let _permit = media.heavy_permit().await?;
    let effort = media.lossless_effort();
    tokio::task::spawn_blocking(move || compress::compress(path.as_ref(), max_side, budget, effort))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|error| match error {
            CompressError::Image(error) => error.into(),
            CompressError::TooBig => AppError::TooLarge {
                message: "Tego zdjęcia nie da się zmniejszyć tak, by pozostało czytelne. Zrób je z bliska."
                    .into(),
            },
        })
}

/// Read the receipt photographed in `source` (deleted when this returns) for `actor`.
/// `job_id` tags the progress events so the client can tell its own.
pub async fn scan(
    ctx: &Context,
    actor: &Actor,
    job_id: Uuid,
    source: TempPath,
) -> AppResult<OcrResult> {
    let ocr = &ctx.ocr;
    if !ocr.is_configured() {
        return Err(AppError::unavailable(
            "ocr_not_configured",
            "Odczytywanie paragonów nie jest skonfigurowane.",
        ));
    }
    let _busy = ocr.claim(actor.user_id).ok_or_else(|| {
        AppError::conflict(
            "ocr_in_progress",
            "Poprzedni paragon jest jeszcze odczytywany.",
        )
    })?;
    let _permit = ocr.permits.acquire().await.map_err(anyhow::Error::from)?;
    let compressed = compress_file(&ctx.media, source, ocr.max_side, ocr.budget).await?;

    let (progress, mut attempts) = tokio::sync::mpsc::unbounded_channel::<Attempt>();
    let db = ctx.db.clone();
    let user = actor.user_id;
    let announcer = tokio::spawn(async move {
        while let Some(attempt) = attempts.recv().await {
            let event = Event::LedgerOcrRequest {
                job_id,
                attempt: attempt.number,
                candidate: attempt.candidate,
            };
            if let Err(error) = crate::db::publish_now(&db, Audience::User(user), event).await {
                tracing::warn!(%error, "OCR progress event not sent");
            }
        }
    });
    let served = ocr.router.read(&compressed.webp, &progress).await;
    drop(progress);
    let _ = announcer.await; // progress events go out before the answer
    match served {
        Ok(served) => Ok(OcrResult {
            receipt: served.receipt,
            served_by: served.candidate,
            attempts: served.attempts,
        }),
        Err(ReadError::Unreadable) => Err(domain::DomainError::new(
            "receipt_unreadable",
            "Nie udało się odczytać tego zdjęcia.",
        )
        .into()),
        Err(ReadError::NotConfigured | ReadError::Unavailable) => Err(unavailable()),
    }
}
