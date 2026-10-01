from io import BytesIO

from django.core.files.base import ContentFile
from django.core.files.uploadedfile import UploadedFile
from PIL import Image, ImageOps, UnidentifiedImageError
from rest_framework import serializers

MAX_UPLOAD_BYTES = 5 * 1024 * 1024
AVATAR_SIZE = 512  # stored square, so every client can render it as-is


def process_avatar(upload: UploadedFile) -> ContentFile:
    """Validate an uploaded image and normalise it: upright, square-cropped, resized, WebP.

    Re-encoding also drops EXIF metadata (e.g. GPS) and anything that isn't pixel data.
    """
    if upload.size is not None and upload.size > MAX_UPLOAD_BYTES:
        raise serializers.ValidationError(
            f"Image is too large (max {MAX_UPLOAD_BYTES // (1024 * 1024)} MB)."
        )
    try:
        with Image.open(upload) as source:  # ty: ignore[invalid-argument-type]
            image = ImageOps.exif_transpose(source)  # first frame only for animated formats
            mode = "RGBA" if "A" in image.getbands() else "RGB"
            image = ImageOps.fit(image.convert(mode), (AVATAR_SIZE, AVATAR_SIZE))
    except (UnidentifiedImageError, Image.DecompressionBombError, OSError, ValueError) as exc:
        raise serializers.ValidationError("Upload a valid image.") from exc

    buffer = BytesIO()
    image.save(buffer, format="WEBP", quality=85)
    return ContentFile(buffer.getvalue())
