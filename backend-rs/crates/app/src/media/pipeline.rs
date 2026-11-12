//! Turning an uploaded file into a stored image: decode (format sniffed from the content, never
//! trusted from the client), turn upright, crop or fit, then encode a lossless WebP master and
//! lossy WebP variants. Re-encoding also drops every bit of metadata (EXIF, GPS).
//!
//! CPU- and memory-heavy: run it on the blocking pool (see [`super::store_upload`]).

use std::io::BufReader;
use std::path::Path;

use fast_image_resize::images::Image as Buffer;
use fast_image_resize::{FilterType, IntoImageView as _, ResizeAlg, ResizeOptions, Resizer};
use image::{
    DynamicImage, ImageDecoder as _, ImageFormat, ImageReader, Limits, RgbImage, RgbaImage,
};

use super::{ImageError, Purpose};

/// Bigger images are refused before they are decoded (a 100 MP RGBA image is 400 MB).
pub const MAX_PIXELS: u64 = 100_000_000;
const AVATAR_MASTER: u32 = 1024;
const PHOTO_MASTER: u32 = 3072;
const LOSSY_QUALITY: f32 = 80.0;

/// libwebp's lossless presets (`WebPConfigLosslessPreset`): (method, quality) per level 0-9.
const LOSSLESS_PRESETS: [(i32, f32); 10] = [
    (0, 0.0),
    (1, 20.0),
    (2, 25.0),
    (3, 30.0),
    (3, 50.0),
    (4, 50.0),
    (4, 75.0),
    (4, 90.0),
    (5, 90.0),
    (6, 100.0),
];

/// The encoded files of one image.
#[derive(Debug)]
pub struct Processed {
    pub width: u32,
    pub height: u32,
    pub master: Vec<u8>,
    pub small: Vec<u8>,
    pub medium: Vec<u8>,
}

/// Process the image in `path` for `purpose`. `lossless_effort` is libwebp's preset level.
pub fn process(
    path: &Path,
    purpose: Purpose,
    lossless_effort: u8,
) -> Result<Processed, ImageError> {
    let image = decode(path)?;
    let master = match purpose {
        Purpose::Avatar => square(&image, AVATAR_MASTER)?,
        Purpose::Photo => fit(&image, PHOTO_MASTER)?,
    };
    let (small, medium) = match purpose {
        Purpose::Avatar => (square(&master, 128)?, square(&master, 512)?),
        Purpose::Photo => (fit(&master, 512)?, fit(&master, 1600)?),
    };
    Ok(Processed {
        width: master.width(),
        height: master.height(),
        master: encode(&master, Some(lossless_effort))?,
        small: encode(&small, None)?,
        medium: encode(&medium, None)?,
    })
}

fn decode(path: &Path) -> Result<DynamicImage, ImageError> {
    let file = std::fs::File::open(path).map_err(anyhow::Error::from)?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(anyhow::Error::from)?;
    match reader.format() {
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP | ImageFormat::Gif) => {}
        Some(_) => return Err(ImageError::Unsupported),
        None => return Err(ImageError::Invalid),
    }
    let mut limits = Limits::default();
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| ImageError::Invalid)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ImageError::TooManyPixels);
    }
    let orientation = decoder.orientation().map_err(|_| ImageError::Invalid)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|error| match error {
        image::ImageError::Limits(_) => ImageError::TooManyPixels,
        _ => ImageError::Invalid,
    })?;
    image.apply_orientation(orientation);
    Ok(if has_transparency(&image) {
        DynamicImage::ImageRgba8(image.into_rgba8())
    } else {
        DynamicImage::ImageRgb8(image.into_rgb8())
    })
}

fn has_transparency(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageRgba8(pixels) => pixels.pixels().any(|p| p[3] < u8::MAX),
        other if other.color().has_alpha() => other.to_rgba8().pixels().any(|p| p[3] < u8::MAX),
        _ => false,
    }
}

/// Centre-cropped to a square, at most `side` pixels (never enlarged).
fn square(image: &DynamicImage, side: u32) -> Result<DynamicImage, ImageError> {
    let (width, height) = (image.width(), image.height());
    let crop = width.min(height);
    let target = crop.min(side);
    if width == height && target == width {
        return Ok(image.clone()); // already a square of the right size
    }
    let left = f64::from(width - crop) / 2.0;
    let top = f64::from(height - crop) / 2.0;
    resize(
        image,
        target,
        target,
        Some((left, top, f64::from(crop), f64::from(crop))),
    )
}

