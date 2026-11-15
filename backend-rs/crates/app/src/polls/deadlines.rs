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
use crate::scheduler::{Job, JobResult, next_after_failures};

const BATCH: i64 = 50;

#[derive(Debug, Clone, Copy)]
pub struct PollDeadlines;

impl PollDeadlines {
    async fn process(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        // Polls whose step failed: rolled back and left for the next run, so one broken poll
        // never holds up the others.
        let mut failed: Vec<i64> = Vec::new();
        loop {
            let mut uow = Uow::begin(&ctx.db).await?;
            // Claimed with SKIP LOCKED: another process (or a vote closing it) is never waited on.
            let due = sqlx::query!(
                "select id, deadline_at from polls
                 where status = 'open' and (deadline_at <= $1 or next_reminder_at <= $1)
                   and id <> all($3)
                 order by least(deadline_at, coalesce(next_reminder_at, deadline_at))
                 limit $2 for update skip locked",
                now,
                BATCH,
                &failed,
            )
            .fetch_all(uow.conn())
            .await?;
            for row in &due {
                uow.savepoint().await?;
                match Self::one(&mut uow, ctx, row.id, now >= row.deadline_at, now).await {
                    Ok(()) => uow.release_savepoint().await?,
                    Err(error) => {
                        tracing::error!(
                            poll_id = row.id,
                            error = format!("{error:#}"),
                            "poll deadline failed; retrying later"
                        );
                        uow.rollback_to_savepoint().await?;
                        failed.push(row.id);
                    }
                }
            }
            uow.commit().await?;
            if due.len() < usize::try_from(BATCH)? {
                break;
            }
        }
        let next = sqlx::query_scalar!(
            "select min(least(deadline_at, coalesce(next_reminder_at, deadline_at))) from polls where status = 'open'"
        )
        .fetch_one(&ctx.db)
        .await?;
        Ok(next_after_failures(next, !failed.is_empty(), now))
    }

    /// Close poll `id` (`expired`) or remind its non-voters.
    async fn one(
        uow: &mut Uow,
        ctx: &Context,
        id: i64,
        expired: bool,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let poll = lock(uow, id).await?;
        if expired {
            close(uow, ctx, &poll, CloseReason::Expired).await?;
        } else {
            remind(uow, ctx, &poll, now).await?;
        }
        Ok(())
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
