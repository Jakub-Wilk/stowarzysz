//! Media housekeeping: unclaimed uploads expire, trashed files are deleted, and (weekly) any
//! file no row references is removed.

use std::collections::HashSet;
use std::time::SystemTime;

use chrono::{DateTime, TimeDelta, Utc};
use futures_util::future::BoxFuture;

use super::{Purpose, TMP_DIR};
use crate::Context;
use crate::scheduler::{Claim, Job, JobResult, PeriodicRun};

const SWEEP_BATCH: i64 = 200;
/// Files younger than this are never treated as orphans (an upload may be mid-flight).
const ORPHAN_GRACE: TimeDelta = TimeDelta::hours(1);

/// Expires unclaimed uploads and deletes trashed files, as soon as either is due.
#[derive(Debug, Clone, Copy)]
pub struct MediaSweeper;

impl MediaSweeper {
    async fn sweep(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        // Unclaimed uploads become trash; both happen in one statement.
        sqlx::query!(
            "with expired as (delete from uploads where expires_at <= $1 returning key)
             insert into media_trash (key, due_at) select key, $1 from expired
             on conflict do nothing",
            now,
        )
        .execute(&ctx.db)
        .await?;

        loop {
            let mut tx = ctx.db.begin().await?;
            let due = sqlx::query_scalar!(
                "select key from media_trash where due_at <= $1 order by due_at
                 limit $2 for update skip locked",
                now,
                SWEEP_BATCH,
            )
            .fetch_all(&mut *tx)
            .await?;
            for key in &due {
                let dir = ctx.media.dir(key);
                match tokio::fs::remove_dir_all(&dir).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        tracing::warn!(key, %error, "cannot delete media; will retry");
                        continue;
                    }
                }
                sqlx::query!("delete from media_trash where key = $1", key)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
            if due.len() < usize::try_from(SWEEP_BATCH)? {
                break;
            }
        }

        let next = sqlx::query_scalar!(
            r#"select least((select min(expires_at) from uploads), (select min(due_at) from media_trash))"#
        )
        .fetch_one(&ctx.db)
        .await?;
        Ok(next)
    }
}

impl Job for MediaSweeper {
    fn name(&self) -> &'static str {
        "media-sweeper"
    }

    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult> {
        Box::pin(Self::sweep(ctx, now))
    }
}

/// Weekly: delete image directories no row references (left by a crash between writing files
/// and recording them), and stale files in the upload scratch directory.
#[derive(Debug, Clone, Copy)]
pub struct MediaGc;

impl MediaGc {
    const EVERY: TimeDelta = TimeDelta::days(7);

    async fn collect(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        let run = match PeriodicRun::claim(&ctx.db, "media-gc", now).await? {
            Claim::Run(run) => run,
            Claim::NotDue(next) => return Ok(Some(next)),
        };
        let referenced: HashSet<String> = sqlx::query_scalar!(
            r#"select key as "key!" from (
                   select avatar_key as key from users
                   union all select proposed_image_key from polls
                   union all select previous_image_key from polls
                   union all select image_key from pact_attachments
                   union all select image_key from ledger_attachments
                   union all select key from uploads
                   union all select key from media_trash
               ) keys where key is not null"#
        )
        .fetch_all(&ctx.db)
        .await?
        .into_iter()
        .collect();

        let root = ctx.media.root().to_path_buf();
        let cutoff = SystemTime::from(now - ORPHAN_GRACE);
        let removed = tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
            let mut removed = 0;
            let old = |path: &std::path::Path| {
                std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .is_ok_and(|at| at < cutoff)
            };
            for prefix in [Purpose::Avatar.prefix(), Purpose::Photo.prefix()] {
                let Ok(entries) = std::fs::read_dir(root.join(prefix)) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let key = format!("{prefix}/{}", entry.file_name().to_string_lossy());
                    if !referenced.contains(&key) && old(&entry.path()) {
                        std::fs::remove_dir_all(entry.path())?;
                        removed += 1;
                    }
                }
            }
            if let Ok(entries) = std::fs::read_dir(root.join(TMP_DIR)) {
                for entry in entries.flatten() {
                    if old(&entry.path()) {
                        let path = entry.path();
                        let _ = if path.is_dir() {
                            std::fs::remove_dir_all(path)
                        } else {
                            std::fs::remove_file(path)
                        };
                    }
                }
            }
            Ok(removed)
        })
        .await??;
        if removed > 0 {
            tracing::warn!(removed, "deleted unreferenced media");
        }
        let next = now + Self::EVERY;
        run.finish(now, next).await?;
        Ok(Some(next))
    }
}

impl Job for MediaGc {
    fn name(&self) -> &'static str {
        "media-gc"
    }

    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult> {
        Box::pin(Self::collect(ctx, now))
    }
}
