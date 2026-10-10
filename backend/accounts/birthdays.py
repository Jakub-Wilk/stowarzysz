from datetime import date, datetime, time

from django.db import models, transaction
from django.utils import timezone

from accounts.models import User
from push.sender import send_push

REMINDER_TIME = time(20, 0)  # local (Europe/Warsaw) time of day the reminder goes out


def is_birthday(birthday: date | None, today: date) -> bool:
    """Whether `today` is `birthday`'s day of the year (29 February is kept on 28 February in
    years without one)."""
    if birthday is None:
        return False
    if birthday.month == 2 and birthday.day == 29:
        if today.month == 2 and today.day == 29:
            return True
        return today.month == 2 and today.day == 28 and not _is_leap(today.year)
    return (birthday.month, birthday.day) == (today.month, today.day)


def _is_leap(year: int) -> bool:
    return year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)


def birthday_users(today: date) -> models.QuerySet:
    """Members whose birthday is `today`."""
    month_day = models.Q(birthday__month=today.month, birthday__day=today.day)
    if today.month == 2 and today.day == 28 and not _is_leap(today.year):
        month_day |= models.Q(birthday__month=2, birthday__day=29)
    return User.objects.members().filter(month_day).order_by("username")


def send_birthday_reminders(now: datetime | None = None) -> int:
    """Tell everyone else about today's birthdays, once each, from 20:00 local time on.

    Returns how many birthdays were announced. Safe to run every few minutes.
    """
    local = timezone.localtime(now or timezone.now())
    if local.time() < REMINDER_TIME:
        return 0
    today = local.date()
    sent = 0
    for person in birthday_users(today).exclude(birthday_reminded_on=today):
        with transaction.atomic():
            # claim the day first, so two overlapping runs cannot both send it
            claimed = User.objects.filter(pk=person.pk).exclude(birthday_reminded_on=today)
            if not claimed.update(birthday_reminded_on=today):
                continue
        others = User.objects.members().exclude(pk=person.pk).values_list("pk", flat=True)
        send_push(
            others,
            title="Urodziny 🎂",
            body=f"Dziś urodziny obchodzi {person.username}. Pamiętaj o życzeniach!",
            url="/",
        )
        sent += 1
    return sent
