//! Poll deadlines: every poll ends 72 hours after it was created, and those who have not
//! answered are reminded at 24, 48 and 69 hours (after downtime only the latest reminder).

use chrono::{DateTime, Utc};
use domain::polls::{CloseReason, REMINDERS, due_reminder_stage, hours_left_at};
use futures_util::future::BoxFuture;

use super::close::close;
use super::lock;
use crate::Context;
use crate::db::Uow;
use crate::push::Notification;
use crate::scheduler::{Job, JobResult};

const BATCH: i64 = 50;

#[derive(Debug, Clone, Copy)]
pub struct PollDeadlines;

impl PollDeadlines {
    async fn process(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        loop {
            let mut uow = Uow::begin(&ctx.db).await?;
            // Claimed with SKIP LOCKED: another process (or a vote closing it) is never waited on.
            let due = sqlx::query_scalar!(
                "select id from polls
                 where status = 'open' and (deadline_at <= $1 or next_reminder_at <= $1)
                 order by least(deadline_at, coalesce(next_reminder_at, deadline_at))
                 limit $2 for update skip locked",
                now,
                BATCH,
            )
            .fetch_all(uow.conn())
            .await?;
            for id in &due {
                let poll = lock(&mut uow, *id).await?;
                let deadline =
                    sqlx::query_scalar!("select deadline_at from polls where id = $1", id)
                        .fetch_one(uow.conn())
                        .await?;
                if now >= deadline {
                    close(&mut uow, ctx, &poll, CloseReason::Expired).await?;
                } else {
                    remind(&mut uow, ctx, &poll, now).await?;
                }
            }
            uow.commit().await?;
            if due.len() < usize::try_from(BATCH)? {
                break;
            }
        }
        Ok(sqlx::query_scalar!(
            "select min(least(deadline_at, coalesce(next_reminder_at, deadline_at))) from polls where status = 'open'"
        )
        .fetch_one(&ctx.db)
        .await?)
    }
}

async fn remind(
    uow: &mut Uow,
    ctx: &Context,
    poll: &super::PollRow,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let stage = due_reminder_stage(now - poll.created_at);
    let next = REMINDERS.get(stage).map(|offset| poll.created_at + *offset);
    sqlx::query!(
        "update polls set reminders_sent = $2, next_reminder_at = $3 where id = $1",
        poll.id,
        i16::try_from(stage)?,
        next,
    )
    .execute(uow.conn())
    .await?;
    if stage > usize::try_from(poll.reminders_sent)? {
        let waiting = sqlx::query_scalar!(
            "select user_id from poll_participants where poll_id = $1 and voted_at is null",
            poll.id
        )
        .fetch_all(uow.conn())
        .await?;
        let notification = Notification {
            title: "Głosowanie czeka na Twój głos".into(),
            body: format!(
                "„{}” kończy się za {} godz.",
                poll.title,
                hours_left_at(stage)
            ),
            url: format!("/voting/{}", poll.id),
        };
        ctx.push.notify(uow, &waiting, &notification).await?;
    }
    Ok(())
}

impl Job for PollDeadlines {
    fn name(&self) -> &'static str {
        "poll-deadlines"
    }

    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult> {
        Box::pin(Self::process(ctx, now))
    }
}
