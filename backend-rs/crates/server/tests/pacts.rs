//! Pacts: creating, visibility, invites, bets and their settlement, pictures.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::time::Duration;

use app::events::{Event, HubMessage};
use axum::http::{Method, StatusCode};
use common::pacts::{
    accept, actions, bet, claim, claim_action, claim_state, confirm, create, decline, detail,
    last_claim, owed, participant, propose, settle, state_of, status, url,
};
use common::{Member, TestApp, png, subscribe, take_pushes};
use serde_json::{Value, json};
use sqlx::PgPool;

fn owes(rows: &[(&str, &str, i64)]) -> BTreeSet<(String, String, i64)> {
    rows.iter()
        .map(|(d, c, a)| ((*d).to_owned(), (*c).to_owned(), *a))
        .collect()
}

async fn four(app: &TestApp) -> (Member, Member, Member, Member) {
    (
        app.member("alice").await,
        app.member("bob").await,
        app.member("carol").await,
        app.member("dave").await, // never takes part in anything
    )
}

/// Alice bets 10 zł against bob and 15 zł against carol; both accepted.
async fn started(app: &TestApp, alice: &Member, bob: &Member, carol: &Member) -> i64 {
    let pact = bet(app, alice, &[(bob, 1000), (carol, 1500)]).await;
    accept(app, bob, pact).await.expect(StatusCode::OK);
    accept(app, carol, pact).await.expect(StatusCode::OK);
    pact
}

// --- auth -------------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_endpoint_requires_auth(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, _) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000)]).await;
    let routes = [
        (Method::GET, "/api/pacts".to_owned()),
        (Method::POST, "/api/pacts".to_owned()),
        (Method::GET, url(pact, "")),
        (Method::POST, url(pact, "respond")),
        (Method::POST, url(pact, "cancel")),
        (Method::POST, url(pact, "join")),
        (Method::POST, url(pact, "withdraw")),
        (Method::POST, url(pact, "participants/1/decide")),
        (Method::POST, url(pact, "claims")),
        (Method::POST, url(pact, "claims/1/confirm")),
        (Method::POST, url(pact, "claims/1/dispute")),
        (Method::POST, url(pact, "claims/1/escalate")),
        (Method::POST, url(pact, "judgment")),
        (Method::POST, url(pact, "attachments")),
        (Method::DELETE, url(pact, "attachments/1")),
    ];
    for (method, uri) in routes {
        let response = app.call(method.clone(), &uri, None, Some(json!({}))).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

// --- create -----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn create_invites_opponents_and_notifies_them(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob, carol, _) = four(&app).await;
    subscribe(app.db(), &[alice.id, bob.id, carol.id]).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 1500)]).await;
    let view = detail(&app, &alice, pact).await;
    assert_eq!(view["status"], "proposed");
    assert_eq!(view["kind"], "bet");
    let rows: Vec<(String, String, String, Value)> = view["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["user"]["username"].as_str().unwrap().to_owned(),
                p["role"].as_str().unwrap().to_owned(),
                p["state"].as_str().unwrap().to_owned(),
                p["stake_amount"].clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("alice".into(), "host".into(), "active".into(), Value::Null),
            (
                "bob".into(),
                "opponent".into(),
                "invited".into(),
                json!(1000)
            ),
            (
                "carol".into(),
                "opponent".into(),
                "invited".into(),
                json!(1500)
            ),
        ]
    );
    assert!(view["participants"][1]["expires_at"].is_string());
    let urls: Vec<String> = sqlx::query_scalar("select payload->>'url' from push_outbox")
        .fetch_all(app.db())
        .await
        .unwrap();
    assert_eq!(urls, vec![format!("/pacts/{pact}"); 2]);
    let pushes = take_pushes(app.db()).await;
    let told: BTreeSet<i64> = pushes.iter().map(|(user, _, _)| *user).collect();
    assert_eq!(told, BTreeSet::from([bob.id, carol.id]));
    assert_eq!(pushes[0].1, "Nowa umowa");
    assert_eq!(pushes[0].2, "alice zaprasza Cię: Witcher 4 w 2027");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn create_validation(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, _) = four(&app).await;
    let attempt = |over: Value| {
        let mut body = json!({"kind": "bet", "title": "t", "condition": "c"});
        body.as_object_mut()
            .unwrap()
            .extend(over.as_object().unwrap().clone());
        let app = &app;
        let alice = &alice;
        async move { app.post_as(alice, "/api/pacts", body).await }
    };
    // Nobody invited is fine: anyone can still ask to join.
    attempt(json!({"opponents": []}))
        .await
        .expect(StatusCode::CREATED);
    let cases = [
        (
            json!({"opponents": [{"user_id": alice.id, "stake_amount": 100}]}),
            "self_invite",
        ),
        (
            json!({"opponents": [{"user_id": 9999, "stake_amount": 100}]}),
            "unknown_invitee",
        ),
        (
            json!({"opponents": [{"user_id": bob.id}]}),
            "stake_required",
        ),
        (
            json!({"opponents": [{"user_id": bob.id, "stake_amount": 100}, {"user_id": bob.id, "stake_amount": 100}]}),
            "invited_twice",
        ),
        (
            json!({"opponents": [{"user_id": bob.id, "stake_amount": -5}]}),
            "invalid_stake",
        ),
        (json!({"title": "  "}), "required"),
        (json!({"condition": ""}), "required"),
        (json!({"title": "x".repeat(201)}), "too_long"),
        (json!({"sides": ["a", "b"]}), "sides_not_allowed"),
    ];
    for (over, code) in cases {
        let response = attempt(over.clone()).await;
        assert_eq!(
            (response.status, response.code()),
            (StatusCode::BAD_REQUEST, code.to_owned()),
            "{over}"
        );
    }
    let unknown_kind = attempt(json!({"kind": "nope"})).await;
    unknown_kind.expect(StatusCode::BAD_REQUEST);
    let field = attempt(json!({"opponents": [{"user_id": bob.id}]}))
        .await
        .json();
    assert!(
        field.to_string().contains("opponents.0.stake_amount"),
        "{field}"
    );
    assert_eq!(app.count("select count(*) from pacts").await, 1);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_non_cash_stake_is_enough(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, _) = four(&app).await;
    let body = json!({
        "kind": "bet", "title": "t", "condition": "c",
        "opponents": [{"user_id": bob.id, "stake_note": "  kolacja "}],
    });
    let pact = create(&app, &alice, body).await;
    let view = detail(&app, &bob, pact).await;
    assert_eq!(view["participants"][1]["stake_note"], "kolacja");
    assert_eq!(view["participants"][1]["stake_amount"], Value::Null);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_pact_can_start_with_no_invitees_and_is_broadcast(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, _, _, dave) = four(&app).await;
    let _listener = app.start_listener().await;
    let mut rx = app.ctx.hub.subscribe();
    let pact = create(
        &app,
        &alice,
        json!({"kind": "bet", "title": "t", "condition": "c"}),
    )
    .await;
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(HubMessage::Event(envelope)) = rx.recv().await {
                return envelope;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(event.event, Event::PactCreated { pact_id: pact });
    assert!(event.audience.includes(dave.id), "everyone hears about it");
    detail(&app, &dave, pact).await;
}

