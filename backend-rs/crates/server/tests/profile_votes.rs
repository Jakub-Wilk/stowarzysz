//! Votes that change a member's profile, and poll deadlines (reminders, expiry).
#![allow(clippy::unwrap_used)]

mod common;

use app::media::jobs::MediaSweeper;
use app::polls::deadlines::PollDeadlines;
use app::scheduler::Job as _;
use axum::http::StatusCode;
use chrono::TimeDelta;
use common::{Member, TestApp, png, subscribe, take_pushes};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn trio(app: &TestApp) -> (Member, Member, Member) {
    (
        app.member("ala").await,
        app.member("ola").await,
        app.member("ewa").await,
    )
}

async fn nickname_vote(
    app: &TestApp,
    creator: &Member,
    target: &Member,
    name: &str,
) -> common::TestResponse {
    app.post_as(
        creator,
        "/api/polls",
        json!({"kind": "nickname", "target_user_id": target.id, "new_username": name}),
    )
    .await
}

async fn vote_all(app: &TestApp, id: &Value, votes: &[(&Member, i64)]) -> Value {
    let mut last = Value::Null;
    for (who, value) in votes {
        last = app
            .put_as(
                who,
                &format!("/api/polls/{id}/ballot"),
                json!({"value": value}),
            )
            .await
            .expect(StatusCode::OK)
            .json();
    }
    last
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_nickname_vote_includes_everyone_and_titles_itself(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, _) = trio(&app).await;
    let poll = nickname_vote(&app, &ala, &ola, "Ola Wielka")
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(poll["title"], "Zmiana nicku: ola → Ola Wielka");
    assert_eq!(
        poll["participant_count"], 3,
        "every member, the target and the creator included"
    );
    assert_eq!(
        poll["config"],
        json!({"target_user_id": ola.id, "target_username": "ola", "new_username": "Ola Wielka"})
    );
    assert_eq!(
        poll["actions"],
        json!({"vote": true, "veto": false, "abstain": false, "close": false, "tie_break": false})
    );
    let id = &poll["id"];
    for path in ["veto", "abstain", "close"] {
        let response = app
            .post_as(&ala, &format!("/api/polls/{id}/{path}"), json!({}))
            .await;
        response.expect(StatusCode::BAD_REQUEST);
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn nickname_votes_are_validated(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let own = nickname_vote(&app, &ala, &ala, "Alicja").await;
    assert_eq!(own.code(), "own_profile");
    for (name, code) in [
        ("ola", "same_username"),
        ("ewa", "username_taken"),
        ("a  b", "username_invalid"),
        ("", "username_required"),
    ] {
        let response = nickname_vote(&app, &ala, &ola, name).await;
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(response.code(), code, "{name}");
        assert!(
            response.json()["errors"]["new_username"].is_array(),
            "{name}"
        );
    }
    sqlx::query("update users set is_active = false where id = $1")
        .bind(ewa.id)
        .execute(app.db())
        .await
        .unwrap();
    let inactive = nickname_vote(&app, &ala, &ewa, "Ewka").await;
    assert_eq!(
        inactive.json()["errors"]["target_user_id"],
        json!(["Nieznany lub nieaktywny poseł."])
    );
    let unknown = app
        .post_as(
            &ala,
            "/api/polls",
            json!({"kind": "nickname", "target_user_id": 999_999, "new_username": "X"}),
        )
        .await;
    assert_eq!(unknown.code(), "unknown_target");
    nickname_vote(&app, &ala, &ola, "Ola z Krakowa")
        .await
        .expect(StatusCode::CREATED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_approved_nickname_vote_renames_and_a_rejected_one_does_not(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let poll = nickname_vote(&app, &ala, &ola, "Olunia").await.json();
    let closed = vote_all(&app, &poll["id"], &[(&ala, 1), (&ola, -1), (&ewa, 1)]).await;
    assert_eq!(closed["result"]["approved"], true);
    assert_eq!(closed["result"]["applied"], true);
    assert_eq!(
        app.get_as(&ola, "/api/me").await.json()["username"],
        "Olunia"
    );

    let poll = nickname_vote(&app, &ala, &ewa, "Ewcia").await.json();
    let closed = vote_all(&app, &poll["id"], &[(&ala, 1), (&ola, -1), (&ewa, 0)]).await;
    assert_eq!(
        (
            closed["result"]["approved"].clone(),
            closed["result"]["applied"].clone()
        ),
        (json!(false), json!(false))
    );
    assert_eq!(closed["result"]["tone"], "negative");
    assert_eq!(app.get_as(&ewa, "/api/me").await.json()["username"], "ewa");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_name_taken_while_voting_is_reported_not_applied(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola, ewa) = trio(&app).await;
    subscribe(app.db(), &[ala.id]).await;
    let poll = nickname_vote(&app, &ala, &ola, "Królowa").await.json();
    sqlx::query("update users set username = 'Królowa' where id = $1")
        .bind(ewa.id)
        .execute(app.db())
        .await
        .unwrap();
    let closed = vote_all(&app, &poll["id"], &[(&ala, 5), (&ola, 5), (&ewa, 5)]).await;
    assert_eq!(closed["result"]["approved"], true);
    assert_eq!(closed["result"]["applied"], false);
    assert_eq!(
        closed["result"]["apply_error"],
        "Użytkownik o takiej nazwie już istnieje."
    );
    let pushes = take_pushes(app.db()).await;
    assert_eq!(
        pushes.last().unwrap().2,
        "Zmiana nicku: ola → Królowa: przyjęto, ale nie udało się zastosować zmiany"
    );
}

async fn avatar_upload(app: &TestApp, who: &Member) -> Value {
    app.upload(who, "avatar", "image/png", png(64, 64))
        .await
        .expect(StatusCode::CREATED)
        .json()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn avatar_votes_need_a_picture_or_a_removal(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, _) = trio(&app).await;
    let upload = avatar_upload(&app, &ala).await;
    for (body, code) in [
        (
            json!({"kind": "avatar", "target_user_id": ola.id}),
            "image_required",
        ),
        (
            json!({"kind": "avatar", "target_user_id": ola.id, "remove": true, "upload_id": upload["id"]}),
            "image_with_removal",
        ),
        (
            json!({"kind": "avatar", "target_user_id": ala.id, "upload_id": upload["id"]}),
            "own_profile",
        ),
    ] {
        let response = app.post_as(&ala, "/api/polls", body.clone()).await;
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(response.code(), code, "{body}");
    }
    let poll = app
        .post_as(
            &ala,
            "/api/polls",
            json!({"kind": "avatar", "target_user_id": ola.id, "upload_id": upload["id"]}),
        )
        .await;
    let poll = poll.expect(StatusCode::CREATED).json();
    assert_eq!(poll["title"], "Zmiana zdjęcia: ola");
    assert!(
        poll["proposed_image"]["sm"]
            .as_str()
            .unwrap()
            .starts_with("/api/media/avatars/")
    );
    assert_eq!(poll["previous_image"], Value::Null);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_approved_avatar_vote_swaps_the_picture_and_keeps_both_for_the_record(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let admin = app.admin("boss").await;
    // ola has a picture already.
    let first = avatar_upload(&app, &admin).await;
    app.put_as(
        &admin,
        &format!("/api/users/{}/avatar", ola.id),
        json!({"upload_id": first["id"]}),
    )
    .await
    .expect(StatusCode::OK);
    let old_url = app.get_as(&ola, "/api/me").await.json()["avatar"]["url"].clone();

    let proposal = avatar_upload(&app, &ala).await;
    let poll = app
        .post_as(
            &ala,
            "/api/polls",
            json!({"kind": "avatar", "target_user_id": ola.id, "upload_id": proposal["id"]}),
        )
        .await
        .json();
    let closed = vote_all(
        &app,
        &poll["id"],
        &[(&ala, 3), (&ola, -1), (&ewa, 1), (&admin, 0)],
    )
    .await;
    assert_eq!(closed["result"]["applied"], true);
    let new_avatar = app.get_as(&ola, "/api/me").await.json()["avatar"].clone();
    assert_eq!(
        new_avatar["url"], closed["proposed_image"]["url"],
        "the proposal is the new picture"
    );
    assert_eq!(
        closed["previous_image"]["url"], old_url,
        "the replaced picture is kept on the poll"
    );

    // Replacing her picture later does not destroy the vote's record.
    let later = avatar_upload(&app, &admin).await;
    app.put_as(
        &admin,
        &format!("/api/users/{}/avatar", ola.id),
        json!({"upload_id": later["id"]}),
    )
    .await
    .expect(StatusCode::OK);
    MediaSweeper.run(&app.ctx, app.now()).await.unwrap();
    for field in ["proposed_image", "previous_image"] {
        let url = app
            .get_as(&ala, &format!("/api/polls/{}", poll["id"]))
            .await
            .json()[field]["url"]
            .as_str()
            .unwrap()
            .to_owned();
        app.get(&url).await.expect(StatusCode::OK);
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn rejected_and_removal_avatar_votes(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let proposal = avatar_upload(&app, &ala).await;
    let poll = app
        .post_as(
            &ala,
            "/api/polls",
            json!({"kind": "avatar", "target_user_id": ola.id, "upload_id": proposal["id"]}),
        )
        .await
        .json();
    let rejected = vote_all(&app, &poll["id"], &[(&ala, 1), (&ola, -5), (&ewa, -1)]).await;
    assert_eq!(rejected["result"]["approved"], false);
    assert!(
        rejected["proposed_image"]["url"].is_string(),
        "the proposal is kept for the record"
    );
    assert_eq!(
        app.get_as(&ola, "/api/me").await.json()["avatar"],
        Value::Null
    );

    let admin = app.admin("boss").await;
    let picture = avatar_upload(&app, &admin).await;
    app.put_as(
        &admin,
        &format!("/api/users/{}/avatar", ewa.id),
        json!({"upload_id": picture["id"]}),
    )
    .await
    .expect(StatusCode::OK);
    let removal = app
        .post_as(
            &ala,
            "/api/polls",
            json!({"kind": "avatar", "target_user_id": ewa.id, "remove": true}),
        )
        .await
        .json();
    assert_eq!(removal["title"], "Usunięcie zdjęcia: ewa");
    let closed = vote_all(
        &app,
        &removal["id"],
        &[(&ala, 2), (&ola, 2), (&ewa, -1), (&admin, 1)],
    )
    .await;
    assert_eq!(closed["result"]["applied"], true);
    assert_eq!(
        app.get_as(&ewa, "/api/me").await.json()["avatar"],
        Value::Null
    );
    assert!(closed["previous_image"]["url"].is_string());
}

async fn score_poll(app: &TestApp, creator: &Member, others: &[&Member]) -> i64 {
    let ids: Vec<i64> = others.iter().map(|m| m.id).collect();
    let response = app
        .post_as(
            creator,
            "/api/polls",
            json!({"kind": "score", "title": "Grill?", "participant_ids": ids}),
        )
        .await;
    response.expect(StatusCode::CREATED).json()["id"]
        .as_i64()
        .unwrap()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn reminders_go_to_non_voters_once_per_stage(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola, ewa) = trio(&app).await;
    subscribe(app.db(), &[ala.id, ola.id, ewa.id]).await;
    let id = score_poll(&app, &ala, &[&ola, &ewa]).await;
    app.put_as(
        &ola,
        &format!("/api/polls/{id}/ballot"),
        json!({"value": 1}),
    )
    .await
    .expect(StatusCode::OK);
    take_pushes(app.db()).await;
    let start = app.now();
    let run = |hours| PollDeadlines.run(&app.ctx, start + TimeDelta::hours(hours));

    let next = run(23).await.unwrap();
    assert_eq!(
        next,
        Some(start + TimeDelta::hours(24)),
        "the job knows when to come back"
    );
    assert!(
        take_pushes(app.db()).await.is_empty(),
        "nothing before 24 h"
    );

    run(24).await.unwrap();
    let mut first = take_pushes(app.db()).await;
    first.sort();
    let body = "„Grill?” kończy się za 48 godz.".to_owned();
    assert_eq!(
        first,
        [
            (ala.id, "Głosowanie czeka na Twój głos".into(), body.clone()),
            (ewa.id, "Głosowanie czeka na Twój głos".into(), body)
        ]
    );
    run(30).await.unwrap();
    assert!(take_pushes(app.db()).await.is_empty(), "once per stage");

    // After downtime (missing 48 h), only the latest reminder goes out.
    run(70).await.unwrap();
    let late = take_pushes(app.db()).await;
    assert_eq!(late.len(), 2);
    assert!(
        late.iter()
            .all(|(.., body)| body == "„Grill?” kończy się za 3 godz.")
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn polls_expire_after_72_hours_counting_non_voters_as_abstaining(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola, ewa) = trio(&app).await;
    subscribe(app.db(), &[ala.id, ola.id, ewa.id]).await;
    let id = score_poll(&app, &ala, &[&ola, &ewa]).await;
    app.put_as(
        &ola,
        &format!("/api/polls/{id}/ballot"),
        json!({"value": 4}),
    )
    .await
    .expect(StatusCode::OK);
    take_pushes(app.db()).await;
    let start = app.now();

    PollDeadlines
        .run(&app.ctx, start + TimeDelta::hours(71))
        .await
        .unwrap();
    assert_eq!(
        app.get_as(&ala, &format!("/api/polls/{id}")).await.json()["status"],
        "open"
    );
    assert_eq!(
        take_pushes(app.db()).await.len(),
        2,
        "the 69-hour reminder to ala and ewa"
    );
    let next = PollDeadlines
        .run(&app.ctx, start + TimeDelta::hours(72))
        .await
        .unwrap();
    assert_eq!(next, None, "nothing open any more");
    let poll = app.get_as(&ala, &format!("/api/polls/{id}")).await.json();
    assert_eq!(
        (poll["status"].clone(), poll["close_reason"].clone()),
        (json!("closed"), json!("expired"))
    );
    assert_eq!(poll["result"]["score"], 4.0);
    assert_eq!(poll["result"]["abstain_count"], 2);
    let ewa_row = poll["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["user"]["username"] == "ewa")
        .unwrap()
        .clone();
    assert_eq!(ewa_row["revealed"]["reason"], "Brak głosu w terminie.");
    let mut notified: Vec<i64> = take_pushes(app.db())
        .await
        .into_iter()
        .map(|(user, ..)| user)
        .collect();
    notified.sort_unstable();
    assert_eq!(
        notified,
        [ala.id, ola.id, ewa.id],
        "everyone hears it ended"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn expiry_applies_an_approved_change_and_an_empty_vote_is_not_approved(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let approved = nickname_vote(&app, &ala, &ola, "Olka").await.json();
    vote_all(&app, &approved["id"], &[(&ala, 2), (&ewa, 1)]).await;
    let empty = nickname_vote(&app, &ala, &ewa, "Ewka").await.json();
    PollDeadlines
        .run(&app.ctx, app.now() + TimeDelta::hours(72))
        .await
        .unwrap();
    assert_eq!(app.get_as(&ola, "/api/me").await.json()["username"], "Olka");
    let empty = app
        .get_as(&ala, &format!("/api/polls/{}", empty["id"]))
        .await
        .json();
    assert_eq!(
        (
            empty["result"]["approved"].clone(),
            empty["result"]["applied"].clone()
        ),
        (json!(false), json!(false))
    );
    assert!(
        empty["result"].get("abstain_count").is_none(),
        "profile votes have no abstaining"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_poll_that_cannot_close_does_not_hold_up_the_others(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let broken = score_poll(&app, &ala, &[&ola, &ewa]).await;
    let fine = score_poll(&app, &ala, &[&ola, &ewa]).await;
    for id in [broken, fine] {
        app.put_as(
            &ola,
            &format!("/api/polls/{id}/ballot"),
            json!({"value": 2}),
        )
        .await
        .expect(StatusCode::OK);
    }
    sqlx::query("update poll_participants set ballot = '\"garbage\"' where poll_id = $1 and ballot is not null")
        .bind(broken)
        .execute(app.db())
        .await
        .unwrap();
    let status = |id: i64| {
        let app = &app;
        let ala = &ala;
        async move { app.get_as(ala, &format!("/api/polls/{id}")).await.json()["status"].clone() }
    };
    let at = app.now() + TimeDelta::hours(72);

    let next = PollDeadlines.run(&app.ctx, at).await.unwrap();
    assert_eq!(
        status(fine).await,
        "closed",
        "the healthy poll still closes"
    );
    assert_eq!(
        status(broken).await,
        "open",
        "the broken one is rolled back"
    );
    assert!(
        next.is_some_and(|next| next >= at + TimeDelta::seconds(30)),
        "the broken poll is retried later, not at once: {next:?}"
    );

    // Once it is repaired, the next run closes it.
    sqlx::query("update poll_participants set ballot = '{\"value\": 2}' where poll_id = $1 and ballot is not null")
        .bind(broken)
        .execute(app.db())
        .await
        .unwrap();
    let next = PollDeadlines.run(&app.ctx, at).await.unwrap();
    assert_eq!(status(broken).await, "closed");
    assert_eq!(next, None);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_scheduler_runs_the_deadline_job(pool: PgPool) {
    let app = TestApp::new(pool);
    let names = app::scheduler::Scheduler::standard().job_names();
    assert!(names.contains(&"poll-deadlines"), "{names:?}");
    let results = app::scheduler::Scheduler::standard()
        .run_once(&app.ctx)
        .await;
    assert!(results.iter().all(|(_, result)| result.is_ok()));
}
