from typing import Any

from django.core.management.base import BaseCommand

from voting.services import process_deadlines


class Command(BaseCommand):
    help = "Close polls that ran out of time and send deadline reminders. Run every few minutes."

    def handle(self, *args: Any, **options: Any) -> None:
        closed, reminded = process_deadlines()
        self.stdout.write(f"Closed {closed} poll(s), sent reminders for {reminded}.")
