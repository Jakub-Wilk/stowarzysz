from io import BytesIO

from django.core.files.base import ContentFile
from django.core.files.uploadedfile import UploadedFile
from PIL import Image, ImageOps, UnidentifiedImageError
from rest_framework import serializers

MAX_UPLOAD_BYTES = 5 * 1024 * 1024
AVATAR_SIZE = 512  # stored square, so every client can render it as-is
PHOTO_MAX_SIDE = 1600  # longest side of an attached picture


def _load(upload: UploadedFile) -> Image.Image:
    """Validate an uploaded image and return it upright (first frame only, RGB or RGBA)."""
    if upload.size is not None and upload.size > MAX_UPLOAD_BYTES:
        raise serializers.ValidationError(
            f"Obraz jest za duży (maks. {MAX_UPLOAD_BYTES // (1024 * 1024)} MB)."
        )
    try:
        with Image.open(upload) as source:  # ty: ignore[invalid-argument-type]
            image = ImageOps.exif_transpose(source)
            mode = "RGBA" if "A" in image.getbands() else "RGB"
            return image.convert(mode)
    except (UnidentifiedImageError, Image.DecompressionBombError, OSError, ValueError) as exc:
        raise serializers.ValidationError("Prześlij prawidłowy obraz.") from exc


def _webp(image: Image.Image) -> ContentFile:
    buffer = BytesIO()
    image.save(buffer, format="WEBP", quality=85)
    return ContentFile(buffer.getvalue())


def process_avatar(upload: UploadedFile) -> ContentFile:
    """Validate an uploaded image and normalise it: upright, square-cropped, resized, WebP.

    Re-encoding also drops EXIF metadata (e.g. GPS) and anything that isn't pixel data.
    """
    return _webp(ImageOps.fit(_load(upload), (AVATAR_SIZE, AVATAR_SIZE)))


def process_photo(upload: UploadedFile) -> ContentFile:
    """Like `process_avatar` but for pictures that are looked at as a whole: the aspect ratio is
    kept and the longest side is capped at `PHOTO_MAX_SIDE` (never enlarged)."""
    image = _load(upload)
    image.thumbnail((PHOTO_MAX_SIDE, PHOTO_MAX_SIDE))
    return _webp(image)
