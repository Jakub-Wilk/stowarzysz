//! Today's birthdays and the 20:00 reminder.
#![allow(clippy::unwrap_used)]

mod common;

use app::accounts::birthdays::BirthdayReminders;
use app::scheduler::Job as _;
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, TimeZone as _, Utc};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// `hh:mm` Warsaw time on `y-m-d`, as UTC.
fn warsaw(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> DateTime<Utc> {
    chrono_tz::Europe::Warsaw
        .with_ymd_and_hms(y, m, d, hh, mm, 0)
        .unwrap()
        .with_timezone(&Utc)
}

async fn set_birthday(app: &TestApp, user: i64, day: &str) {
    sqlx::query("update users set birthday = $2::date where id = $1")
        .bind(user)
        .bind(day)
        .execute(app.db())
        .await
        .unwrap();
}

async fn queued_titles(app: &TestApp) -> Vec<(String, String)> {
    sqlx::query_as(
        "select s.endpoint, o.payload->>'body' from push_outbox o join push_subscriptions s on s.id = o.subscription_id order by s.endpoint",
    )
    .fetch_all(app.db())
    .await
    .unwrap()
}

async fn subscribe(app: &TestApp, user: i64) {
    sqlx::query("insert into push_subscriptions (user_id, endpoint, p256dh, auth) values ($1, 'https://push.test/' || $1, 'k', 'a')")
        .bind(user)
        .execute(app.db())
        .await
        .unwrap();
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_endpoint_lists_todays_birthdays(pool: PgPool) {
    let app = TestApp::new(pool);
    app.get("/api/birthdays/today")
        .await
        .expect(StatusCode::UNAUTHORIZED);
    let ala = app.member("ala").await;
    let ola = app.member("ola").await;
    let ewa = app.member("ewa").await;
    app.clock.set(warsaw(2027, 5, 3, 0, 30)); // just after midnight in Warsaw (22:30 UTC the day before)
    let ala = common::Member {
        token: app.session(ala.id).await.access,
        ..ala
    };
    set_birthday(&app, ala.id, "1990-05-03").await;
    set_birthday(&app, ola.id, "1985-05-03").await;
    set_birthday(&app, ewa.id, "1990-05-02").await;
    let today = app.get_as(&ala, "/api/birthdays/today").await;
    let names: Vec<_> = today
        .expect(StatusCode::OK)
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["username"].clone())
        .collect();
    assert_eq!(names, [json!("ala"), json!("ola")]);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_leap_day_birthday_is_kept_on_28_february(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    set_birthday(&app, ala.id, "2000-02-29").await;
    app.clock.set(warsaw(2027, 2, 28, 12, 0));
    let ala = common::Member {
        token: app.session(ala.id).await.access,
        ..ala
    };
    assert_eq!(
        app.get_as(&ala, "/api/birthdays/today")
            .await
            .json()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    app.clock.set(warsaw(2028, 2, 28, 12, 0));
    let ala = common::Member {
        token: app.session(ala.id).await.access,
        ..ala
    };
    assert_eq!(
        app.get_as(&ala, "/api/birthdays/today")
            .await
            .json()
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_reminder_waits_until_8_pm_and_goes_to_everyone_else_once(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let ala = app.member("ala").await;
    let ola = app.member("ola").await;
    let ewa = app.member("ewa").await;
    for user in [ala.id, ola.id, ewa.id] {
        subscribe(&app, user).await;
    }
    set_birthday(&app, ala.id, "1990-05-03").await;

    // Before 20:00: nothing yet, and the job knows when to come back.
    let next = BirthdayReminders
        .run(&app.ctx, warsaw(2027, 5, 3, 19, 59))
        .await
        .unwrap();
    assert_eq!(next, Some(warsaw(2027, 5, 3, 20, 0)));
    assert_eq!(queued_titles(&app).await, Vec::<(String, String)>::new());

    let next = BirthdayReminders
        .run(&app.ctx, warsaw(2027, 5, 3, 21, 15))
        .await
        .unwrap(); // catches up
    assert_eq!(next, Some(warsaw(2027, 5, 4, 20, 0)));
    let body = "Dziś urodziny obchodzi ala. Pamiętaj o życzeniach!".to_owned();
    assert_eq!(
        queued_titles(&app).await,
        [
            (format!("https://push.test/{}", ola.id), body.clone()),
            (format!("https://push.test/{}", ewa.id), body)
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );

    // Once per day.
    BirthdayReminders
        .run(&app.ctx, warsaw(2027, 5, 3, 23, 0))
        .await
        .unwrap();
    assert_eq!(queued_titles(&app).await.len(), 2);
    let reminded: NaiveDate =
        sqlx::query_scalar("select birthday_reminded_on from users where id = $1")
            .bind(ala.id)
            .fetch_one(app.db())
            .await
            .unwrap();
    assert_eq!(reminded, NaiveDate::from_ymd_opt(2027, 5, 3).unwrap());

    // And again next year.
    sqlx::query("delete from push_outbox")
        .execute(app.db())
        .await
        .unwrap();
    BirthdayReminders
        .run(&app.ctx, warsaw(2028, 5, 3, 20, 0))
        .await
        .unwrap();
    assert_eq!(queued_titles(&app).await.len(), 2);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn administrators_set_birthdays(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let response = app
        .patch_as(
            &admin,
            &format!("/api/users/{}", ala.id),
            json!({"birthday": "1990-05-03"}),
        )
        .await;
    assert_eq!(
        response.expect(StatusCode::OK).json()["birthday"],
        "1990-05-03"
    );
    assert_eq!(
        app.get_as(&ala, "/api/me").await.json()["birthday"],
        "1990-05-03"
    );
}