// --- visibility -------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_member_can_read_every_pact_but_only_participants_act(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, dave) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000)]).await;
    let view = detail(&app, &dave, pact).await;
    assert_eq!(view["my"], Value::Null);
    let list = app.get_as(&dave, "/api/pacts").await.json();
    assert_eq!(list["items"][0]["id"], pact);
    assert_eq!(list["items"][0]["active_count"], 1);
    assert_eq!(
        accept(&app, &dave, pact).await.status,
        StatusCode::FORBIDDEN
    );
    let cancel = app.post_as(&dave, &url(pact, "cancel"), json!({})).await;
    assert_eq!(cancel.status, StatusCode::FORBIDDEN);
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    assert_eq!(
        propose(&app, &dave, pact, &bob, "host").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_as(&dave, &url(pact + 100, "")).await.status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn list_filters_and_pages(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, dave) = four(&app).await;
    let mine = bet(&app, &alice, &[(&bob, 1000)]).await;
    app.clock.advance(chrono::TimeDelta::seconds(1));
    let other = bet(&app, &bob, &[(&alice, 500)]).await;
    let ids = |page: Value| -> Vec<i64> {
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect()
    };
    let everything = app.get_as(&dave, "/api/pacts").await.json();
    assert_eq!(ids(everything), [other, mine], "newest first");
    assert_eq!(
        ids(app.get_as(&dave, "/api/pacts?mine=true").await.json()),
        Vec::<i64>::new()
    );
    assert_eq!(
        ids(app.get_as(&alice, "/api/pacts?mine=true").await.json()),
        [other, mine]
    );
    assert_eq!(
        ids(app.get_as(&alice, "/api/pacts?status=active").await.json()),
        Vec::<i64>::new()
    );
    assert_eq!(
        ids(app
            .get_as(&alice, "/api/pacts?status=proposed")
            .await
            .json()),
        [other, mine]
    );
    let mine_row = &app.get_as(&bob, "/api/pacts").await.json()["items"][1];
    assert_eq!(mine_row["my"]["role"], "opponent");
    assert_eq!(mine_row["my"]["state"], "invited");

    for i in 0..31 {
        app.clock.advance(chrono::TimeDelta::seconds(1));
        create(
            &app,
            &dave,
            json!({"kind": "bet", "title": format!("p{i}"), "condition": "c"}),
        )
        .await;
    }
    let first = app.get_as(&alice, "/api/pacts").await.json();
    assert_eq!(first["items"].as_array().unwrap().len(), 30);
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = app
        .get_as(&alice, &format!("/api/pacts?cursor={cursor}"))
        .await
        .json();
    assert_eq!(ids(second.clone()).len(), 3);
    assert_eq!(second["next_cursor"], Value::Null);
}

