//! Stored images: layout on disk, signed URLs, staged uploads and deferred deletion.
//!
//! An image is a directory `MEDIA_ROOT/<prefix>/<uuid>/` named by its *key* (`avatars/<uuid>` or
//! `photos/<uuid>`), holding a lossless `master.webp` and lossy `sm.webp` / `md.webp` variants.
//! Files never move: an upload is written straight to its final place and an `uploads` row marks
//! it as unclaimed. Using it ([`take_upload`]) deletes that row in the same transaction that
//! stores the key, so a rollback leaves it unclaimed (and swept after 30 minutes). Deleting is
//! deferred the same way ([`trash`]). Nothing on disk is ever referenced by a row that did not
//! commit, and nothing referenced is ever deleted.
//!
//! Files are served by the API (not the web server) through HMAC-signed URLs that stay valid
//! for at least a day and do not change within a day, so browsers can cache them.

pub mod jobs;
pub mod pipeline;
mod uploads;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::sync::Semaphore;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Uow;
use crate::{AppError, AppResult};
use domain::DomainError;

pub use uploads::{UploadView, store_upload, take_upload};

const DAY_SECS: i64 = 86_400;
/// Where uploads are streamed to before processing (same filesystem, so renames are cheap).
pub const TMP_DIR: &str = ".tmp";

/// What an image is for: decides its size and where it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// A profile picture: square.
    Avatar,
    /// A photo looked at as a whole (receipts, attachments): aspect ratio kept.
    Photo,
}

impl Purpose {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Avatar => "avatars",
            Self::Photo => "photos",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Avatar => "avatar",
            Self::Photo => "photo",
        }
    }

    fn from_db(value: &str) -> Self {
        if value == "avatar" {
            Self::Avatar
        } else {
            Self::Photo
        }
    }
}

/// The files of one image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Master,
    Small,
    Medium,
}

impl Variant {
    pub const ALL: [Self; 3] = [Self::Master, Self::Small, Self::Medium];

    pub fn file(self) -> &'static str {
        match self {
            Self::Master => "master.webp",
            Self::Small => "sm.webp",
            Self::Medium => "md.webp",
        }
    }

    pub fn from_file(file: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.file() == file)
    }
}

/// Signed URLs of an image. Show `sm` in lists and avatars, `md` in a detail view, `url` (the
/// lossless master) when zoomed in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Image {
    /// The full-quality (lossless) image.
    pub url: String,
    /// Small preview (128 px avatars, 512 px photos).
    pub sm: String,
    /// Medium size (512 px avatars, 1600 px photos).
    pub md: String,
}

/// The media store of this process.
#[derive(Clone)]
pub struct Media {
    root: PathBuf,
    signing_key: Vec<u8>,
    lossless_effort: u8,
    permits: Arc<Semaphore>,
}

impl std::fmt::Debug for Media {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Media")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Media {
    pub fn new(config: &Config) -> Self {
        Self {
            root: config.media_root.clone(),
            signing_key: config.secrets.media_signing.expose().as_bytes().to_vec(),
            lossless_effort: config.images.lossless_effort,
            // Decoding a 50 MB upload can take hundreds of MB; at most two at a time.
            permits: Arc::new(Semaphore::new(2)),
        }
    }

    /// A turn at memory-heavy image work (at most two at a time).
    pub async fn heavy_permit(&self) -> AppResult<tokio::sync::SemaphorePermit<'_>> {
        Ok(self.permits.acquire().await.map_err(anyhow::Error::from)?)
    }

    /// libwebp's lossless preset level for masters.
    pub fn lossless_effort(&self) -> u8 {
        self.lossless_effort
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the directory layout and clear leftovers of interrupted uploads.
    pub async fn init(&self) -> anyhow::Result<()> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            for prefix in [Purpose::Avatar.prefix(), Purpose::Photo.prefix()] {
                std::fs::create_dir_all(root.join(prefix))?;
            }
            let tmp = root.join(TMP_DIR);
            if tmp.exists() {
                std::fs::remove_dir_all(&tmp)?;
            }
            std::fs::create_dir_all(tmp)?;
            Ok(())
        })
        .await?
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join(TMP_DIR)
    }

    /// The directory of `key` (which must be valid, see [`parse_key`]).
    pub fn dir(&self, key: &str) -> PathBuf {
        key.split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    pub fn file(&self, key: &str, variant: Variant) -> PathBuf {
        self.dir(key).join(variant.file())
    }

    /// Signed URLs for `key`, valid until the end of the next UTC day.
    pub fn image(&self, key: &str, now: DateTime<Utc>) -> Image {
        let expires = day(now) + 1;
        let url = |variant: Variant| {
            let file = variant.file();
            let signature = self.sign(key, file, expires);
            format!("/api/media/{key}/{file}?e={expires}&s={signature}")
        };
        Image {
            url: url(Variant::Master),
            sm: url(Variant::Small),
            md: url(Variant::Medium),
        }
    }

    /// Whether a media URL's signature is genuine and unexpired. Returns the seconds it stays
    /// valid (for caching).
    pub fn verify(
        &self,
        key: &str,
        file: &str,
        expires: i64,
        signature: &str,
        now: DateTime<Utc>,
    ) -> Option<i64> {
        let today = day(now);
        if expires < today || expires > today + 2 {
            return None;
        }
        let given = URL_SAFE_NO_PAD.decode(signature).ok()?;
        self.mac(key, file, expires)
            .verify_truncated_left(&given)
            .ok()?;
        Some((expires + 1) * DAY_SECS - now.timestamp())
    }

    fn sign(&self, key: &str, file: &str, expires: i64) -> String {
        let tag = self.mac(key, file, expires).finalize().into_bytes();
        URL_SAFE_NO_PAD.encode(&tag[..16])
    }

    fn mac(&self, key: &str, file: &str, expires: i64) -> Hmac<Sha256> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.signing_key).expect("HMAC takes any key");
        mac.update(format!("{key}/{file}:{expires}").as_bytes());
        mac
    }
}

