//! Helpers for the pact tests: requests by name, and reading state straight from the database.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::{Member, TestApp, TestResponse};

pub fn url(pact: i64, suffix: &str) -> String {
    if suffix.is_empty() {
        format!("/api/pacts/{pact}")
    } else {
        format!("/api/pacts/{pact}/{suffix}")
    }
}

/// Create a pact from `body` as `who`; returns its id.
pub async fn create(app: &TestApp, who: &Member, body: Value) -> i64 {
    app.post_as(who, "/api/pacts", body)
        .await
        .expect(StatusCode::CREATED)
        .json()["id"]
        .as_i64()
        .unwrap()
}

/// `host` bets "Witcher 4 w 2027" against each opponent for their stake (grosze).
pub async fn bet(app: &TestApp, host: &Member, opponents: &[(&Member, i64)]) -> i64 {
    bet_with(app, host, opponents, json!({})).await
}

pub async fn bet_with(
    app: &TestApp,
    host: &Member,
    opponents: &[(&Member, i64)],
    extra: Value,
) -> i64 {
    let mut body = json!({
        "kind": "bet",
        "title": "Witcher 4 w 2027",
        "condition": "Wyjdzie przed 2028",
        "opponents": opponents
            .iter()
            .map(|(who, stake)| json!({"user_id": who.id, "stake_amount": stake}))
            .collect::<Vec<_>>(),
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    create(app, host, body).await
}

pub async fn respond(app: &TestApp, who: &Member, pact: i64, body: Value) -> TestResponse {
    app.post_as(who, &url(pact, "respond"), body).await
}

pub async fn accept(app: &TestApp, who: &Member, pact: i64) -> TestResponse {
    respond(app, who, pact, json!({"accept": true})).await
}

pub async fn decline(app: &TestApp, who: &Member, pact: i64) -> TestResponse {
    respond(app, who, pact, json!({"accept": false})).await
}

pub async fn join(app: &TestApp, who: &Member, pact: i64, terms: Value) -> TestResponse {
    app.post_as(who, &url(pact, "join"), terms).await
}

pub async fn decide(
    app: &TestApp,
    who: &Member,
    pact: i64,
    requester: &Member,
    approve: bool,
) -> TestResponse {
    let participant = participant(app, pact, requester).await;
    app.post_as(
        who,
        &url(pact, &format!("participants/{participant}/decide")),
        json!({"approve": approve}),
    )
    .await
}

/// A claim about the whole pact (`wager` `None`) or one wager.
pub async fn claim(
    app: &TestApp,
    who: &Member,
    pact: i64,
    result: Value,
    wager: Option<i64>,
) -> TestResponse {
    app.post_as(
        who,
        &url(pact, "claims"),
        json!({"wager_id": wager, "result": result}),
    )
    .await
}

/// `who` claims `winner` of the wager with `opponent`.
pub async fn propose(
    app: &TestApp,
    who: &Member,
    pact: i64,
    opponent: &Member,
    winner: &str,
) -> TestResponse {
    let wager = participant(app, pact, opponent).await;
    claim(app, who, pact, json!({"winner": winner}), Some(wager)).await
}

pub async fn claim_action(
    app: &TestApp,
    who: &Member,
    pact: i64,
    claim: i64,
    action: &str,
) -> TestResponse {
    app.post_as(
        who,
        &url(pact, &format!("claims/{claim}/{action}")),
        json!({}),
    )
    .await
}

pub async fn confirm(app: &TestApp, who: &Member, pact: i64, claim: i64) -> TestResponse {
    claim_action(app, who, pact, claim, "confirm").await
}

/// `proposer` claims `winner` of `opponent`'s wager and `confirmer` agrees.
pub async fn settle(
    app: &TestApp,
    pact: i64,
    proposer: &Member,
    confirmer: &Member,
    opponent: &Member,
    winner: &str,
) {
    propose(app, proposer, pact, opponent, winner)
        .await
        .expect(StatusCode::CREATED);
    let claim = last_claim(app, pact).await;
    confirm(app, confirmer, pact, claim)
        .await
        .expect(StatusCode::OK);
}

/// The newest claim on `pact`.
pub async fn last_claim(app: &TestApp, pact: i64) -> i64 {
    sqlx::query_scalar("select max(id) from pact_claims where pact_id = $1")
        .bind(pact)
        .fetch_one(app.db())
        .await
        .unwrap()
}

pub async fn claim_state(app: &TestApp, claim: i64) -> String {
    sqlx::query_scalar("select state from pact_claims where id = $1")
        .bind(claim)
        .fetch_one(app.db())
        .await
        .unwrap()
}

/// `who`'s participant id in `pact` (their wager, in a bet).
pub async fn participant(app: &TestApp, pact: i64, who: &Member) -> i64 {
    sqlx::query_scalar("select id from pact_participants where pact_id = $1 and user_id = $2")
        .bind(pact)
        .bind(who.id)
        .fetch_optional(app.db())
        .await
        .unwrap()
        .unwrap_or(0)
}

pub async fn state_of(app: &TestApp, pact: i64, who: &Member) -> String {
    sqlx::query_scalar("select state from pact_participants where pact_id = $1 and user_id = $2")
        .bind(pact)
        .bind(who.id)
        .fetch_one(app.db())
        .await
        .unwrap()
}

pub async fn status(app: &TestApp, pact: i64) -> String {
    sqlx::query_scalar("select status from pacts where id = $1")
        .bind(pact)
        .fetch_one(app.db())
        .await
        .unwrap()
}

/// Every obligation in the ledger as (debtor, creditor, amount).
pub async fn owed(app: &TestApp) -> BTreeSet<(String, String, i64)> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "select d.username, c.username, o.amount from ledger_obligations o
         join users d on d.id = o.debtor_id join users c on c.id = o.creditor_id",
    )
    .fetch_all(app.db())
    .await
    .unwrap();
    rows.into_iter().collect()
}

/// A claim's verdicts as (username, verdict), sorted.
pub async fn verdicts(app: &TestApp, claim: i64) -> Vec<(String, String)> {
    sqlx::query_as(
        "select u.username, v.verdict from pact_verdicts v join users u on u.id = v.user_id
         where v.claim_id = $1 order by u.username",
    )
    .bind(claim)
    .fetch_all(app.db())
    .await
    .unwrap()
}

pub async fn detail(app: &TestApp, who: &Member, pact: i64) -> Value {
    app.get_as(who, &url(pact, ""))
        .await
        .expect(StatusCode::OK)
        .json()
}

pub async fn actions(app: &TestApp, who: &Member, pact: i64) -> Value {
    detail(app, who, pact).await["actions"].clone()
}

/// Move `pact`'s deadline (and its next scheduler visit) an hour into the past.
pub async fn overdue(app: &TestApp, pact: i64) {
    sqlx::query(
        "update pacts set due_at = $2 - interval '1 hour',
                          next_nudge_at = case when next_nudge_at is not null then $2 - interval '1 hour' end
         where id = $1",
    )
    .bind(pact)
    .bind(app.now())
    .execute(app.db())
    .await
    .unwrap();
}

/// A ballot in a pact vote (`{"kept": ..}` or `{"upheld": ..}`).
pub async fn ballot(app: &TestApp, who: &Member, poll: i64, ballot: Value) -> TestResponse {
    app.put_as(who, &format!("/api/polls/{poll}/ballot"), ballot)
        .await
}
