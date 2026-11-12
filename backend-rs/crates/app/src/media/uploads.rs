use chrono::{DateTime, TimeDelta, Utc};
use domain::DomainError;
use serde::Serialize;
use tempfile::TempPath;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{Image, Purpose, pipeline};
use crate::db::Uow;
use crate::{AppResult, Context};

/// An unclaimed upload expires (and is swept) this long after it was made.
pub const UPLOAD_TTL: TimeDelta = TimeDelta::minutes(30);

/// A processed upload, ready to be used by id.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadView {
    /// Pass this where an image is expected (e.g. `upload_id`).
    pub id: Uuid,
    pub purpose: Purpose,
    pub width: u32,
    pub height: u32,
    /// A preview of the stored image.
    pub image: Image,
    /// When the upload is discarded unless used.
    pub expires_at: DateTime<Utc>,
}

/// An upload taken into use: what the row that references it stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredImage {
    pub key: String,
    pub width: i32,
    pub height: i32,
}

/// Process the file at `source` (deleted when this returns, whatever happens) and keep it as an
/// unclaimed upload of `owner_id`.
pub async fn store_upload(
    ctx: &Context,
    owner_id: i64,
    purpose: Purpose,
    source: TempPath,
) -> AppResult<UploadView> {
    let media = ctx.media.clone();
    let id = Uuid::new_v4();
    let key = format!("{}/{}", purpose.prefix(), id.hyphenated());
    let dir = media.dir(&key);
    let staging = media.tmp_dir().join(id.hyphenated().to_string());

    let (width, height) = {
        let _permit = media.permits.acquire().await.map_err(anyhow::Error::from)?;
        let effort = media.lossless_effort;
        let staging = staging.clone();
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || -> AppResult<(u32, u32)> {
            let processed = pipeline::process(&source, purpose, effort)?;
            drop(source); // the raw upload is not needed any more
            let write = || -> std::io::Result<()> {
                std::fs::create_dir_all(&staging)?;
                std::fs::write(
                    staging.join(super::Variant::Master.file()),
                    &processed.master,
                )?;
                std::fs::write(staging.join(super::Variant::Small.file()), &processed.small)?;
                std::fs::write(
                    staging.join(super::Variant::Medium.file()),
                    &processed.medium,
                )?;
                std::fs::rename(&staging, &dir) // all files appear at once
            };
            if let Err(error) = write() {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(anyhow::Error::from(error).into());
            }
            Ok((processed.width, processed.height))
        })
        .await
        .map_err(anyhow::Error::from)??
    };

    let now = ctx.clock.now();
    let expires_at = now + UPLOAD_TTL;
    let inserted = sqlx::query!(
        "insert into uploads (id, owner_id, purpose, key, width, height, created_at, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8)",
        id,
        owner_id,
        purpose.as_str(),
        key,
        i32::try_from(width).map_err(anyhow::Error::from)?,
        i32::try_from(height).map_err(anyhow::Error::from)?,
        now,
        expires_at,
    )
    .execute(&ctx.db)
    .await;
    if let Err(error) = inserted {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(error.into());
    }
    Ok(UploadView {
        id,
        purpose,
        width,
        height,
        image: media.image(&key, now),
        expires_at,
    })
}

/// Take `owner_id`'s unexpired upload `id` of `purpose` into use, in `uow`'s transaction.
pub async fn take_upload(
    uow: &mut Uow,
    ctx: &Context,
    owner_id: i64,
    id: Uuid,
    purpose: Purpose,
) -> AppResult<StoredImage> {
    let taken = sqlx::query!(
        "delete from uploads
         where id = $1 and owner_id = $2 and purpose = $3 and expires_at > $4
         returning key, width, height, purpose",
        id,
        owner_id,
        purpose.as_str(),
        ctx.clock.now(),
    )
    .fetch_optional(uow.conn())
    .await?;
    let taken = taken.ok_or_else(|| {
        DomainError::new(
            "upload_not_found",
            "Przesłany obraz wygasł lub nie istnieje. Prześlij go ponownie.",
        )
        .on("upload_id")
    })?;
    debug_assert_eq!(Purpose::from_db(&taken.purpose), purpose);
    Ok(StoredImage {
        key: taken.key,
        width: taken.width,
        height: taken.height,
    })
}
