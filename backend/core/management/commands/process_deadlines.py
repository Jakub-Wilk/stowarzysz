from typing import Any

from django.core.management import call_command
from django.core.management.base import BaseCommand, CommandError

COMMANDS = ("process_poll_deadlines", "process_pact_deadlines", "process_birthdays")


class Command(BaseCommand):
    help = "Run every deadline job (polls, pacts, birthdays). Cron runs this every ~5 minutes."

    def handle(self, *args: Any, **options: Any) -> None:
        failed = []
        for name in COMMANDS:  # one failing job must not stop the others
            try:
                call_command(name, stdout=self.stdout)
            except Exception as exc:
                failed.append(name)
                self.stderr.write(f"{name} failed: {exc!r}")
        if failed:
            raise CommandError(f"Failed: {', '.join(failed)}")
