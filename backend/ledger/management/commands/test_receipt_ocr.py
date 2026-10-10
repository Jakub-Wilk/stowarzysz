import mimetypes
from pathlib import Path
from typing import Any

from django.conf import settings
from django.core.management.base import BaseCommand, CommandError, CommandParser

from ledger.receipt_ocr import ocr_receipt

DEFAULT_IMAGE = Path(settings.BASE_DIR).parent / "image.jpg"


class Command(BaseCommand):
    help = "Send a receipt photo (default: repo-root image.jpg) to Gemini and print the OCR."

    def add_arguments(self, parser: CommandParser) -> None:
        parser.add_argument("path", nargs="?", type=Path, default=DEFAULT_IMAGE)

    def handle(self, *args: Any, **options: Any) -> None:
        path: Path = options["path"]
        if not settings.GEMINI_API_KEY:
            raise CommandError("GEMINI_API_KEY is not set (put it in backend/.env)")
        if not path.is_file():
            raise CommandError(f"Image not found: {path}")
        mime_type = mimetypes.guess_type(path)[0] or "image/jpeg"
        self.stderr.write(f"Sending {path} to {settings.GEMINI_MODEL}...")
        receipt = ocr_receipt(path.read_bytes(), mime_type)
        self.stdout.write(receipt.model_dump_json(indent=2, exclude_none=True))
