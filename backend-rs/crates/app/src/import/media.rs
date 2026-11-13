//! Moving the old `MEDIA_ROOT` files into the new layout (`<prefix>/<uuid>/{master,sm,md}.webp`).
//! The legacy file is the master as it is; the variants are generated. A file referenced twice
//! (a profile picture that came from a vote) becomes one key, shared. Everything written is
//! remembered, so a failed or dry run leaves nothing behind.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::media::pipeline::adopt;
use crate::media::{Media, Purpose, StoredImage, Variant};

pub struct MediaImport {
    media: Media,
    from: Option<PathBuf>,
    done: HashMap<String, StoredImage>,
    /// Directories created so far.
    written: Vec<PathBuf>,
    /// Referenced files that were not there (`what`, legacy path).
    pub missing: Vec<(String, String)>,
}

impl MediaImport {
    pub fn new(media: &Media, from: Option<PathBuf>) -> Self {
        Self {
            media: media.clone(),
            from,
            done: HashMap::new(),
            written: Vec::new(),
            missing: Vec::new(),
        }
    }

    pub fn images(&self) -> usize {
        self.done.len()
    }

    /// Import the legacy file `legacy` (a path relative to the old `MEDIA_ROOT`) for `what`
    /// (shown if it is missing). `None` if it is missing: the reference is dropped and reported.
    pub async fn adopt(
        &mut self,
        what: String,
        legacy: &str,
        purpose: Purpose,
    ) -> anyhow::Result<Option<StoredImage>> {
        if let Some(stored) = self.done.get(legacy) {
            return Ok(Some(stored.clone()));
        }
        let Some(root) = &self.from else {
            anyhow::bail!("{what} references {legacy}: pass --media-from <old MEDIA_ROOT>");
        };
        let source = safe_join(root, legacy)?;
        if !source.is_file() {
            self.missing.push((what, legacy.to_owned()));
            return Ok(None);
        }
        // Keep the old file's id when it is one (`avatars/<32 hex>.webp`).
        let id = Path::new(legacy)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| Uuid::try_parse(stem).ok())
            .unwrap_or_else(Uuid::new_v4);
        let key = format!("{}/{}", purpose.prefix(), id.hyphenated());
        let dir = self.media.dir(&key);
        anyhow::ensure!(!dir.exists(), "{} already exists", dir.display());
        let effort = self.media.lossless_effort();
        let target = dir.clone();
        let processed = tokio::task::spawn_blocking(move || {
            let processed = adopt(&source, purpose, effort)
                .map_err(|error| anyhow::anyhow!("{}: {error}", source.display()))?;
            std::fs::create_dir_all(&target)?;
            std::fs::write(target.join(Variant::Master.file()), &processed.master)?;
            std::fs::write(target.join(Variant::Small.file()), &processed.small)?;
            std::fs::write(target.join(Variant::Medium.file()), &processed.medium)?;
            anyhow::Ok(processed)
        })
        .await?;
        self.written.push(dir);
        let processed = processed?;
        let stored = StoredImage {
            key,
            width: i32::try_from(processed.width)?,
            height: i32::try_from(processed.height)?,
        };
        self.done.insert(legacy.to_owned(), stored.clone());
        Ok(Some(stored))
    }

    /// Delete everything written (a failed or dry run).
    pub fn discard(&mut self) {
        for dir in self.written.drain(..) {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.done.clear();
    }
}

/// `root/relative`, refusing paths that would leave `root`.
fn safe_join(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(relative);
    anyhow::ensure!(
        path.components()
            .all(|c| matches!(c, std::path::Component::Normal(_))),
        "unexpected media path {relative:?}"
    );
    Ok(root.join(path))
}