// --- invites ----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn each_opponent_accepts_separately_and_the_pact_starts_with_the_first(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 1500)]).await;
    assert_eq!(actions(&app, &bob, pact).await["respond"], true);
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    assert_eq!(status(&app, pact).await, "active");
    assert_eq!(state_of(&app, pact, &bob).await, "active");
    assert_eq!(state_of(&app, pact, &carol).await, "invited");
    let again = accept(&app, &bob, pact).await;
    assert_eq!(
        (again.status, again.code()),
        (StatusCode::CONFLICT, "already_answered".into())
    );
    accept(&app, &carol, pact).await.expect(StatusCode::OK);
    let expires: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "select expires_at from pact_participants where pact_id = $1 and user_id = $2",
    )
    .bind(pact)
    .bind(carol.id)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(expires, None, "an answered invite no longer lapses");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn declining_one_wager_leaves_the_others(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 1500)]).await;
    decline(&app, &bob, pact).await.expect(StatusCode::OK);
    accept(&app, &carol, pact).await.expect(StatusCode::OK);
    assert_eq!(status(&app, pact).await, "active");
    assert_eq!(state_of(&app, pact, &bob).await, "declined");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_pact_stays_open_for_joiners_when_every_invitee_says_no(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, dave) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 1500)]).await;
    for who in [&bob, &carol] {
        decline(&app, who, pact).await.expect(StatusCode::OK);
    }
    assert_eq!(status(&app, pact).await, "proposed");
    assert_eq!(actions(&app, &dave, pact).await["join"], true);
    assert_eq!(actions(&app, &alice, pact).await["cancel"], true);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn strangers_cannot_respond(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, dave) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000)]).await;
    let response = accept(&app, &dave, pact).await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::FORBIDDEN, "not_participant".into())
    );
    assert_eq!(state_of(&app, pact, &bob).await, "invited");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_creator_can_cancel_before_it_starts_only(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob, carol, _) = four(&app).await;
    subscribe(app.db(), &[alice.id, bob.id, carol.id]).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 500)]).await;
    decline(&app, &carol, pact).await.expect(StatusCode::OK);
    take_pushes(app.db()).await;
    let by_bob = app.post_as(&bob, &url(pact, "cancel"), json!({})).await;
    assert_eq!(by_bob.status, StatusCode::FORBIDDEN);
    let view = app
        .post_as(&alice, &url(pact, "cancel"), json!({}))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(view["status"], "cancelled");
    assert_eq!(view["participants"][1]["state"], "withdrawn");
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            bob.id,
            "Umowa anulowana".into(),
            "alice anuluje umowę: Witcher 4 w 2027".into()
        )],
        "only those still in it are told"
    );
    let late = accept(&app, &bob, pact).await;
    assert_eq!(late.status, StatusCode::CONFLICT);

    let started = bet(&app, &alice, &[(&bob, 1000)]).await;
    accept(&app, &bob, started).await.expect(StatusCode::OK);
    let response = app
        .post_as(&alice, &url(started, "cancel"), json!({}))
        .await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::CONFLICT, "pact_started".into())
    );
}

