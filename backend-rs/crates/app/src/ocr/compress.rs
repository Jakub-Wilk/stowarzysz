//! Fitting a receipt photo (up to 50 MB) under the model's inline budget without losing
//! legibility: decode and turn upright, fit to `max_side`, then the first of these that fits:
//! lossless WebP, WebP at quality 92, and again a fifth smaller, down to [`MIN_SIDE`]. Nothing is
//! written to disk; the result lives in memory for one request.

use std::path::Path;

use crate::media::ImageError;
use crate::media::pipeline::{decode, encode_with, fit};

/// Below this the small print of a long receipt stops being legible.
pub const MIN_SIDE: u32 = 1536;
const LOSSY_QUALITY: f32 = 92.0;

/// An image ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compressed {
    pub webp: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub lossless: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CompressError {
    #[error(transparent)]
    Image(#[from] ImageError),
    /// Even the smallest legible version is over the budget.
    #[error("does not fit the budget")]
    TooBig,
}

/// See the module docs. CPU-heavy: run on the blocking pool.
pub fn compress(
    path: &Path,
    max_side: u32,
    budget: u64,
    effort: u8,
) -> Result<Compressed, CompressError> {
    let image = decode(path)?;
    let longest = image.width().max(image.height());
    let mut side = max_side.min(longest);
    loop {
        let fitted = fit(&image, side)?;
        for lossless in [true, false] {
            let effort = lossless.then_some(effort);
            let webp = encode_with(&fitted, effort, LOSSY_QUALITY)?;
            if u64::try_from(webp.len()).unwrap_or(u64::MAX) <= budget {
                return Ok(Compressed {
                    webp,
                    width: fitted.width(),
                    height: fitted.height(),
                    lossless,
                });
            }
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // shrinking
        let smaller = (f64::from(side) * 0.8).round() as u32;
        if smaller < MIN_SIDE {
            return Err(CompressError::TooBig);
        }
        side = smaller;
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageFormat, RgbImage};

    use super::*;

    /// Pseudo-random pixels: incompressible, so sizes are predictable.
    fn noise(width: u32, height: u32) -> tempfile::NamedTempFile {
        let mut state = 0x2545_f491_u32;
        let image = RgbImage::from_fn(width, height, |_, _| {
            let mut channel = || {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state.to_le_bytes()[0]
            };
            image::Rgb([channel(), channel(), channel()])
        });
        let file = tempfile::NamedTempFile::new().unwrap();
        DynamicImage::ImageRgb8(image)
            .save_with_format(file.path(), ImageFormat::Png)
            .unwrap();
        file
    }

    #[test]
    fn a_small_photo_stays_lossless_and_unscaled() {
        let file = noise(300, 200);
        let out = compress(file.path(), 3072, 10 * 1024 * 1024, 0).unwrap();
        assert!(out.lossless);
        assert_eq!((out.width, out.height), (300, 200));
        let decoded = image::load_from_memory(&out.webp).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (300, 200));
    }

    #[test]
    fn over_the_budget_it_goes_lossy_then_smaller() {
        let file = noise(2400, 1800);
        // Lossless noise is ~3 bytes a pixel (13 MB here): far over 1.5 MB.
        let out = compress(file.path(), 3072, 1_500_000, 0).unwrap();
        assert!(!out.lossless);
        assert!(out.webp.len() <= 1_500_000);
        assert!(out.width.max(out.height) >= MIN_SIDE);
        assert_eq!(out.width * 3, out.height * 4, "aspect ratio kept");
    }

    #[test]
    fn a_budget_nothing_legible_fits_is_refused() {
        let file = noise(2000, 2000);
        assert!(matches!(
            compress(file.path(), 3072, 10_000, 0),
            Err(CompressError::TooBig)
        ));
        let not_an_image = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(not_an_image.path(), b"hello").unwrap();
        assert!(matches!(
            compress(not_an_image.path(), 3072, 10_000, 0),
            Err(CompressError::Image(_))
        ));
    }
}