/// Days since the Unix epoch (UTC).
fn day(now: DateTime<Utc>) -> i64 {
    now.timestamp().div_euclid(DAY_SECS)
}

/// A key from a URL, if well-formed: `avatars/<uuid>` or `photos/<uuid>`.
pub fn parse_key(prefix: &str, id: &str) -> Option<String> {
    let id = Uuid::parse_str(id).ok()?;
    [Purpose::Avatar, Purpose::Photo]
        .into_iter()
        .any(|p| p.prefix() == prefix)
        .then(|| format!("{prefix}/{}", id.hyphenated()))
}

/// Delete `key`'s files once `uow` commits (no-op for `None`).
pub async fn trash(uow: &mut Uow, key: Option<&str>) -> AppResult<()> {
    if let Some(key) = key {
        sqlx::query!(
            "insert into media_trash (key) values ($1) on conflict do nothing",
            key
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(())
}

/// Why an image was refused.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("not an image")]
    Invalid,
    #[error("unsupported format")]
    Unsupported,
    #[error("too many pixels")]
    TooManyPixels,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<ImageError> for AppError {
    fn from(error: ImageError) -> Self {
        match error {
            ImageError::Invalid => {
                DomainError::new("invalid_image", "Prześlij prawidłowy obraz.").into()
            }
            ImageError::Unsupported => AppError::UnsupportedMedia {
                message: "Nieobsługiwany format obrazu. Prześlij JPEG, PNG, WebP lub GIF.".into(),
            },
            ImageError::TooManyPixels => {
                DomainError::new("image_too_large", "Obraz ma za dużą rozdzielczość.").into()
            }
            ImageError::Internal(error) => AppError::Internal(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media() -> Media {
        Media::new(&Config::for_tests())
    }

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn query(url: &str) -> (i64, String) {
        let query = url.split_once('?').unwrap().1;
        let mut e = 0;
        let mut s = String::new();
        for pair in query.split('&') {
            match pair.split_once('=').unwrap() {
                ("e", value) => e = value.parse().unwrap(),
                ("s", value) => s = value.to_owned(),
                _ => {}
            }
        }
        (e, s)
    }

    #[test]
    fn urls_are_stable_within_a_day_and_valid_for_at_least_a_day() {
        let media = media();
        let key = "avatars/00000000-0000-4000-8000-000000000001";
        let morning = at(20_000 * DAY_SECS + 3600);
        let evening = at(20_000 * DAY_SECS + 23 * 3600);
        assert_eq!(media.image(key, morning), media.image(key, evening));

        let (e, s) = query(&media.image(key, evening).sm);
        let left = media.verify(key, "sm.webp", e, &s, evening).unwrap();
        assert!(left > DAY_SECS, "{left}");
        assert!(
            media
                .verify(key, "sm.webp", e, &s, at(20_002 * DAY_SECS))
                .is_none(),
            "expired"
        );
    }

    #[test]
    fn tampered_urls_are_rejected() {
        let media = media();
        let key = "photos/00000000-0000-4000-8000-000000000002";
        let now = at(20_000 * DAY_SECS);
        let (e, s) = query(&media.image(key, now).url);
        assert!(media.verify(key, "master.webp", e, &s, now).is_some());
        assert!(
            media.verify(key, "md.webp", e, &s, now).is_none(),
            "other file"
        );
        assert!(
            media
                .verify(
                    "photos/00000000-0000-4000-8000-000000000003",
                    "master.webp",
                    e,
                    &s,
                    now
                )
                .is_none()
        );
        assert!(
            media.verify(key, "master.webp", e + 1, &s, now).is_none(),
            "other expiry"
        );
        assert!(media.verify(key, "master.webp", e, "AAAA", now).is_none());
    }

    #[test]
    fn keys_are_validated() {
        let id = "8f14e45f-ceea-467f-a9b7-9a8b4c2d1e3f";
        assert_eq!(
            parse_key("avatars", id).as_deref(),
            Some("avatars/8f14e45f-ceea-467f-a9b7-9a8b4c2d1e3f")
        );
        assert_eq!(
            parse_key("photos", &id.to_uppercase()).as_deref(),
            Some("photos/8f14e45f-ceea-467f-a9b7-9a8b4c2d1e3f")
        );
        assert_eq!(parse_key("etc", id), None);
        assert_eq!(parse_key("avatars", "../../secret"), None);
    }
}
