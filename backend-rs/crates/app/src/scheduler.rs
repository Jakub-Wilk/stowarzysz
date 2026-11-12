//! In-process background jobs with exact due times (this replaces the old cron job).
//!
//! A [`Job`] does all the work that is due at `now` and says when it next has work, so the loop
//! sleeps exactly until then (at most [`MAX_SLEEP`], and it is woken early through
//! [`crate::listener::Wake::Jobs`]). Jobs claim what they work on with `FOR UPDATE SKIP LOCKED`,
//! so any number of processes can run the scheduler without doing anything twice.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use futures_util::future::BoxFuture;
use sqlx::{PgPool, Postgres, Transaction};
use tokio_util::sync::CancellationToken;

use crate::context::Context;

/// The scheduler looks at every job at least this often.
pub const MAX_SLEEP: Duration = Duration::from_mins(1);
/// After a failure a job is retried this much later.
const RETRY_AFTER: TimeDelta = TimeDelta::seconds(30);

pub type JobResult = anyhow::Result<Option<DateTime<Utc>>>;

pub trait Job: Send + Sync + fmt::Debug {
    fn name(&self) -> &'static str;

    /// Do everything that is due at `now`. Returns when this job next has work (`None`: nothing
    /// known; it is looked at again within [`MAX_SLEEP`]).
    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult>;
}

#[derive(Debug, Clone)]
pub struct Scheduler {
    jobs: Vec<Arc<dyn Job>>,
}

impl Scheduler {
    pub fn new(jobs: Vec<Arc<dyn Job>>) -> Self {
        Self { jobs }
    }

    /// Every job the application runs.
    pub fn standard() -> Self {
        Self::new(vec![Arc::new(Housekeeping)])
    }

    pub fn job_names(&self) -> Vec<&'static str> {
        self.jobs.iter().map(|job| job.name()).collect()
    }

    /// Run every job once (the `run-jobs` command). Failures are reported, not fatal.
    pub async fn run_once(&self, ctx: &Context) -> Vec<(&'static str, JobResult)> {
        let now = ctx.clock.now();
        let mut results = Vec::with_capacity(self.jobs.len());
        for job in &self.jobs {
            results.push((job.name(), job.run(ctx, now).await));
        }
        results
    }

    /// Run until cancelled.
    pub async fn run(self, ctx: Arc<Context>, cancel: CancellationToken) {
        let mut next_due: HashMap<&'static str, DateTime<Utc>> = HashMap::new();
        loop {
            let now = ctx.clock.now();
            let idle = now + TimeDelta::from_std(MAX_SLEEP).unwrap_or(TimeDelta::MAX);
            let mut wake_at = idle;
            for job in &self.jobs {
                let due = next_due.get(job.name()).copied().unwrap_or(now);
                if due <= now {
                    let next = match job.run(&ctx, now).await {
                        Ok(next) => next.unwrap_or(idle),
                        Err(error) => {
                            tracing::error!(
                                job = job.name(),
                                error = format!("{error:#}"),
                                "job failed"
                            );
                            now + RETRY_AFTER
                        }
                    };
                    next_due.insert(job.name(), next);
                }
                wake_at = wake_at.min(next_due.get(job.name()).copied().unwrap_or(now));
            }
            let pause = (wake_at - ctx.clock.now())
                .to_std()
                .unwrap_or(Duration::ZERO)
                .min(MAX_SLEEP);
            tokio::select! {
                () = cancel.cancelled() => return,
                () = ctx.signals.jobs.notified() => next_due.clear(), // everything is due again
                () = tokio::time::sleep(pause) => {}
            }
        }
    }
}

/// A claim on a periodic job's `scheduled_jobs` row: held until [`PeriodicRun::finish`], so the
/// run happens in one process only.
#[derive(Debug)]
pub struct PeriodicRun {
    tx: Transaction<'static, Postgres>,
    name: &'static str,
}

/// Whether a periodic job should run now.
#[derive(Debug)]
pub enum Claim {
    Run(PeriodicRun),
    /// Not due (or another process is running it); look again at this time.
    NotDue(DateTime<Utc>),
}

impl PeriodicRun {
    pub async fn claim(
        pool: &PgPool,
        name: &'static str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Claim> {
        sqlx::query!(
            "insert into scheduled_jobs (name, next_run_at) values ($1, $2) on conflict (name) do nothing",
            name,
            now,
        )
        .execute(pool)
        .await?;
        let mut tx = pool.begin().await?;
        let due = sqlx::query_scalar!(
            "select next_run_at from scheduled_jobs where name = $1 for update skip locked",
            name,
        )
        .fetch_optional(&mut *tx)
        .await?;
        Ok(match due {
            None => Claim::NotDue(now + RETRY_AFTER), // running elsewhere right now
            Some(next) if next > now => Claim::NotDue(next),
            Some(_) => Claim::Run(Self { tx, name }),
        })
    }

    /// Record the run and schedule the next one.
    pub async fn finish(mut self, now: DateTime<Utc>, next: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query!(
            "update scheduled_jobs set last_run_at = $2, next_run_at = $3 where name = $1",
            self.name,
            now,
            next,
        )
        .execute(&mut *self.tx)
        .await?;
        self.tx.commit().await?;
        Ok(())
    }
}

/// Daily cleanup of rows nothing needs any more.
#[derive(Debug, Clone, Copy)]
pub struct Housekeeping;

impl Housekeeping {
    const EVERY: TimeDelta = TimeDelta::hours(24);
    /// Revoked sessions and used activation links are kept this long for troubleshooting.
    const KEEP: TimeDelta = TimeDelta::days(30);

    async fn sweep(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        let run = match PeriodicRun::claim(&ctx.db, "housekeeping", now).await? {
            Claim::Run(run) => run,
            Claim::NotDue(next) => return Ok(Some(next)),
        };
        let keep_until = now - Self::KEEP;
        let sessions = sqlx::query!(
            "delete from auth_sessions where expires_at < $1 or revoked_at < $2",
            now,
            keep_until,
        )
        .execute(&ctx.db)
        .await?
        .rows_affected();
        let tokens = sqlx::query!(
            "delete from activation_tokens
             where (used_at is not null or expires_at < $1) and created_at < $2",
            now,
            keep_until,
        )
        .execute(&ctx.db)
        .await?
        .rows_affected();
        let legacy = sqlx::query!(
            "delete from legacy_refresh_tokens where expires_at < $1",
            now
        )
        .execute(&ctx.db)
        .await?
        .rows_affected();
        let pushes = sqlx::query!("delete from push_outbox where expires_at <= now()")
            .execute(&ctx.db)
            .await?
            .rows_affected();
        tracing::info!(sessions, tokens, legacy, pushes, "housekeeping done");
        let next = now + Self::EVERY;
        run.finish(now, next).await?;
        Ok(Some(next))
    }
}

impl Job for Housekeeping {
    fn name(&self) -> &'static str {
        "housekeeping"
    }

    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult> {
        Box::pin(Self::sweep(ctx, now))
    }
}
