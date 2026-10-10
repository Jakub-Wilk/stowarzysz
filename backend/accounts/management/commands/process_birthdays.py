from typing import Any

from django.core.management.base import BaseCommand

from accounts.birthdays import send_birthday_reminders


class Command(BaseCommand):
    help = "Push a birthday reminder (from 20:00 local time) to everyone but the birthday person."

    def handle(self, *args: Any, **options: Any) -> None:
        self.stdout.write(f"Birthdays: {send_birthday_reminders()} reminder(s) sent.")
