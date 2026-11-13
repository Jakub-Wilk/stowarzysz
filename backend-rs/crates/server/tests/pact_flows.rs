//! Pacts end to end: group bets, joining, calling off, predictions, resolutions judged by the
//! Sejmik, disputed claims put to a vote, deadlines, statistics and shared goals.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;

use app::pacts::deadlines::PactDeadlines;
use app::polls::deadlines::PollDeadlines;
use app::scheduler::Job as _;
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use common::pacts::{
    accept, actions, ballot, bet, bet_with, claim, claim_action, claim_state, confirm, create,
    decide, decline, detail, join, last_claim, overdue, owed, participant, propose, respond,
    settle, state_of, status, url, verdicts,
};
use common::{Member, TestApp, subscribe, take_pushes};
use serde_json::{Value, json};
use sqlx::PgPool;

fn owes(rows: &[(&str, &str, i64)]) -> BTreeSet<(String, String, i64)> {
    rows.iter()
        .map(|(d, c, a)| ((*d).to_owned(), (*c).to_owned(), *a))
        .collect()
}

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

fn later(app: &TestApp, by: TimeDelta) -> String {
    (app.now() + by).to_rfc3339()
}

async fn people<const N: usize>(app: &TestApp, names: [&str; N]) -> [Member; N] {
    let mut members = Vec::with_capacity(N);
    for name in names {
        members.push(app.member(name).await);
    }
    members.try_into().unwrap()
}

/// `who` with a fresh session (after the clock moved past their token's expiry).
async fn signed_in(app: &TestApp, who: &Member) -> Member {
    Member {
        token: app.session(who.id).await.access,
        ..who.clone()
    }
}

async fn run_pact_jobs(app: &TestApp, at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    PactDeadlines.run(&app.ctx, at).await.unwrap()
}

async fn run_poll_jobs(app: &TestApp, at: DateTime<Utc>) {
    PollDeadlines.run(&app.ctx, at).await.unwrap();
}

async fn jurors(app: &TestApp, poll: i64) -> BTreeSet<String> {
    let names: Vec<String> = sqlx::query_scalar(
        "select u.username from poll_participants p join users u on u.id = p.user_id
         where p.poll_id = $1",
    )
    .bind(poll)
    .fetch_all(app.db())
    .await
    .unwrap();
    names.into_iter().collect()
}

// --- group bets -------------------------------------------------------------------------------

/// alice: tak, 20 zł. bob and carol are invited, still to answer.
async fn group(app: &TestApp, alice: &Member, bob: &Member, carol: &Member) -> i64 {
    create(
        app,
        alice,
        json!({
            "kind": "group_bet",
            "title": "Witcher 4 w 2027",
            "condition": "Wyjdzie przed 2028",
            "host": {"side": "tak", "stake_amount": 2000},
            "opponents": [{"user_id": bob.id}, {"user_id": carol.id}],
        }),
    )
    .await
}

