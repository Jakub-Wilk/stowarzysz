//! Score votes and everything generic about polls: participants, hidden ballots, closing,
//! vetoes, abstentions, reactions, listing.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use app::events::{Event, HubMessage};
use axum::http::{Method, StatusCode};
use common::{Member, TestApp, subscribe, take_pushes};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn score_poll(app: &TestApp, creator: &Member, others: &[&Member]) -> i64 {
    let ids: Vec<i64> = others.iter().map(|m| m.id).collect();
    let response = app
        .post_as(
            creator,
            "/api/polls",
            json!({"kind": "score", "title": "Pizza w piątek?", "participant_ids": ids}),
        )
        .await;
    response.expect(StatusCode::CREATED).json()["id"]
        .as_i64()
        .unwrap()
}

async fn vote(app: &TestApp, who: &Member, id: i64, value: i64) -> common::TestResponse {
    app.put_as(
        who,
        &format!("/api/polls/{id}/ballot"),
        json!({"value": value}),
    )
    .await
}

async fn detail(app: &TestApp, who: &Member, id: i64) -> Value {
    app.get_as(who, &format!("/api/polls/{id}"))
        .await
        .expect(StatusCode::OK)
        .json()
}

async fn trio(app: &TestApp) -> (Member, Member, Member) {
    (
        app.member("ala").await,
        app.member("ola").await,
        app.member("ewa").await,
    )
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_endpoint_requires_a_session(pool: PgPool) {
    let app = TestApp::new(pool);
    for (method, path) in [
        (Method::GET, "/api/polls"),
        (Method::POST, "/api/polls"),
        (Method::GET, "/api/polls/1"),
        (Method::PUT, "/api/polls/1/ballot"),
        (Method::POST, "/api/polls/1/veto"),
        (Method::POST, "/api/polls/1/abstain"),
        (Method::POST, "/api/polls/1/close"),
        (Method::POST, "/api/polls/1/tie-break"),
        (Method::POST, "/api/polls/1/reactions"),
    ] {
        let body = (method != Method::GET).then(|| json!({}));
        assert_eq!(
            app.call(method.clone(), path, None, body).await.status,
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn creating_includes_the_creator_announces_and_notifies_the_others(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let _listener = app.start_listener().await;
    let mut events = app.ctx.hub.subscribe();
    let (ala, ola, ewa) = trio(&app).await;
    subscribe(app.db(), &[ala.id, ola.id, ewa.id]).await;

    let id = score_poll(&app, &ala, &[&ola]).await;
    let poll = detail(&app, &ala, id).await;
    assert_eq!(poll["kind"], "score");
    assert_eq!(poll["status"], "open");
    assert_eq!(poll["participant_count"], 2);
    let names: Vec<_> = poll["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["user"]["username"].clone())
        .collect();
    assert_eq!(names, [json!("ala"), json!("ola")]);
    let deadline =
        chrono::DateTime::parse_from_rfc3339(poll["deadline_at"].as_str().unwrap()).unwrap();
    assert_eq!(deadline.to_utc() - app.now(), chrono::TimeDelta::hours(72));
    assert_eq!(
        poll["actions"],
        json!({"vote": true, "veto": true, "abstain": true, "close": true, "tie_break": false})
    );

    assert_eq!(
        take_pushes(app.db()).await,
        [(
            ola.id,
            "Nowe głosowanie w Sejmiku".into(),
            "Pizza w piątek?".into()
        )]
    );
    let created = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(HubMessage::Event(envelope)) = events.recv().await {
                return envelope.event.clone();
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(created, Event::PollCreated { poll_id: id });
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn creation_is_validated(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, _) = trio(&app).await;
    // The creator alone is fine.
    app.post_as(
        &ala,
        "/api/polls",
        json!({"kind": "score", "title": "Sam ze sobą"}),
    )
    .await
    .expect(StatusCode::CREATED);

    let blank = app
        .post_as(&ala, "/api/polls", json!({"kind": "score", "title": "  "}))
        .await;
    blank.expect(StatusCode::BAD_REQUEST);
    assert_eq!(
        blank.json()["errors"]["title"],
        json!(["Podaj tytuł głosowania."])
    );
    let long = app
        .post_as(
            &ala,
            "/api/polls",
            json!({"kind": "score", "title": "x".repeat(201)}),
        )
        .await;
    assert_eq!(long.code(), "title_too_long");

    sqlx::query("update users set is_active = false where id = $1")
        .bind(ola.id)
        .execute(app.db())
        .await
        .unwrap();
    for ids in [json!([ola.id]), json!([999_999])] {
        let response = app
            .post_as(
                &ala,
                "/api/polls",
                json!({"kind": "score", "title": "x", "participant_ids": ids}),
            )
            .await;
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(
            response.json()["errors"]["participant_ids"],
            json!(["Nieznany lub nieaktywny użytkownik."])
        );
    }
    // Pact votes are called by pacts only; unknown kinds are not a poll at all.
    for kind in ["pact_ruling", "resolution_judgment", "yes_no"] {
        let response = app
            .post_as(&ala, "/api/polls", json!({"kind": kind, "title": "x"}))
            .await;
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(response.code(), "invalid_body", "{kind}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn everyone_reads_only_participants_vote(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let id = score_poll(&app, &ala, &[&ola]).await;
    let outsider = detail(&app, &ewa, id).await;
    assert_eq!(
        outsider["my"],
        json!({"participating": false, "has_voted": false, "vetoed": false, "abstained": false})
    );
    assert_eq!(outsider["actions"]["vote"], false);
    let refused = vote(&app, &ewa, id, 3).await;
    refused.expect(StatusCode::FORBIDDEN);
    assert_eq!(refused.code(), "not_participant");
    app.get_as(&ala, "/api/polls/999999")
        .await
        .expect(StatusCode::NOT_FOUND);
    vote(&app, &ala, 999_999, 1)
        .await
        .expect(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn ballots_are_validated(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, _) = trio(&app).await;
    let id = score_poll(&app, &ala, &[&ola]).await;
    let path = format!("/api/polls/{id}/ballot");
    for (body, code) in [
        (json!({"value": 6}), "score_out_of_range"),
        (json!({"value": -6}), "score_out_of_range"),
        (json!({}), "score_required"),
        (json!({"value": 2.5}), "invalid_body"),
        (json!({"value": true}), "invalid_body"),
        (json!({"value": "3"}), "invalid_body"),
    ] {
        let response = app.put_as(&ala, &path, body.clone()).await;
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(response.code(), code, "{body}");
    }
    vote(&app, &ala, id, -5).await.expect(StatusCode::OK);
    // Fields of other kinds are dropped.
    let extra = app
        .put_as(&ala, &path, json!({"value": 5, "choice": "o1", "evil": 1}))
        .await;
    assert_eq!(
        extra.expect(StatusCode::OK).json()["my_ballot"],
        json!({"value": 5})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn ballots_stay_hidden_until_the_end_and_can_change_until_then(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let id = score_poll(&app, &ala, &[&ola, &ewa]).await;
    vote(&app, &ala, id, 2).await.expect(StatusCode::OK);
    vote(&app, &ala, id, 4).await.expect(StatusCode::OK);
    vote(&app, &ola, id, -1).await.expect(StatusCode::OK);

    for viewer in [&ala, &ola, &ewa] {
        let poll = detail(&app, viewer, id).await;
        assert!(
            poll["participants"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p.get("revealed").is_none()),
            "hidden from everyone"
        );
        assert!(poll["result"].is_null());
    }
    assert_eq!(
        detail(&app, &ala, id).await["my_ballot"],
        json!({"value": 4}),
        "your own vote is yours to see"
    );
    assert_eq!(detail(&app, &ala, id).await["voted_count"], 2);

    // The last vote closes it and reveals everything.
    let last = vote(&app, &ewa, id, 0).await.expect(StatusCode::OK).json();
    assert_eq!(last["status"], "closed");
    assert_eq!(last["close_reason"], "auto");
    assert_eq!(
        last["result"],
        json!({"votes_cast": 3, "veto_count": 0, "vetoed": false, "score": 1.0, "tone": "positive", "abstain_count": 0})
    );
    let ala_row = last["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["user"]["username"] == "ala")
        .unwrap()
        .clone();
    assert_eq!(
        ala_row["revealed"],
        json!({"ballot": {"value": 4}, "vetoed": false, "abstained": false, "reason": "", "reaction": null})
    );

    // No voting after the end; the result is frozen.
    let late = vote(&app, &ola, id, 5).await;
    late.expect(StatusCode::CONFLICT);
    assert_eq!(late.code(), "poll_closed");
    assert_eq!(detail(&app, &ala, id).await["result"]["score"], 1.0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_veto_counts_as_minus_five_is_flagged_and_final(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let id = score_poll(&app, &ala, &[&ola, &ewa]).await;
    vote(&app, &ola, id, 5).await.expect(StatusCode::OK);
    let vetoed = app
        .post_as(
            &ola,
            &format!("/api/polls/{id}/veto"),
            json!({"reason": "  Nie lubię pizzy  "}),
        )
        .await;
    let vetoed = vetoed.expect(StatusCode::OK).json();
    assert_eq!(vetoed["my"]["vetoed"], true);
    assert_eq!(vetoed["my_reason"], "Nie lubię pizzy");
    assert_eq!(vetoed["actions"]["vote"], false);

    for response in [
        vote(&app, &ola, id, 3).await,
        app.post_as(&ola, &format!("/api/polls/{id}/abstain"), json!({}))
            .await,
    ] {
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(response.code(), "already_vetoed");
    }
    vote(&app, &ala, id, 5).await.expect(StatusCode::OK);
    let closed = vote(&app, &ewa, id, 5).await.json();
    assert_eq!(closed["result"]["vetoed"], true);
    assert_eq!(closed["result"]["veto_count"], 1);
    assert_eq!(closed["result"]["score"], 1.667);
    let ola_row = closed["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["user"]["username"] == "ola")
        .unwrap()
        .clone();
    assert_eq!(ola_row["revealed"]["reason"], "Nie lubię pizzy");

    let long = "x".repeat(301);
    let id = score_poll(&app, &ala, &[&ola]).await;
    let response = app
        .post_as(
            &ola,
            &format!("/api/polls/{id}/veto"),
            json!({"reason": long}),
        )
        .await;
    assert_eq!(response.code(), "reason_too_long");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_the_creator_ends_early_with_the_ballots_cast(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola, ewa) = trio(&app).await;
    subscribe(app.db(), &[ala.id, ola.id, ewa.id]).await;
    let id = score_poll(&app, &ala, &[&ola, &ewa]).await;
    vote(&app, &ola, id, 4).await.expect(StatusCode::OK);
    vote(&app, &ala, id, -1).await.expect(StatusCode::OK);
    let refused = app
        .post_as(&ola, &format!("/api/polls/{id}/close"), json!({}))
        .await;
    refused.expect(StatusCode::FORBIDDEN);
    assert_eq!(refused.code(), "not_creator");
    take_pushes(app.db()).await;

    let closed = app
        .post_as(&ala, &format!("/api/polls/{id}/close"), json!({}))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(closed["close_reason"], "creator");
    assert_eq!(closed["result"]["score"], 1.5);
    assert_eq!(closed["result"]["abstain_count"], 1);
    let ewa_row = closed["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["user"]["username"] == "ewa")
        .unwrap()
        .clone();
    assert_eq!(ewa_row["revealed"]["abstained"], true);
    assert_eq!(
        ewa_row["revealed"]["reason"],
        "Brak głosu przed zakończeniem głosowania."
    );
    let mut notified: Vec<i64> = take_pushes(app.db())
        .await
        .into_iter()
        .map(|(user, title, _)| {
            assert_eq!(title, "Głosowanie zakończone");
            user
        })
        .collect();
    notified.sort_unstable();
    assert_eq!(notified, [ala.id, ola.id, ewa.id]);

    // Without any ballot there is no score.
    let empty = score_poll(&app, &ala, &[&ola]).await;
    let closed = app
        .post_as(&ala, &format!("/api/polls/{empty}/close"), json!({}))
        .await
        .json();
    assert_eq!(
        (
            closed["result"]["score"].clone(),
            closed["result"]["tone"].clone()
        ),
        (Value::Null, Value::Null)
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_tone_follows_the_sign_of_the_average(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, _) = trio(&app).await;
    for (a, b, tone) in [(1, 0, "positive"), (1, -1, "neutral"), (-1, 0, "negative")] {
        let id = score_poll(&app, &ala, &[&ola]).await;
        vote(&app, &ala, id, a).await.expect(StatusCode::OK);
        let closed = vote(&app, &ola, id, b).await.json();
        assert_eq!(closed["result"]["tone"], tone, "{a} {b}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn abstaining_is_left_out_of_the_score(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa) = trio(&app).await;
    let id = score_poll(&app, &ala, &[&ola, &ewa]).await;
    let abstained = app
        .post_as(
            &ola,
            &format!("/api/polls/{id}/abstain"),
            json!({"reason": "Nie znam się"}),
        )
        .await
        .json();
    assert_eq!(
        abstained["my"],
        json!({"participating": true, "has_voted": true, "vetoed": false, "abstained": true})
    );
    assert_eq!(abstained["my_ballot"], Value::Null);
    // The reason stays hidden until the end.
    assert!(
        detail(&app, &ala, id).await["participants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("revealed").is_none())
    );
    // A vote replaces an abstention.
    app.post_as(&ewa, &format!("/api/polls/{id}/abstain"), json!({}))
        .await
        .expect(StatusCode::OK);
    vote(&app, &ewa, id, 3).await.expect(StatusCode::OK);
    let closed = vote(&app, &ala, id, 5).await.json();
    assert_eq!(closed["result"]["votes_cast"], 2);
    assert_eq!(closed["result"]["score"], 4.0);
    assert_eq!(closed["result"]["abstain_count"], 1);
    let ola_row = closed["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["user"]["username"] == "ola")
        .unwrap()
        .clone();
    assert_eq!(ola_row["revealed"]["reason"], "Nie znam się");

    // Everyone abstaining: no score at all.
    let id = score_poll(&app, &ala, &[&ola]).await;
    app.post_as(&ala, &format!("/api/polls/{id}/abstain"), json!({}))
        .await
        .expect(StatusCode::OK);
    let closed = app
        .post_as(&ola, &format!("/api/polls/{id}/abstain"), json!({}))
        .await
        .json();
    assert_eq!(closed["result"]["score"], Value::Null);
    assert_eq!(closed["result"]["abstain_count"], 2);
    // Only participants abstain.
    let id = score_poll(&app, &ala, &[]).await;
    app.post_as(&ola, &format!("/api/polls/{id}/abstain"), json!({}))
        .await
        .expect(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn reactions_are_for_finished_votes(pool: PgPool) {
    let app = TestApp::new(pool);
    let _listener = app.start_listener().await;
    let (ala, ola, ewa) = trio(&app).await;
    let id = score_poll(&app, &ala, &[&ola]).await;
    let path = format!("/api/polls/{id}/reactions");
    let open = app.post_as(&ala, &path, json!({"emoji": "🔥"})).await;
    open.expect(StatusCode::BAD_REQUEST);
    assert_eq!(open.code(), "poll_open");
    app.post_as(&ala, &format!("/api/polls/{id}/close"), json!({}))
        .await
        .expect(StatusCode::OK);

    let mut events = app.ctx.hub.subscribe();
    for emoji in ["❤️", "🔥", "😭", "👎", "🤣"] {
        app.post_as(&ola, &path, json!({"emoji": emoji}))
            .await
            .expect(StatusCode::NO_CONTENT);
    }
    let other = app.post_as(&ola, &path, json!({"emoji": "🍕"})).await;
    assert_eq!(other.code(), "invalid_reaction");
    // Spamming is fine and the last one sticks.
    for _ in 0..5 {
        app.post_as(&ola, &path, json!({"emoji": "🔥"}))
            .await
            .expect(StatusCode::NO_CONTENT);
    }
    // An outsider's reaction is seen by everyone but stored for nobody.
    app.post_as(&ewa, &path, json!({"emoji": "🤣"}))
        .await
        .expect(StatusCode::NO_CONTENT);

    let poll = detail(&app, &ala, id).await;
    let reactions: Vec<_> = poll["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["revealed"]["reaction"].clone())
        .collect();
    assert_eq!(reactions, [Value::Null, json!("🔥")]); // ala, ola
    let mut seen = Vec::new();
    while seen.len() < 11 {
        let message = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap();
        if let Ok(HubMessage::Event(envelope)) = message
            && let Event::PollReaction { user_id, emoji, .. } = &envelope.event
        {
            seen.push((*user_id, emoji.clone()));
        }
    }
    assert_eq!(seen.last().unwrap(), &(ewa.id, "🤣".to_owned()));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_list_is_paged_newest_first_and_filters(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, _) = trio(&app).await;
    let mut ids = Vec::new();
    for n in 0..23 {
        app.clock.advance(chrono::TimeDelta::seconds(1));
        let ala = Member {
            token: app.session(ala.id).await.access,
            ..ala.clone()
        };
        let others: Vec<&Member> = if n % 2 == 0 { vec![&ola] } else { vec![] };
        ids.push(score_poll(&app, &ala, &others).await);
    }
    let ala = Member {
        token: app.session(ala.id).await.access,
        ..ala
    };
    let ola = Member {
        token: app.session(ola.id).await.access,
        ..ola
    };
    let first = app
        .get_as(&ala, "/api/polls")
        .await
        .expect(StatusCode::OK)
        .json();
    let page: Vec<i64> = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    let newest: Vec<i64> = ids.iter().rev().take(20).copied().collect();
    assert_eq!(page, newest);
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = app
        .get_as(&ala, &format!("/api/polls?cursor={cursor}"))
        .await
        .json();
    assert_eq!(second["items"].as_array().unwrap().len(), 3);
    assert_eq!(second["next_cursor"], Value::Null);
    app.get_as(&ala, "/api/polls?cursor=garbage")
        .await
        .expect(StatusCode::BAD_REQUEST);

    // Open polls come unpaged; `participating` keeps only yours.
    let open = app.get_as(&ala, "/api/polls?status=open").await.json();
    assert_eq!(
        (
            open["items"].as_array().unwrap().len(),
            open["next_cursor"].clone()
        ),
        (23, Value::Null)
    );
    assert_eq!(
        app.get_as(&ola, "/api/polls?status=open&participating=true")
            .await
            .json()["items"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    assert_eq!(
        app.get_as(&ala, "/api/polls?status=closed").await.json()["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    app.get_as(&ala, "/api/polls?status=weird")
        .await
        .expect(StatusCode::BAD_REQUEST);

    // A list item shows progress and your own state, never ballots.
    vote(&app, &ola, ids[0], 2).await.expect(StatusCode::OK);
    let items = app.get_as(&ola, "/api/polls?status=open").await.json()["items"].clone();
    let item = items
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == ids[0])
        .unwrap()
        .clone();
    assert_eq!(
        (
            item["participant_count"].clone(),
            item["voted_count"].clone()
        ),
        (json!(2), json!(1))
    );
    assert_eq!(item["my"]["has_voted"], true);
    assert_eq!(item["creator"]["username"], "ala");
    assert!(item.get("participants").is_none());
}