/// Scaled so the longest side is at most `max_side` (never enlarged).
fn fit(image: &DynamicImage, max_side: u32) -> Result<DynamicImage, ImageError> {
    let (width, height) = (image.width(), image.height());
    let longest = width.max(height);
    if longest <= max_side {
        return Ok(image.clone());
    }
    let scale = f64::from(max_side) / f64::from(longest);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // scaled down, positive
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    resize(image, scaled(width), scaled(height), None)
}

fn resize(
    image: &DynamicImage,
    width: u32,
    height: u32,
    crop: Option<(f64, f64, f64, f64)>,
) -> Result<DynamicImage, ImageError> {
    let unchanged = (width, height) == (image.width(), image.height());
    if unchanged && crop.is_none() {
        return Ok(image.clone());
    }
    let pixel_type = image
        .pixel_type()
        .ok_or_else(|| anyhow::anyhow!("unsupported pixel type"))?;
    let mut target = Buffer::new(width, height, pixel_type);
    let mut options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    if let Some((left, top, w, h)) = crop {
        options = options.crop(left, top, w, h);
    }
    Resizer::new()
        .resize(image, &mut target, &options)
        .map_err(anyhow::Error::from)?;
    let pixels = target.into_vec();
    let resized = match image {
        DynamicImage::ImageRgba8(_) => {
            RgbaImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgba8)
        }
        _ => RgbImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgb8),
    };
    Ok(resized.ok_or_else(|| anyhow::anyhow!("resized buffer has the wrong size"))?)
}