async fn pick(app: &TestApp, who: &Member, pact: i64, side: &str, stake: Option<i64>) {
    respond(
        app,
        who,
        pact,
        json!({"accept": true, "side": side, "stake_amount": stake}),
    )
    .await
    .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_group_bet_starts_only_once_everyone_has_answered(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = group(&app, alice, bob, carol).await;
    assert_eq!(
        detail(&app, alice, pact).await["sides"],
        json!(["tak", "nie"])
    );
    pick(&app, bob, pact, "nie", Some(3000)).await;
    assert_eq!(
        status(&app, pact).await,
        "proposed",
        "carol has not answered"
    );
    pick(&app, carol, pact, "nie", Some(5000)).await;
    assert_eq!(status(&app, pact).await, "active");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_group_bet_decline_lets_the_rest_start(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = group(&app, alice, bob, carol).await;
    pick(&app, bob, pact, "nie", Some(3000)).await;
    decline(&app, carol, pact).await.expect(StatusCode::OK);
    assert_eq!(status(&app, pact).await, "active");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn accepting_a_group_bet_needs_a_side_but_a_stake_is_optional(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = group(&app, alice, bob, carol).await;
    let no_side = accept(&app, bob, pact).await;
    assert_eq!(
        (no_side.status, no_side.code()),
        (StatusCode::BAD_REQUEST, "invalid_side".into())
    );
    let maybe = respond(
        &app,
        bob,
        pact,
        json!({"accept": true, "side": "maybe", "stake_amount": 100}),
    )
    .await;
    assert_eq!(maybe.status, StatusCode::BAD_REQUEST);
    assert_eq!(state_of(&app, pact, bob).await, "invited");
    pick(&app, bob, pact, "nie", None).await;
    let view = detail(&app, bob, pact).await;
    assert_eq!(view["participants"][1]["side"], "nie");
    assert_eq!(view["participants"][1]["stake_amount"], Value::Null);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_group_bet_needs_the_hosts_side_and_proper_sides(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let base = json!({"kind": "group_bet", "title": "t", "condition": "c", "opponents": [{"user_id": bob.id}]});
    let no_host_side = app.post_as(alice, "/api/pacts", base.clone()).await;
    assert_eq!(
        (no_host_side.status, no_host_side.code()),
        (StatusCode::BAD_REQUEST, "invalid_side".into())
    );
    let mut draw = base;
    draw["sides"] = json!(["draw", "nie"]);
    draw["host"] = json!({"side": "nie"});
    let reserved = app.post_as(alice, "/api/pacts", draw).await;
    assert_eq!(
        (reserved.status, reserved.code()),
        (StatusCode::BAD_REQUEST, "reserved_side".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_group_bet_pays_the_documented_example_after_everyone_confirms(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = group(&app, alice, bob, carol).await;
    pick(&app, bob, pact, "nie", Some(3000)).await;
    pick(&app, carol, pact, "nie", Some(5000)).await;
    let view = claim(&app, alice, pact, json!({"winner": "nie"}), None)
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(view["claims"][0]["wager_id"], Value::Null);
    let id = last_claim(&app, pact).await;
    confirm(&app, bob, pact, id).await.expect(StatusCode::OK);
    assert_eq!(
        status(&app, pact).await,
        "active",
        "carol still has to agree"
    );
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
    let twice = confirm(&app, bob, pact, id).await;
    assert_eq!(
        (twice.status, twice.code()),
        (StatusCode::CONFLICT, "already_confirmed".into())
    );
    let done = confirm(&app, carol, pact, id)
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(
        owed(&app).await,
        owes(&[("alice", "bob", 750), ("alice", "carol", 1250)])
    );
    assert_eq!(done["status"], "resolved");
    assert_eq!(done["outcome"], json!({"result": {"winner": "nie"}}));
    assert_eq!(
        app.count("select count(*) from ledger_entries").await,
        1,
        "one entry for the whole settlement"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn one_dispute_stops_a_whole_group_claim(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = group(&app, alice, bob, carol).await;
    pick(&app, bob, pact, "nie", Some(3000)).await;
    pick(&app, carol, pact, "nie", Some(5000)).await;
    claim(&app, alice, pact, json!({"winner": "tak"}), None)
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    confirm(&app, bob, pact, id).await.expect(StatusCode::OK);
    claim_action(&app, carol, pact, id, "dispute")
        .await
        .expect(StatusCode::OK);
    assert_eq!(status(&app, pact).await, "active");
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_group_claim_names_a_side_and_comes_from_a_participant(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    let pact = group(&app, alice, bob, carol).await;
    pick(&app, bob, pact, "nie", Some(3000)).await;
    pick(&app, carol, pact, "nie", Some(5000)).await;
    let host = claim(&app, alice, pact, json!({"winner": "host"}), None).await;
    assert_eq!(host.status, StatusCode::BAD_REQUEST);
    let outsider = claim(&app, dave, pact, json!({"winner": "tak"}), None).await;
    assert_eq!(
        outsider.status,
        StatusCode::FORBIDDEN,
        "can read it, not act"
    );
}

// --- joining ----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_bet_join_request_is_approved_by_the_host_alone(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let [ref alice, ref bob, ref carol, ref dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    subscribe(app.db(), &[alice.id, bob.id, carol.id]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    take_pushes(app.db()).await;
    join(&app, carol, pact, json!({"stake_amount": 1500}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(state_of(&app, pact, carol).await, "requested");
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            alice.id,
            "Prośba o dołączenie".into(),
            "carol chce dołączyć do umowy: Witcher 4 w 2027".into()
        )]
    );
    assert_eq!(
        decide(&app, bob, pact, carol, true).await.status,
        StatusCode::FORBIDDEN,
        "not bob's call"
    );
    assert_eq!(
        decide(&app, dave, pact, carol, true).await.status,
        StatusCode::FORBIDDEN
    );
    decide(&app, alice, pact, carol, true)
        .await
        .expect(StatusCode::OK);
    assert_eq!(state_of(&app, pact, carol).await, "active");
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            carol.id,
            "Prośba przyjęta".into(),
            "Przyjęto Cię do umowy: Witcher 4 w 2027".into()
        )]
    );
    assert_eq!(
        decide(&app, alice, pact, carol, true).await.status,
        StatusCode::CONFLICT
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_joined_wager_settles_like_any_other(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    join(&app, carol, pact, json!({"stake_amount": 1500}))
        .await
        .expect(StatusCode::OK);
    decide(&app, alice, pact, carol, true)
        .await
        .expect(StatusCode::OK);
    settle(&app, pact, alice, carol, carol, "opponent").await;
    assert_eq!(owed(&app).await, owes(&[("alice", "carol", 1500)]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_pact_takes_join_requests_but_a_bet_needs_a_stake(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    let no_stake = join(&app, carol, pact, json!({})).await;
    assert_eq!(
        (no_stake.status, no_stake.code()),
        (StatusCode::BAD_REQUEST, "stake_required".into())
    );
    join(&app, carol, pact, json!({"stake_amount": 100}))
        .await
        .expect(StatusCode::OK);
    let again = join(&app, carol, pact, json!({"stake_amount": 100})).await;
    assert_eq!(
        (again.status, again.code()),
        (StatusCode::CONFLICT, "already_in".into())
    );
    let done = bet(&app, alice, &[(bob, 1000)]).await;
    sqlx::query("update pacts set status = 'resolved' where id = $1")
        .bind(done)
        .execute(app.db())
        .await
        .unwrap();
    let late = join(&app, carol, done, json!({"stake_amount": 100})).await;
    assert_eq!(
        (late.status, late.code()),
        (StatusCode::CONFLICT, "joining_closed".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn rejected_requests_can_be_made_again(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    join(&app, carol, pact, json!({"stake_amount": 100}))
        .await
        .expect(StatusCode::OK);
    decide(&app, alice, pact, carol, false)
        .await
        .expect(StatusCode::OK);
    assert_eq!(state_of(&app, pact, carol).await, "rejected");
    join(&app, carol, pact, json!({"stake_amount": 200}))
        .await
        .expect(StatusCode::OK);
    let row = &detail(&app, alice, pact).await["participants"][2];
    assert_eq!(
        (row["state"].clone(), row["stake_amount"].clone()),
        (json!("requested"), json!(200))
    );
    assert_eq!(
        row["approvals"],
        json!([{"approver_id": alice.id, "approved": null}])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn withdrawing_a_request(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    join(&app, carol, pact, json!({"stake_amount": 100}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(actions(&app, carol, pact).await["withdraw"], true);
    app.post_as(carol, &url(pact, "withdraw"), json!({}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(state_of(&app, pact, carol).await, "withdrawn");
    let twice = app.post_as(carol, &url(pact, "withdraw"), json!({})).await;
    assert_eq!(twice.status, StatusCode::CONFLICT);
    let decided = decide(&app, alice, pact, carol, true).await;
    assert_eq!(decided.status, StatusCode::CONFLICT);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_open_pact_starts_with_the_first_approved_joiner(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = create(
        &app,
        alice,
        json!({"kind": "bet", "title": "t", "condition": "c"}),
    )
    .await;
    assert_eq!(status(&app, pact).await, "proposed");
    join(&app, bob, pact, json!({"stake_amount": 500}))
        .await
        .expect(StatusCode::OK);
    decide(&app, alice, pact, bob, true)
        .await
        .expect(StatusCode::OK);
    assert_eq!(status(&app, pact).await, "active");
}

async fn two_sided(app: &TestApp, alice: &Member, bob: &Member) -> i64 {
    let pact = create(
        app,
        alice,
        json!({
            "kind": "group_bet", "title": "t", "condition": "c",
            "host": {"side": "tak", "stake_amount": 1000},
            "opponents": [{"user_id": bob.id}],
        }),
    )
    .await;
    pick(app, bob, pact, "nie", Some(1000)).await;
    pact
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn group_bet_joiners_need_every_active_participant_to_approve(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    let pact = two_sided(&app, alice, bob).await;
    join(
        &app,
        dave,
        pact,
        json!({"side": "nie", "stake_amount": 500}),
    )
    .await
    .expect(StatusCode::OK);
    decide(&app, alice, pact, dave, true)
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        state_of(&app, pact, dave).await,
        "requested",
        "bob has not said yes"
    );
    assert_eq!(
        decide(&app, carol, pact, dave, true).await.status,
        StatusCode::FORBIDDEN
    );
    decide(&app, bob, pact, dave, true)
        .await
        .expect(StatusCode::OK);
    let row = &detail(&app, dave, pact).await["participants"][2];
    assert_eq!(
        (row["state"].clone(), row["side"].clone()),
        (json!("active"), json!("nie"))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn one_no_is_enough_to_reject_a_group_bet_joiner(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref dave] = people(&app, ["alice", "bob", "dave"]).await;
    let pact = two_sided(&app, alice, bob).await;
    join(
        &app,
        dave,
        pact,
        json!({"side": "tak", "stake_amount": 500}),
    )
    .await
    .expect(StatusCode::OK);
    decide(&app, bob, pact, dave, false)
        .await
        .expect(StatusCode::OK);
    assert_eq!(state_of(&app, pact, dave).await, "rejected");
    assert_eq!(
        actions(&app, alice, pact).await["decide"],
        json!([]),
        "nothing left for alice to decide"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn group_bet_membership_is_frozen_while_a_claim_is_open(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref dave] = people(&app, ["alice", "bob", "dave"]).await;
    let pact = two_sided(&app, alice, bob).await;
    claim(&app, alice, pact, json!({"winner": "tak"}), None)
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(actions(&app, dave, pact).await["join"], false);
    let frozen = join(
        &app,
        dave,
        pact,
        json!({"side": "tak", "stake_amount": 100}),
    )
    .await;
    assert_eq!(
        (frozen.status, frozen.code()),
        (StatusCode::CONFLICT, "claim_in_flight".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_detail_tells_the_caller_what_they_can_do(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet_with(
        &app,
        alice,
        &[(bob, 1000)],
        json!({"due_at": later(&app, TimeDelta::days(30))}),
    )
    .await;
    assert_eq!(actions(&app, bob, pact).await["respond"], true);
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    assert_eq!(actions(&app, carol, pact).await["join"], true);
    join(&app, carol, pact, json!({"stake_amount": 100}))
        .await
        .expect(StatusCode::OK);
    let carol_row = participant(&app, pact, carol).await;
    let bob_row = participant(&app, pact, bob).await;
    let host = actions(&app, alice, pact).await;
    assert_eq!(host["decide"], json!([carol_row]));
    assert_eq!(host["wagers"], json!([bob_row]));
    assert_eq!(
        (host["claim"].clone(), host["void"].clone()),
        (json!(false), json!(true)),
        "before the deadline a wager can only be called off"
    );
    assert_eq!(actions(&app, carol, pact).await["withdraw"], true);
    claim(&app, bob, pact, json!({"void": true}), Some(bob_row))
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    assert_eq!(actions(&app, alice, pact).await["confirm"], json!([id]));
    assert_eq!(actions(&app, bob, pact).await["confirm"], json!([]));
    let too_early = propose(&app, alice, pact, bob, "host").await;
    assert_eq!(
        (too_early.status, too_early.code()),
        (StatusCode::CONFLICT, "too_early".into())
    );
}

// --- calling off and predictions --------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_wager_can_be_called_off_by_mutual_agreement(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet(&app, alice, &[(bob, 1000), (carol, 1500)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    accept(&app, carol, pact).await.expect(StatusCode::OK);
    assert_eq!(
        propose(&app, bob, pact, bob, "x").await.status,
        StatusCode::BAD_REQUEST
    );
    let wager = participant(&app, pact, bob).await;
    claim(&app, bob, pact, json!({"void": true}), Some(wager))
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    confirm(&app, alice, pact, id).await.expect(StatusCode::OK);
    assert_eq!(state_of(&app, pact, bob).await, "void");
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
    assert_eq!(status(&app, pact).await, "active", "carol's wager stands");
    assert_eq!(
        verdicts(&app, id).await,
        pairs(&[("alice", "void"), ("bob", "void")])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn calling_off_every_wager_voids_the_pact(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    let wager = participant(&app, pact, bob).await;
    claim(&app, alice, pact, json!({"void": true}), Some(wager))
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    let view = confirm(&app, bob, pact, id)
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(view["status"], "void");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_group_bet_without_stakes_is_a_prediction(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = create(
        &app,
        alice,
        json!({
            "kind": "group_bet", "title": "Polska na mundialu", "condition": "Wyjdzie z grupy",
            "host": {"side": "tak"},
            "opponents": [{"user_id": bob.id}, {"user_id": carol.id}],
        }),
    )
    .await;
    assert_eq!(
        accept(&app, bob, pact).await.status,
        StatusCode::BAD_REQUEST
    );
    pick(&app, bob, pact, "nie", None).await;
    pick(&app, carol, pact, "tak", None).await;
    claim(&app, bob, pact, json!({"winner": "tak"}), None)
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    confirm(&app, alice, pact, id).await.expect(StatusCode::OK);
    confirm(&app, carol, pact, id).await.expect(StatusCode::OK);
    assert_eq!(
        verdicts(&app, id).await,
        pairs(&[("alice", "won"), ("bob", "lost"), ("carol", "won")])
    );
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
    assert_eq!(status(&app, pact).await, "resolved");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn deadlines_are_in_the_future_and_some_kinds_need_one(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    for kind in ["resolution", "shared_goal"] {
        let response = app
            .post_as(
                alice,
                "/api/pacts",
                json!({"kind": kind, "title": "t", "condition": "c"}),
            )
            .await;
        assert_eq!(
            (response.status, response.code()),
            (StatusCode::BAD_REQUEST, "due_required".into()),
            "{kind}"
        );
    }
    let past = app
        .post_as(
            alice,
            "/api/pacts",
            json!({
                "kind": "bet", "title": "t", "condition": "c",
                "due_at": later(&app, TimeDelta::days(-1)),
                "opponents": [{"user_id": bob.id, "stake_amount": 100}],
            }),
        )
        .await;
    assert_eq!(
        (past.status, past.code()),
        (StatusCode::BAD_REQUEST, "due_in_past".into())
    );
}

// --- resolutions ------------------------------------------------------------------------------

async fn resolution(app: &TestApp, who: &Member) -> i64 {
    create(
        app,
        who,
        json!({
            "kind": "resolution",
            "title": "Rzucę palenie",
            "condition": "Do końca roku bez papierosa",
            "due_at": later(app, TimeDelta::days(90)),
        }),
    )
    .await
}

async fn judgment(app: &TestApp, who: &Member, pact: i64) -> common::TestResponse {
    app.post_as(who, &url(pact, "judgment"), json!({})).await
}

/// The judging vote running now.
async fn judgment_poll(app: &TestApp, pact: i64) -> i64 {
    sqlx::query_scalar("select judgment_poll_id from pacts where id = $1")
        .bind(pact)
        .fetch_one(app.db())
        .await
        .unwrap()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_resolution_starts_at_once_and_invites_nobody(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = resolution(&app, alice).await;
    let view = detail(&app, bob, pact).await;
    assert_eq!(view["status"], "active");
    assert_eq!(view["participants"].as_array().unwrap().len(), 1);
    let invited = app
        .post_as(
            alice,
            "/api/pacts",
            json!({
                "kind": "resolution", "title": "t", "condition": "c",
                "due_at": later(&app, TimeDelta::days(9)),
                "opponents": [{"user_id": bob.id}],
            }),
        )
        .await;
    assert_eq!(
        (invited.status, invited.code()),
        (StatusCode::BAD_REQUEST, "no_invitees".into())
    );
    let joining = join(&app, bob, pact, json!({})).await;
    assert_eq!(
        (joining.status, joining.code()),
        (StatusCode::CONFLICT, "not_joinable".into())
    );
    assert_eq!(view["actions"]["join"], false);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_resolution_is_judged_by_every_other_member(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    let pact = resolution(&app, alice).await;
    let early = judgment(&app, bob, pact).await;
    assert_eq!(
        (early.status, early.code()),
        (StatusCode::FORBIDDEN, "too_early".into()),
        "only the author may call it early"
    );
    assert_eq!(actions(&app, bob, pact).await["judgment"], false);
    assert_eq!(actions(&app, alice, pact).await["judgment"], true);
    overdue(&app, pact).await;
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
    let poll_id = judgment_poll(&app, pact).await;
    let poll = app
        .get_as(bob, &format!("/api/polls/{poll_id}"))
        .await
        .json();
    assert_eq!(poll["kind"], "resolution_judgment");
    assert_eq!(poll["title"], "Ocena postanowienia: Rzucę palenie");
    assert_eq!(
        jurors(&app, poll_id).await,
        BTreeSet::from(["bob".into(), "carol".into(), "dave".into()])
    );
    let again = judgment(&app, alice, pact).await;
    assert_eq!(
        (again.status, again.code()),
        (StatusCode::CONFLICT, "judgment_running".into())
    );
    let manual = claim(&app, alice, pact, json!({"kept": true}), None).await;
    assert_eq!(
        (manual.status, manual.code()),
        (StatusCode::CONFLICT, "judged_by_vote".into())
    );
    assert_eq!(
        ballot(&app, alice, poll_id, json!({"kept": true}))
            .await
            .status,
        StatusCode::FORBIDDEN,
        "the author does not judge"
    );
    for (juror, kept) in [(bob, true), (carol, true), (dave, false)] {
        ballot(&app, juror, poll_id, json!({"kept": kept}))
            .await
            .expect(StatusCode::OK);
    }
    let closed = app
        .get_as(bob, &format!("/api/polls/{poll_id}"))
        .await
        .json();
    assert_eq!(
        (
            closed["result"]["approved"].clone(),
            closed["result"]["applied"].clone()
        ),
        (json!(true), json!(true))
    );
    let view = detail(&app, bob, pact).await;
    assert_eq!(view["status"], "resolved");
    assert_eq!(view["judgment_poll_id"], Value::Null);
    assert_eq!(view["outcome"], json!({"result": {"kept": true}}));
    let id = last_claim(&app, pact).await;
    assert_eq!(verdicts(&app, id).await, pairs(&[("alice", "won")]));
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_broken_resolution_counts_as_lost(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = resolution(&app, alice).await;
    overdue(&app, pact).await;
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
    let poll = judgment_poll(&app, pact).await;
    for juror in [bob, carol] {
        ballot(&app, juror, poll, json!({"kept": false}))
            .await
            .expect(StatusCode::OK);
    }
    let id = last_claim(&app, pact).await;
    assert_eq!(verdicts(&app, id).await, pairs(&[("alice", "lost")]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_tied_or_empty_judgment_leaves_the_resolution_open(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref _dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    let pact = resolution(&app, alice).await;
    overdue(&app, pact).await;
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
    let poll = judgment_poll(&app, pact).await;
    ballot(&app, bob, poll, json!({"kept": true}))
        .await
        .expect(StatusCode::OK);
    ballot(&app, carol, poll, json!({"kept": false}))
        .await
        .expect(StatusCode::OK);
    run_poll_jobs(&app, app.now() + TimeDelta::days(4)).await; // dave never votes
    let view = detail(&app, alice, pact).await;
    assert_eq!(view["status"], "active");
    assert_eq!(view["judgment_poll_id"], Value::Null);
    assert_eq!(app.count("select count(*) from pact_claims").await, 0);
    let poll_view = app
        .get_as(alice, &format!("/api/polls/{poll}"))
        .await
        .json();
    assert_eq!(poll_view["result"]["applied"], false);
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_author_can_claim_a_resolution_early(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = resolution(&app, alice).await;
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
    let poll = judgment_poll(&app, pact).await;
    for juror in [bob, carol] {
        ballot(&app, juror, poll, json!({"kept": true}))
            .await
            .expect(StatusCode::OK);
    }
    assert_eq!(status(&app, pact).await, "resolved");
    let id = last_claim(&app, pact).await;
    assert_eq!(verdicts(&app, id).await, pairs(&[("alice", "won")]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_early_judgment_that_fails_leaves_the_resolution_open(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = resolution(&app, alice).await;
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
    let poll = judgment_poll(&app, pact).await;
    for juror in [bob, carol] {
        ballot(&app, juror, poll, json!({"kept": false}))
            .await
            .expect(StatusCode::OK);
    }
    let view = detail(&app, alice, pact).await;
    assert_eq!(
        view["status"], "active",
        "not lost: the deadline is still ahead"
    );
    assert_eq!(view["judgment_poll_id"], Value::Null);
    assert_eq!(app.count("select count(*) from pact_claims").await, 0);
    judgment(&app, alice, pact).await.expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn anyone_can_call_for_judgment_of_an_overdue_resolution(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = resolution(&app, alice).await;
    overdue(&app, pact).await;
    run_pact_jobs(&app, app.now()).await;
    assert_eq!(status(&app, pact).await, "awaiting_result");
    assert_eq!(actions(&app, bob, pact).await["judgment"], true);
    judgment(&app, bob, pact).await.expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_resolutions_are_judged_this_way(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    let response = judgment(&app, alice, pact).await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::CONFLICT, "not_judged".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn pact_votes_cannot_be_called_through_the_polls_api(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice] = people(&app, ["alice"]).await;
    for kind in ["pact_ruling", "resolution_judgment"] {
        let response = app
            .post_as(alice, "/api/polls", json!({"kind": kind, "pact_id": 1}))
            .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{kind}");
    }
}

// --- disputes put to the Sejmik ---------------------------------------------------------------

/// Alice bets bob 10 zł, claims she won, bob disputes it. Returns (pact, claim).
async fn disputed(app: &TestApp, alice: &Member, bob: &Member) -> (i64, i64) {
    let pact = bet(app, alice, &[(bob, 1000)]).await;
    accept(app, bob, pact).await.expect(StatusCode::OK);
    propose(app, alice, pact, bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(app, pact).await;
    claim_action(app, bob, pact, id, "dispute")
        .await
        .expect(StatusCode::OK);
    (pact, id)
}

async fn escalate(app: &TestApp, who: &Member, pact: i64, id: i64) -> common::TestResponse {
    claim_action(app, who, pact, id, "escalate").await
}

async fn ruling_poll(app: &TestApp, pact: i64, who: &Member) -> i64 {
    detail(app, who, pact).await["claims"][0]["ruling_poll_id"]
        .as_i64()
        .unwrap()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn escalating_creates_a_poll_for_members_outside_the_pact(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref _dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    let (pact, id) = disputed(&app, alice, bob).await;
    assert_eq!(
        escalate(&app, carol, pact, id).await.status,
        StatusCode::FORBIDDEN,
        "only the parties can"
    );
    assert_eq!(actions(&app, alice, pact).await["escalate"], json!([id]));
    assert_eq!(actions(&app, carol, pact).await["escalate"], json!([]));
    escalate(&app, alice, pact, id).await.expect(StatusCode::OK);
    let poll_id = ruling_poll(&app, pact, alice).await;
    let poll = app
        .get_as(carol, &format!("/api/polls/{poll_id}"))
        .await
        .json();
    assert_eq!(poll["kind"], "pact_ruling");
    assert_eq!(poll["title"], "Spór o umowę: Witcher 4 w 2027");
    assert_eq!(poll["config"]["claim"], json!({"winner": "host"}));
    assert_eq!(
        jurors(&app, poll_id).await,
        BTreeSet::from(["carol".into(), "dave".into()])
    );
    let twice = escalate(&app, alice, pact, id).await;
    assert_eq!(
        (twice.status, twice.code()),
        (StatusCode::CONFLICT, "not_disputed".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn escalation_needs_impartial_voters(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let (pact, id) = disputed(&app, alice, bob).await;
    let response = escalate(&app, alice, pact, id).await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::BAD_REQUEST, "no_jurors".into())
    );
    assert_eq!(app.count("select count(*) from polls").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_claim_must_be_disputed_before_it_goes_to_a_vote(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref _carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    propose(&app, alice, pact, bob, "host")
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    assert_eq!(
        escalate(&app, alice, pact, id).await.status,
        StatusCode::CONFLICT
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_tied_ruling_overrules_the_claim(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref dave] =
        people(&app, ["alice", "bob", "carol", "dave"]).await;
    let (pact, id) = disputed(&app, alice, bob).await;
    escalate(&app, alice, pact, id).await.expect(StatusCode::OK);
    let poll = ruling_poll(&app, pact, alice).await;
    assert_eq!(
        ballot(&app, alice, poll, json!({"upheld": true}))
            .await
            .status,
        StatusCode::FORBIDDEN,
        "the parties do not vote"
    );
    ballot(&app, carol, poll, json!({"upheld": true}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(owed(&app).await, owes(&[]), "one juror still to go");
    ballot(&app, dave, poll, json!({"upheld": false}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(claim_state(&app, id).await, "overruled");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_majority_upholds_and_money_moves(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref dave, ref erin] =
        people(&app, ["alice", "bob", "carol", "dave", "erin"]).await;
    let (pact, id) = disputed(&app, alice, bob).await;
    escalate(&app, bob, pact, id).await.expect(StatusCode::OK);
    let poll = ruling_poll(&app, pact, alice).await;
    let blocked = propose(&app, alice, pact, bob, "host").await;
    assert_eq!(
        (blocked.status, blocked.code()),
        (StatusCode::CONFLICT, "ruling_open".into()),
        "no new claim while the vote runs"
    );
    for (juror, upheld) in [(carol, true), (dave, true), (erin, false)] {
        ballot(&app, juror, poll, json!({"upheld": upheld}))
            .await
            .expect(StatusCode::OK);
    }
    let result = app
        .get_as(carol, &format!("/api/polls/{poll}"))
        .await
        .json();
    assert_eq!(result["status"], "closed");
    assert_eq!(
        (
            result["result"]["approved"].clone(),
            result["result"]["applied"].clone()
        ),
        (json!(true), json!(true))
    );
    assert_eq!(owed(&app).await, owes(&[("bob", "alice", 1000)]));
    assert_eq!(claim_state(&app, id).await, "confirmed");
    assert_eq!(status(&app, pact).await, "resolved");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_overruled_claim_leaves_the_wager_open_for_a_new_claim(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol, ref dave, ref erin] =
        people(&app, ["alice", "bob", "carol", "dave", "erin"]).await;
    let (pact, id) = disputed(&app, alice, bob).await;
    escalate(&app, alice, pact, id).await.expect(StatusCode::OK);
    let poll = ruling_poll(&app, pact, alice).await;
    for juror in [carol, dave, erin] {
        ballot(&app, juror, poll, json!({"upheld": false}))
            .await
            .expect(StatusCode::OK);
    }
    assert_eq!(claim_state(&app, id).await, "overruled");
    assert_eq!(state_of(&app, pact, bob).await, "active");
    propose(&app, bob, pact, bob, "opponent")
        .await
        .expect(StatusCode::CREATED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_expired_ruling_with_no_votes_overrules_the_claim(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref _carol] = people(&app, ["alice", "bob", "carol"]).await;
    let (pact, id) = disputed(&app, alice, bob).await;
    escalate(&app, alice, pact, id).await.expect(StatusCode::OK);
    run_poll_jobs(&app, app.now() + TimeDelta::days(4)).await;
    assert_eq!(claim_state(&app, id).await, "overruled");
}

// --- deadlines --------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn overdue_pacts_are_moved_and_nudged_weekly(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    subscribe(app.db(), &[alice.id, bob.id]).await;
    let pact = bet_with(
        &app,
        alice,
        &[(bob, 1000)],
        json!({"due_at": later(&app, TimeDelta::days(1))}),
    )
    .await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    take_pushes(app.db()).await;
    let due = app.now() + TimeDelta::days(1);
    assert_eq!(
        run_pact_jobs(&app, app.now()).await,
        Some(due),
        "next: the deadline"
    );
    let now = due + TimeDelta::hours(1);
    app.clock.set(now);
    // A day later the access tokens have expired.
    let (alice, bob) = (&signed_in(&app, alice).await, &signed_in(&app, bob).await);
    assert_eq!(
        run_pact_jobs(&app, now).await,
        Some(now + TimeDelta::days(7)),
        "next: a week's nudge"
    );
    assert_eq!(status(&app, pact).await, "awaiting_result");
    let nudge = |who: &Member| {
        (
            who.id,
            "Termin umowy minął".to_owned(),
            "„Witcher 4 w 2027”: ustalcie wynik".to_owned(),
        )
    };
    assert_eq!(take_pushes(app.db()).await, [nudge(alice), nudge(bob)]);
    run_pact_jobs(&app, now + TimeDelta::days(3)).await;
    assert_eq!(take_pushes(app.db()).await, Vec::new());
    run_pact_jobs(&app, now + TimeDelta::days(8)).await;
    assert_eq!(take_pushes(app.db()).await.len(), 2);
    // An overdue pact can still be settled.
    assert_eq!(actions(&app, alice, pact).await["claim"], true);
    settle(&app, pact, alice, bob, bob, "host").await;
    assert_eq!(status(&app, pact).await, "resolved");
    take_pushes(app.db()).await;
    assert_eq!(run_pact_jobs(&app, now + TimeDelta::days(30)).await, None);
    assert_eq!(take_pushes(app.db()).await, Vec::new());
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn unanswered_invites_expire(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref dave] = people(&app, ["alice", "bob", "dave"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    let expiry = app.now() + TimeDelta::days(7);
    assert_eq!(run_pact_jobs(&app, app.now()).await, Some(expiry));
    assert_eq!(state_of(&app, pact, bob).await, "invited");
    run_pact_jobs(&app, expiry).await;
    assert_eq!(state_of(&app, pact, bob).await, "expired");
    assert_eq!(
        status(&app, pact).await,
        "proposed",
        "still open to joiners"
    );
    assert_eq!(actions(&app, dave, pact).await["join"], true);
    assert_eq!(
        actions(&app, bob, pact).await["join"],
        true,
        "and bob may ask"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn unanswered_join_requests_expire_too(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = two_sided(&app, alice, bob).await;
    join(&app, carol, pact, json!({"side": "tak"}))
        .await
        .expect(StatusCode::OK);
    run_pact_jobs(&app, app.now() + TimeDelta::days(7)).await;
    assert_eq!(state_of(&app, pact, carol).await, "expired");
    assert_eq!(
        app.count("select count(*) from pact_join_consents").await,
        0,
        "nobody is asked to decide any more"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn expiring_a_straggler_starts_a_group_bet(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = group(&app, alice, bob, carol).await;
    pick(&app, bob, pact, "nie", Some(3000)).await; // carol never answers
    run_pact_jobs(&app, app.now() + TimeDelta::days(8)).await;
    assert_eq!(status(&app, pact).await, "active");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_pact_that_never_started_lapses_at_its_deadline(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = bet_with(
        &app,
        alice,
        &[(bob, 1000)],
        json!({"due_at": later(&app, TimeDelta::days(1))}),
    )
    .await;
    run_pact_jobs(&app, app.now() + TimeDelta::days(2)).await;
    assert_eq!(status(&app, pact).await, "cancelled");
    assert_eq!(
        state_of(&app, pact, bob).await,
        "withdrawn",
        "its invite goes with it"
    );
    assert_eq!(
        run_pact_jobs(&app, app.now() + TimeDelta::days(3)).await,
        None
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_scheduler_runs_the_pact_job(pool: PgPool) {
    let _ = pool;
    let names = app::scheduler::Scheduler::standard().job_names();
    assert!(names.contains(&"pact-deadlines"), "{names:?}");
}

// --- statistics -------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn stats_count_settled_claims_and_money(pool: PgPool) {
    let app = TestApp::new(pool);
    let boss = app.admin("boss").await;
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let first = bet(&app, alice, &[(bob, 1000), (carol, 1500)]).await;
    accept(&app, bob, first).await.expect(StatusCode::OK);
    accept(&app, carol, first).await.expect(StatusCode::OK);
    settle(&app, first, alice, bob, bob, "host").await;
    settle(&app, first, alice, carol, carol, "opponent").await;
    let open = bet(&app, alice, &[(bob, 500)]).await;
    accept(&app, bob, open).await.expect(StatusCode::OK);
    propose(&app, alice, open, bob, "host")
        .await
        .expect(StatusCode::CREATED); // unconfirmed: does not count

    let rows: Vec<Value> = app
        .get_as(&boss, "/api/users")
        .await
        .json()
        .as_array()
        .unwrap()
        .clone();
    let record = |name: &str| rows.iter().find(|r| r["username"] == name).unwrap()["pacts"].clone();
    let alice_record = record("alice");
    assert_eq!(
        (alice_record["won"].clone(), alice_record["lost"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(
        (record("bob")["won"].clone(), record("bob")["lost"].clone()),
        (json!(0), json!(1))
    );
    assert_eq!(
        (
            record("carol")["won"].clone(),
            record("carol")["lost"].clone()
        ),
        (json!(1), json!(0))
    );
    assert_eq!(
        (
            alice_record["money_won"].clone(),
            alice_record["money_lost"].clone()
        ),
        (json!(1000), json!(1500))
    );
    assert_eq!(record("carol")["money_won"], 1500);
    assert_eq!(
        alice_record["by_kind"],
        json!({"bet": {"won": 1, "lost": 1, "draw": 0}})
    );
    let one = app
        .get_as(&boss, &format!("/api/users/{}", alice.id))
        .await
        .json();
    assert_eq!(one["pacts"], alice_record);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_member_with_no_settled_pacts_has_an_empty_record(pool: PgPool) {
    let app = TestApp::new(pool);
    let boss = app.admin("boss").await;
    let [ref alice, ref bob, ref dave] = people(&app, ["alice", "bob", "dave"]).await;
    let pact = bet(&app, alice, &[(bob, 1000)]).await;
    accept(&app, bob, pact).await.expect(StatusCode::OK);
    let record = app
        .get_as(&boss, &format!("/api/users/{}", dave.id))
        .await
        .json()["pacts"]
        .clone();
    assert_eq!(
        record,
        json!({"won": 0, "lost": 0, "draw": 0, "money_won": 0, "money_lost": 0, "by_kind": {}})
    );
}

// --- shared goals -----------------------------------------------------------------------------

async fn goal(app: &TestApp, author: &Member, invitees: &[&Member], years: i64) -> i64 {
    let pact = create(
        app,
        author,
        json!({
            "kind": "shared_goal",
            "title": "Wyjazd do Ameryki",
            "condition": "Wszyscy razem w USA",
            "due_at": later(app, TimeDelta::days(365 * years)),
            "opponents": invitees.iter().map(|u| json!({"user_id": u.id})).collect::<Vec<_>>(),
        }),
    )
    .await;
    for who in invitees {
        accept(app, who, pact).await.expect(StatusCode::OK);
    }
    pact
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_shared_goal_has_no_stakes(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let response = app
        .post_as(
            alice,
            "/api/pacts",
            json!({
                "kind": "shared_goal", "title": "t", "condition": "c",
                "due_at": later(&app, TimeDelta::days(9)),
                "opponents": [{"user_id": bob.id, "stake_amount": 100}],
            }),
        )
        .await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::BAD_REQUEST, "no_stakes".into())
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_shared_goal_can_be_completed_early_when_all_confirm(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = goal(&app, alice, &[bob, carol], 10).await;
    assert_eq!(status(&app, pact).await, "active");
    assert_eq!(actions(&app, bob, pact).await["claim"], true);
    claim(&app, bob, pact, json!({"done": true}), None)
        .await
        .expect(StatusCode::CREATED);
    let id = last_claim(&app, pact).await;
    confirm(&app, alice, pact, id).await.expect(StatusCode::OK);
    assert_eq!(
        status(&app, pact).await,
        "active",
        "carol has not confirmed"
    );
    confirm(&app, carol, pact, id).await.expect(StatusCode::OK);
    assert_eq!(status(&app, pact).await, "resolved");
    assert_eq!(
        verdicts(&app, id).await,
        pairs(&[("alice", "won"), ("bob", "won"), ("carol", "won")])
    );
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_shared_goal_cannot_be_failed_before_the_deadline(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob] = people(&app, ["alice", "bob"]).await;
    let pact = goal(&app, alice, &[bob], 10).await;
    let failed = claim(&app, bob, pact, json!({"done": false}), None).await;
    assert_eq!(
        (failed.status, failed.code()),
        (StatusCode::CONFLICT, "too_early".into())
    );
    assert_eq!(
        claim(&app, bob, pact, json!({"done": "yes"}), None)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    claim(&app, bob, pact, json!({"void": true}), None)
        .await
        .expect(StatusCode::CREATED);
    overdue(&app, pact).await;
    claim(&app, bob, pact, json!({"done": false}), None)
        .await
        .expect(StatusCode::CREATED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_participants_claim_a_shared_goal_and_joiners_need_everyone(pool: PgPool) {
    let app = TestApp::new(pool);
    let [ref alice, ref bob, ref carol] = people(&app, ["alice", "bob", "carol"]).await;
    let pact = goal(&app, alice, &[bob], 10).await;
    assert_eq!(
        claim(&app, carol, pact, json!({"done": true}), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    join(&app, carol, pact, json!({}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(actions(&app, carol, pact).await["withdraw"], true);
    let carol_row = participant(&app, pact, carol).await;
    assert_eq!(
        actions(&app, alice, pact).await["decide"],
        json!([carol_row])
    );
    assert_eq!(actions(&app, bob, pact).await["decide"], json!([carol_row]));
}