// --- outcomes and settlement ------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_host_wins_and_is_paid_by_each_opponent(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    settle(&app, pact, &alice, &bob, &bob, "host").await;
    assert_eq!(owed(&app).await, owes(&[("bob", "alice", 1000)]));
    assert_eq!(status(&app, pact).await, "active", "carol's wager is open");
    settle(&app, pact, &carol, &alice, &carol, "host").await;
    assert_eq!(
        owed(&app).await,
        owes(&[("bob", "alice", 1000), ("carol", "alice", 1500)])
    );
    let view = detail(&app, &alice, pact).await;
    assert_eq!(view["status"], "resolved");
    assert!(view["resolved_at"].is_string());
    let wagers = view["outcome"]["wagers"].as_array().unwrap();
    assert_eq!(wagers.len(), 2);
    assert_eq!(
        wagers[0],
        json!({"participant_id": participant(&app, pact, &bob).await, "result": {"winner": "host"}})
    );
    let entries: Vec<(String, String, Option<i64>)> =
        sqlx::query_as("select kind, source, pact_id from ledger_entries order by id")
            .fetch_all(app.db())
            .await
            .unwrap();
    assert_eq!(
        entries,
        vec![("debt".into(), "pact".into(), Some(pact)); 2],
        "one ledger entry per settled wager"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_host_loses_and_pays_each_opponent_their_own_stake(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    settle(&app, pact, &bob, &alice, &bob, "opponent").await;
    settle(&app, pact, &alice, &carol, &carol, "opponent").await;
    assert_eq!(
        owed(&app).await,
        owes(&[("alice", "bob", 1000), ("alice", "carol", 1500)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn wagers_settle_independently_with_different_results(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    settle(&app, pact, &alice, &bob, &bob, "host").await;
    settle(&app, pact, &alice, &carol, &carol, "draw").await;
    assert_eq!(owed(&app).await, owes(&[("bob", "alice", 1000)]));
    assert_eq!(status(&app, pact).await, "resolved");
    let draw = last_claim(&app, pact).await;
    assert_eq!(
        common::pacts::verdicts(&app, draw).await,
        [
            ("alice".into(), "draw".into()),
            ("carol".into(), "draw".into())
        ]
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_pact_with_an_unanswered_invite_is_not_resolved(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 1500)]).await;
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    settle(&app, pact, &alice, &bob, &bob, "host").await;
    assert_eq!(status(&app, pact).await, "active");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_bet_resolves_once_its_last_waiting_invite_is_declined(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, dave) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 1500)]).await;
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    common::pacts::join(&app, &dave, pact, json!({"stake_amount": 100}))
        .await
        .expect(StatusCode::OK);
    settle(&app, pact, &alice, &bob, &bob, "host").await;
    assert_eq!(status(&app, pact).await, "active");
    decline(&app, &carol, pact).await.expect(StatusCode::OK);
    assert_eq!(
        status(&app, pact).await,
        "resolved",
        "nothing is left to wait for"
    );
    assert_eq!(
        state_of(&app, pact, &dave).await,
        "withdrawn",
        "an undecided join request goes with it"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_proposer_cannot_confirm_their_own_claim(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    propose(&app, &alice, pact, &bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let claim = last_claim(&app, pact).await;
    let response = confirm(&app, &alice, pact, claim).await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::FORBIDDEN, "own_claim".into())
    );
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_the_wagers_two_sides_can_propose_or_confirm(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    assert_eq!(
        propose(&app, &carol, pact, &bob, "host").await.status,
        StatusCode::FORBIDDEN
    );
    propose(&app, &alice, pact, &bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let claim = last_claim(&app, pact).await;
    assert_eq!(
        confirm(&app, &carol, pact, claim).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(actions(&app, &bob, pact).await["confirm"], json!([claim]));
    assert_eq!(actions(&app, &carol, pact).await["confirm"], json!([]));
    // A claim is only reachable through its own pact.
    let other = bet(&app, &bob, &[(&carol, 100)]).await;
    assert_eq!(
        confirm(&app, &bob, other, claim).await.status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_dispute_keeps_the_wager_open_and_a_new_claim_can_follow(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    propose(&app, &alice, pact, &bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let first = last_claim(&app, pact).await;
    let view = claim_action(&app, &bob, pact, first, "dispute")
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(view["claims"][0]["state"], "disputed");
    assert_eq!(claim_state(&app, first).await, "disputed");
    assert_eq!(state_of(&app, pact, &bob).await, "active");
    assert_eq!(
        confirm(&app, &bob, pact, first).await.status,
        StatusCode::CONFLICT
    );
    settle(&app, pact, &bob, &alice, &bob, "opponent").await;
    assert_eq!(owed(&app).await, owes(&[("alice", "bob", 1000)]));
    assert_eq!(claim_state(&app, first).await, "superseded");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_new_claim_supersedes_the_pending_one(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    propose(&app, &alice, pact, &bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let first = last_claim(&app, pact).await;
    let view = propose(&app, &bob, pact, &bob, "opponent")
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(claim_state(&app, first).await, "superseded");
    let live: Vec<i64> = view["claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    assert_eq!(live, [last_claim(&app, pact).await]);
    let stale = confirm(&app, &bob, pact, first).await;
    assert_eq!(
        (stale.status, stale.code()),
        (StatusCode::CONFLICT, "claim_decided".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn claim_validation_and_state_checks(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, _) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000)]).await;
    let early = propose(&app, &alice, pact, &bob, "host").await;
    assert_eq!(
        (early.status, early.code()),
        (StatusCode::CONFLICT, "pact_closed".into()),
        "not started yet"
    );
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    let nobody = propose(&app, &alice, pact, &bob, "nobody").await;
    assert_eq!(
        (nobody.status, nobody.code()),
        (StatusCode::BAD_REQUEST, "invalid_result".into())
    );
    let host_row = participant(&app, pact, &alice).await;
    let not_a_wager = claim(
        &app,
        &alice,
        pact,
        json!({"winner": "host"}),
        Some(host_row),
    )
    .await;
    assert_eq!(
        (not_a_wager.status, not_a_wager.code()),
        (StatusCode::BAD_REQUEST, "wager_required".into())
    );
    let wager = participant(&app, pact, &bob).await;
    for bad in [json!([1, 2]), json!("host"), json!({"void": false})] {
        let response = claim(&app, &alice, pact, bad.clone(), Some(wager)).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let malformed = app
        .send(
            axum::http::Request::post(url(pact, "claims"))
                .header("authorization", format!("Bearer {}", alice.token))
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{\"result\": "))
                .unwrap(),
        )
        .await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_non_cash_wager_settles_into_an_item_debt(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, _) = four(&app).await;
    let body = json!({
        "kind": "bet", "title": "t", "condition": "c",
        "opponents": [{"user_id": bob.id, "stake_note": "kolacja"}],
    });
    let pact = create(&app, &alice, body).await;
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    settle(&app, pact, &alice, &bob, &bob, "host").await;
    let entry: (String, String, Option<i64>, String, Option<i64>, String) =
        sqlx::query_as("select kind, source, pact_id, item, amount, currency from ledger_entries")
            .fetch_one(app.db())
            .await
            .unwrap();
    assert_eq!(
        entry,
        (
            "debt".into(),
            "pact".into(),
            Some(pact),
            "kolacja".into(),
            Some(1),
            String::new()
        )
    );
    let debt: (i64, i64, String) =
        sqlx::query_as("select debtor_id, creditor_id, item from ledger_obligations")
            .fetch_one(app.db())
            .await
            .unwrap();
    assert_eq!(debt, (bob.id, alice.id, "kolacja".into()));
    assert_eq!(status(&app, pact).await, "resolved");
    // The settlement changes only through the pact.
    let id: i64 = sqlx::query_scalar("select id from ledger_entries")
        .fetch_one(app.db())
        .await
        .unwrap();
    let ledger = app
        .get_as(&alice, &format!("/api/ledger/entries/{id}"))
        .await
        .json();
    assert_eq!(ledger["actions"]["cancel"], false);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn settling_pushes_the_proposer(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob, carol, _) = four(&app).await;
    subscribe(app.db(), &[alice.id, bob.id, carol.id]).await;
    let pact = started(&app, &alice, &bob, &carol).await;
    take_pushes(app.db()).await;
    propose(&app, &alice, pact, &bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let pushes = take_pushes(app.db()).await;
    assert_eq!(
        pushes,
        [(
            bob.id,
            "Potwierdź wynik umowy".into(),
            "alice podaje wynik: Witcher 4 w 2027".into()
        )]
    );
    let claim = last_claim(&app, pact).await;
    confirm(&app, &bob, pact, claim)
        .await
        .expect(StatusCode::OK);
    let pushes: Vec<_> = take_pushes(app.db())
        .await
        .into_iter()
        .filter(|(_, title, _)| title.starts_with("Umowa"))
        .collect();
    assert_eq!(
        pushes,
        [(
            alice.id,
            "Umowa rozstrzygnięta".into(),
            "Witcher 4 w 2027: 10 zł".into()
        )]
    );
}

// --- pictures ---------------------------------------------------------------------------------

async fn photo(app: &TestApp, who: &Member) -> Value {
    app.upload(who, "photo", "image/png", png(64, 48))
        .await
        .expect(StatusCode::CREATED)
        .json()["id"]
        .clone()
}

async fn attach(app: &TestApp, who: &Member, pact: i64, caption: &str) -> common::TestResponse {
    let upload = photo(app, who).await;
    app.post_as(
        who,
        &url(pact, "attachments"),
        json!({"upload_id": upload, "caption": caption}),
    )
    .await
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_participant_can_attach_a_picture_everyone_sees(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, _, dave) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000)]).await;
    let view = attach(&app, &alice, pact, "  umowa na serwetce ")
        .await
        .expect(StatusCode::CREATED)
        .json();
    let picture = &view["attachments"][0];
    assert_eq!(picture["caption"], "umowa na serwetce");
    assert_eq!(picture["uploaded_by"]["username"], "alice");
    assert_eq!(
        (picture["width"].clone(), picture["height"].clone()),
        (json!(64), json!(48))
    );
    let image_url = picture["image"]["url"].as_str().unwrap();
    assert!(image_url.starts_with("/api/media/photos/"), "{image_url}");
    let seen = detail(&app, &dave, pact).await;
    assert_eq!(seen["attachments"].as_array().unwrap().len(), 1);
    let served = app.get(image_url).await;
    assert_eq!(served.status, StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn who_may_attach(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, dave) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 500)]).await;
    attach(&app, &bob, pact, "")
        .await
        .expect(StatusCode::CREATED);
    let outsider = attach(&app, &dave, pact, "").await;
    assert_eq!(
        (outsider.status, outsider.code()),
        (StatusCode::FORBIDDEN, "not_participant".into())
    );
    decline(&app, &carol, pact).await.expect(StatusCode::OK);
    assert_eq!(
        attach(&app, &carol, pact, "").await.status,
        StatusCode::FORBIDDEN,
        "declining gives up the right"
    );
    accept(&app, &bob, pact).await.expect(StatusCode::OK);
    settle(&app, pact, &alice, &bob, &bob, "host").await;
    assert_eq!(status(&app, pact).await, "resolved");
    attach(&app, &bob, pact, "dowód")
        .await
        .expect(StatusCode::CREATED);
    let long = attach(&app, &bob, pact, &"x".repeat(201)).await;
    assert_eq!(long.status, StatusCode::BAD_REQUEST);
    let foreign = photo(&app, &alice).await;
    let stolen = app
        .post_as(
            &bob,
            &url(pact, "attachments"),
            json!({"upload_id": foreign}),
        )
        .await;
    assert_eq!(
        (stolen.status, stolen.code()),
        (StatusCode::BAD_REQUEST, "upload_not_found".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_number_of_pictures_is_capped(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, _, _, _) = four(&app).await;
    let pact = create(
        &app,
        &alice,
        json!({"kind": "bet", "title": "t", "condition": "c"}),
    )
    .await;
    for _ in 0..10 {
        attach(&app, &alice, pact, "")
            .await
            .expect(StatusCode::CREATED);
    }
    assert_eq!(actions(&app, &alice, pact).await["attach"], false);
    let eleventh = attach(&app, &alice, pact, "").await;
    assert_eq!(
        (eleventh.status, eleventh.code()),
        (StatusCode::BAD_REQUEST, "too_many_attachments".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_uploader_or_the_creator_can_delete_a_picture(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol, _) = four(&app).await;
    let pact = bet(&app, &alice, &[(&bob, 1000), (&carol, 500)]).await;
    for who in [&bob, &bob] {
        attach(&app, who, pact, "")
            .await
            .expect(StatusCode::CREATED);
    }
    let pictures: Vec<i64> = detail(&app, &alice, pact).await["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_i64().unwrap())
        .collect();
    let remove = |who: &Member, id: i64| {
        let uri = url(pact, &format!("attachments/{id}"));
        let app = &app;
        let who = who.clone();
        async move { app.delete_as(&who, &uri).await }
    };
    assert_eq!(
        remove(&carol, pictures[0]).await.status,
        StatusCode::FORBIDDEN
    );
    remove(&bob, pictures[0]).await.expect(StatusCode::OK);
    let left = remove(&alice, pictures[1])
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(left["attachments"], json!([]));
    assert_eq!(
        remove(&alice, pictures[1]).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.count("select count(*) from media_trash where key like 'photos/%'")
            .await,
        2,
        "the files go once the deletes commit"
    );
}
