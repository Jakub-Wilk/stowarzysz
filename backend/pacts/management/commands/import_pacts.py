import csv
import json
from pathlib import Path
from typing import Any

from django.core.management.base import BaseCommand, CommandError
from django.db import transaction

from pacts import importing


class Command(BaseCommand):
    help = (
        "Import the old Google Sheet (CSV export) as pacts. Rows without a mapping become the "
        "host's own resolutions; rows in --map get opponents and become bets. Safe to re-run: "
        "rows already imported (same host and title) are skipped."
    )

    def add_arguments(self, parser: Any) -> None:
        parser.add_argument("csv_path", type=Path)
        parser.add_argument("--host", required=True, help="username that owns unmapped rows")
        parser.add_argument(
            "--map",
            type=Path,
            help=(
                'JSON: {"<title>": {"host": "u", '
                '"opponents": [{"username": "u", "stake_pln": 10}]}}'
            ),
        )
        parser.add_argument(
            "--images",
            type=Path,
            help="folder of pictures named <title>.<ext> (or <title>__2.<ext>) for new pacts",
        )
        parser.add_argument("--dry-run", action="store_true", help="import, then roll back")

    def handle(self, *args: Any, **options: Any) -> None:
        try:
            default_host = importing.get_user(options["host"])
            mapping = {}
            if options["map"]:
                mapping = importing.parse_mapping(json.loads(options["map"].read_text("utf-8")))
            with options["csv_path"].open(encoding="utf-8-sig", newline="") as handle:
                rows = list(csv.DictReader(handle))
            imported = skipped = pictures = 0
            with transaction.atomic():
                for row in rows:
                    host, opponents = mapping.get(
                        row.get(importing.TITLE, "").strip(), (default_host, [])
                    )
                    pact = importing.import_row(row, host=host, opponents=opponents)
                    if pact:
                        imported += 1
                        if options["images"]:
                            pictures += importing.attach_images(pact, host, options["images"])
                    else:
                        skipped += 1
                if options["dry_run"]:
                    transaction.set_rollback(True)
        except (importing.SheetImportError, OSError, KeyError, ValueError) as exc:
            raise CommandError(str(exc)) from exc
        suffix = " (dry run, rolled back)" if options["dry_run"] else ""
        self.stdout.write(
            f"Imported {imported} pact(s), skipped {skipped}, "
            f"attached {pictures} picture(s){suffix}."
        )