/// WebP: lossless at `effort` (a libwebp preset level), else lossy.
fn encode(image: &DynamicImage, lossless_effort: Option<u8>) -> Result<Vec<u8>, ImageError> {
    let encoder = match image {
        DynamicImage::ImageRgba8(pixels) => {
            webp::Encoder::from_rgba(pixels.as_raw(), image.width(), image.height())
        }
        DynamicImage::ImageRgb8(pixels) => {
            webp::Encoder::from_rgb(pixels.as_raw(), image.width(), image.height())
        }
        _ => return Err(anyhow::anyhow!("unexpected pixel layout").into()),
    };
    let bytes = match lossless_effort {
        Some(effort) => {
            let (method, quality) = LOSSLESS_PRESETS[usize::from(effort.min(9))];
            let mut config =
                webp::WebPConfig::new().map_err(|()| anyhow::anyhow!("libwebp config"))?;
            config.lossless = 1;
            config.method = method;
            config.quality = quality;
            encoder
                .encode_advanced(&config)
                .map_err(|error| anyhow::anyhow!("webp encoding failed: {error:?}"))?
                .to_vec()
        }
        None => encoder.encode(LOSSY_QUALITY).to_vec(),
    };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use image::{ImageEncoder as _, Rgb, Rgba};

    use super::*;

    fn write(image: &DynamicImage, format: ImageFormat) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        image.save_with_format(file.path(), format).unwrap();
        file
    }

    fn webp_size(bytes: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory_with_format(bytes, ImageFormat::WebP).unwrap();
        (image.width(), image.height())
    }

    fn gradient(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            #[allow(clippy::cast_possible_truncation)]
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        }))
    }

    #[test]
    fn avatars_are_square_and_capped() {
        let file = write(&gradient(2000, 1200), ImageFormat::Png);
        let out = process(file.path(), Purpose::Avatar, 0).unwrap();
        assert_eq!((out.width, out.height), (1024, 1024));
        assert_eq!(webp_size(&out.master), (1024, 1024));
        assert_eq!(webp_size(&out.small), (128, 128));
        assert_eq!(webp_size(&out.medium), (512, 512));
    }

    #[test]
    fn small_avatars_are_cropped_but_not_enlarged() {
        let file = write(&gradient(300, 200), ImageFormat::Jpeg);
        let out = process(file.path(), Purpose::Avatar, 0).unwrap();
        assert_eq!((out.width, out.height), (200, 200));
        assert_eq!(webp_size(&out.small), (128, 128));
        assert_eq!(webp_size(&out.medium), (200, 200));
    }

    #[test]
    fn photos_keep_their_aspect_ratio() {
        let file = write(&gradient(4000, 1000), ImageFormat::Png);
        let out = process(file.path(), Purpose::Photo, 0).unwrap();
        assert_eq!((out.width, out.height), (3072, 768));
        assert_eq!(webp_size(&out.medium), (1600, 400));
        assert_eq!(webp_size(&out.small), (512, 128));
    }

    #[test]
    fn the_master_is_lossless() {
        let original = gradient(640, 480);
        let file = write(&original, ImageFormat::Png);
        let out = process(file.path(), Purpose::Photo, 4).unwrap();
        let decoded = image::load_from_memory_with_format(&out.master, ImageFormat::WebP).unwrap();
        assert_eq!(decoded.to_rgb8().as_raw(), original.to_rgb8().as_raw());
    }

    #[test]
    fn transparency_is_kept_only_where_present() {
        let transparent = DynamicImage::ImageRgba8(RgbaImage::from_fn(64, 64, |x, _| {
            Rgba([255, 0, 0, if x < 32 { 0 } else { 255 }])
        }));
        let out = process(
            write(&transparent, ImageFormat::Png).path(),
            Purpose::Photo,
            0,
        )
        .unwrap();
        let decoded = image::load_from_memory_with_format(&out.master, ImageFormat::WebP).unwrap();
        assert!(decoded.color().has_alpha());

        let opaque = DynamicImage::ImageRgba8(RgbaImage::from_pixel(64, 64, Rgba([1, 2, 3, 255])));
        let out = process(write(&opaque, ImageFormat::Png).path(), Purpose::Photo, 0).unwrap();
        let decoded = image::load_from_memory_with_format(&out.master, ImageFormat::WebP).unwrap();
        assert!(!decoded.color().has_alpha());
    }

    #[test]
    fn exif_orientation_is_applied_and_metadata_dropped() {
        // A 40x20 JPEG tagged "rotate 90° clockwise" (orientation 6) is shown as 20x40.
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .write_image(
                gradient(40, 20).to_rgb8().as_raw(),
                40,
                20,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let tagged = with_exif_orientation(&jpeg, 6);
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), &tagged).unwrap();
        let out = process(file.path(), Purpose::Photo, 0).unwrap();
        assert_eq!((out.width, out.height), (20, 40));
        assert!(!out.master.windows(4).any(|w| w == b"EXIF" || w == b"Exif"));
    }

    /// Insert an APP1 EXIF segment holding only an orientation tag after the JPEG's SOI marker.
    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II*\0");
        tiff.extend_from_slice(&8_u32.to_le_bytes()); // IFD0 offset
        tiff.extend_from_slice(&1_u16.to_le_bytes()); // one entry
        tiff.extend_from_slice(&0x0112_u16.to_le_bytes()); // Orientation
        tiff.extend_from_slice(&3_u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1_u32.to_le_bytes()); // count
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0_u32.to_le_bytes()); // no next IFD
        let mut segment = b"Exif\0\0".to_vec();
        segment.extend_from_slice(&tiff);
        let length = u16::try_from(segment.len() + 2).unwrap();
        let mut out = jpeg[..2].to_vec(); // SOI
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&segment);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn refuses_non_images_and_other_formats() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"definitely not an image").unwrap();
        assert!(matches!(
            process(file.path(), Purpose::Photo, 0),
            Err(ImageError::Invalid)
        ));

        // Recognised (BMP magic) but not accepted.
        std::fs::write(file.path(), b"BM\0\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0").unwrap();
        assert!(matches!(
            process(file.path(), Purpose::Photo, 0),
            Err(ImageError::Unsupported)
        ));
    }

    #[test]
    fn refuses_too_many_pixels_before_decoding() {
        // A well-formed PNG claiming 20000x20000 pixels (400 MP): only its header is read.
        fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
            let mut crc = flate2::Crc::new();
            crc.update(&kind);
            crc.update(data);
            let mut out = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
            out.extend_from_slice(&kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc.sum().to_be_bytes());
            out
        }
        let mut ihdr = 20_000_u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&20_000_u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(*b"IHDR", &ihdr));
        png.extend(chunk(
            *b"IDAT",
            &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
        ));
        png.extend(chunk(*b"IEND", &[]));
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), &png).unwrap();
        assert!(matches!(
            process(file.path(), Purpose::Photo, 0),
            Err(ImageError::TooManyPixels)
        ));
    }
}
