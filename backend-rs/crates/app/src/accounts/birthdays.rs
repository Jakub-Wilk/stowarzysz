//! The birthday reminder: from 20:00 Warsaw time, everyone else is told, once per birthday.

use chrono::{DateTime, Days, NaiveDate, Utc};
use domain::accounts::birthday::{self, REMINDER_TIME};
use futures_util::future::BoxFuture;

use super::profile::month_days;
use crate::Context;
use crate::clock::{ZONE, local_today};
use crate::db::Uow;
use crate::push::Notification;
use crate::scheduler::{Job, JobResult};

#[derive(Debug, Clone, Copy)]
pub struct BirthdayReminders;

/// 20:00 in Warsaw on `day`, as a UTC instant.
fn reminder_at(day: NaiveDate) -> DateTime<Utc> {
    let local = day.and_time(REMINDER_TIME);
    local
        .and_local_timezone(ZONE)
        .earliest()
        .map_or_else(|| local.and_utc(), |at| at.with_timezone(&Utc))
}

impl BirthdayReminders {
    async fn remind(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        let today = local_today(now);
        let tomorrow = reminder_at(today + Days::new(1));
        if now < reminder_at(today) {
            return Ok(Some(reminder_at(today)));
        }
        let mut uow = Uow::begin(&ctx.db).await?;
        // Claiming the day is the update itself, so overlapping runs cannot both announce it.
        let people = sqlx::query!(
            "update users set birthday_reminded_on = $1
             where is_active and password_hash is not null
               and to_char(birthday, 'MM-DD') = any($2)
               and birthday_reminded_on is distinct from $1
             returning id, username",
            today,
            &month_days(&birthday::birthdays_on(today)),
        )
        .fetch_all(uow.conn())
        .await?;
        for person in &people {
            let others = sqlx::query_scalar!(
                "select id from users where is_active and password_hash is not null and id <> $1",
                person.id,
            )
            .fetch_all(uow.conn())
            .await?;
            let notification = Notification {
                title: "Urodziny 🎂".into(),
                body: format!(
                    "Dziś urodziny obchodzi {}. Pamiętaj o życzeniach!",
                    person.username
                ),
                url: "/".into(),
            };
            ctx.push.notify(&mut uow, &others, &notification).await?;
        }
        uow.commit().await?;
        if !people.is_empty() {
            tracing::info!(count = people.len(), "birthday reminders queued");
        }
        Ok(Some(tomorrow))
    }
}

impl Job for BirthdayReminders {
    fn name(&self) -> &'static str {
        "birthdays"
    }

    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult> {
        Box::pin(Self::remind(ctx, now))
    }
}
