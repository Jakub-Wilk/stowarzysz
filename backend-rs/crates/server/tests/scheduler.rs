//! Scheduler: periodic jobs run once per period across processes; housekeeping cleans up.
#![allow(clippy::unwrap_used)]

mod common;

use app::scheduler::{Claim, Housekeeping, Job as _, PeriodicRun, Scheduler};
use chrono::TimeDelta;
use common::{TestApp, insert_user};
use sqlx::PgPool;

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_periodic_job_is_claimed_by_one_runner_at_a_time(pool: PgPool) {
    let app = TestApp::new(pool);
    let now = app.ctx.clock.now();

    let Claim::Run(run) = PeriodicRun::claim(app.db(), "demo", now).await.unwrap() else {
        panic!("a new job is due immediately");
    };
    // While it runs, another process sees it as not due.
    assert!(matches!(
        PeriodicRun::claim(app.db(), "demo", now).await.unwrap(),
        Claim::NotDue(_)
    ));
    let next = now + TimeDelta::hours(1);
    run.finish(now, next).await.unwrap();

    match PeriodicRun::claim(app.db(), "demo", now).await.unwrap() {
        Claim::NotDue(at) => assert_eq!(at, next),
        Claim::Run(_) => panic!("ran twice in one period"),
    }
    assert!(matches!(
        PeriodicRun::claim(app.db(), "demo", next).await.unwrap(),
        Claim::Run(_)
    ));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn housekeeping_removes_expired_sessions_once_a_day(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = insert_user(app.db(), "ala").await;
    let now = app.ctx.clock.now();
    for (hash, expires_in) in [(1_u8, -1_i64), (2, 1)] {
        sqlx::query(
            "insert into auth_sessions (id, user_id, refresh_hash, expires_at)
             values (gen_random_uuid(), $1, $2, $3)",
        )
        .bind(ala)
        .bind(vec![hash])
        .bind(now + TimeDelta::days(expires_in))
        .execute(app.db())
        .await
        .unwrap();
    }

    let next = Housekeeping.run(&app.ctx, now).await.unwrap();
    assert_eq!(next, Some(now + TimeDelta::hours(24)));
    let left: i64 = sqlx::query_scalar("select count(*) from auth_sessions")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(left, 1, "only the live session remains");

    // Running again within the day does nothing, even if more has expired meanwhile.
    let later = now + TimeDelta::days(2);
    assert_eq!(
        Housekeeping
            .run(&app.ctx, now + TimeDelta::hours(1))
            .await
            .unwrap(),
        next
    );
    let left: i64 = sqlx::query_scalar("select count(*) from auth_sessions")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(left, 1);
    Housekeeping.run(&app.ctx, later).await.unwrap();
    let left: i64 = sqlx::query_scalar("select count(*) from auth_sessions")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn run_once_runs_every_job(pool: PgPool) {
    let app = TestApp::new(pool);
    let scheduler = Scheduler::standard();
    let results = scheduler.run_once(&app.ctx).await;
    assert_eq!(results.len(), scheduler.job_names().len());
    for (job, result) in results {
        assert!(result.is_ok(), "{job}: {result:?}");
    }
}
