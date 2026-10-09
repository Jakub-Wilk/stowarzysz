from typing import Any

from django.core.management.base import BaseCommand

from pacts.services import process_deadlines


class Command(BaseCommand):
    help = (
        "Move overdue pacts along, send reminders and expire stale invites. Run every few minutes."
    )

    def handle(self, *args: Any, **options: Any) -> None:
        counts = process_deadlines()
        self.stdout.write(
            f"Pacts: {counts['overdue']} overdue, {counts['nudged']} nudged, "
            f"{counts['expired']} invite(s) expired, {counts['lapsed']} lapsed."
        )
