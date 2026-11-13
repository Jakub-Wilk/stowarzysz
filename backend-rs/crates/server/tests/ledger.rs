//! Kasa Skarbowa: expenses, incomes, debts, payments, the Bilans, the feed, notifications and
//! attachments (the checklist of the Python `test_ledger.py`).
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use chrono::TimeDelta;
use common::{
    Member, TestApp, TestResponse, insert_pact, png, rates_stub, record_debts, subscribe,
    take_pushes,
};
use domain::ledger::money::Debt;
use serde_json::{Value, json};
use sqlx::PgPool;

const ENTRIES: &str = "/api/ledger/entries";

/// An item shared equally by `people`.
fn item(name: &str, amount: i64, people: &[&Member]) -> Value {
    json!({"name": name, "amount": amount, "shares": people.iter().map(|m| json!({"user_id": m.id})).collect::<Vec<_>>()})
}

/// An item split by `split` with a weight (or amount) per person.
fn split_item(name: &str, amount: i64, split: &str, people: &[(&Member, i64)]) -> Value {
    json!({"name": name, "amount": amount, "split": split,
           "shares": people.iter().map(|(m, w)| json!({"user_id": m.id, "weight": w})).collect::<Vec<_>>()})
}

fn merge(mut body: Value, extra: Value) -> Value {
    if let (Some(body), Value::Object(extra)) = (body.as_object_mut(), extra) {
        body.extend(extra);
    }
    body
}

async fn add(
    app: &TestApp,
    kind: &str,
    by: &Member,
    items: Vec<Value>,
    payers: &[(&Member, i64)],
    extra: Value,
) -> TestResponse {
    let total: i64 = items.iter().map(|i| i["amount"].as_i64().unwrap()).sum();
    let payers: Vec<Value> = if payers.is_empty() {
        vec![json!({"user_id": by.id, "amount": total})]
    } else {
        payers
            .iter()
            .map(|(m, a)| json!({"user_id": m.id, "amount": a}))
            .collect()
    };
    let body = json!({"kind": kind, "title": "Pizza", "items": items, "payers": payers});
    app.post_as(by, ENTRIES, merge(body, extra)).await
}

async fn expense(app: &TestApp, by: &Member, items: Vec<Value>, extra: Value) -> Value {
    add(app, "expense", by, items, &[], extra)
        .await
        .expect(StatusCode::CREATED)
        .json()
}

async fn pay(app: &TestApp, by: &Member, to: &Member, amount: i64, extra: Value) -> TestResponse {
    let body = json!({"kind": "payment", "to_user_id": to.id, "amount": amount});
    app.post_as(by, ENTRIES, merge(body, extra)).await
}

async fn act(app: &TestApp, who: &Member, id: &Value, action: &str) -> TestResponse {
    app.post_as(who, &format!("{ENTRIES}/{id}/{action}"), json!({}))
        .await
}

async fn bilans(app: &TestApp, who: &Member) -> Value {
    app.get_as(who, "/api/ledger/balances")
        .await
        .expect(StatusCode::OK)
        .json()
}

fn nets(body: &Value, item: &str) -> BTreeMap<String, i64> {
    body["members"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["item"] == item)
        .map(|m| {
            (
                m["user"]["username"].as_str().unwrap().to_owned(),
                m["net"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn money_nets(body: &Value) -> BTreeMap<String, i64> {
    nets(body, "")
}

fn transfers(body: &Value) -> Vec<(String, String, String, i64)> {
    body["settlements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["debtor"]["username"].as_str().unwrap().to_owned(),
                s["creditor"]["username"].as_str().unwrap().to_owned(),
                s["item"].as_str().unwrap().to_owned(),
                s["amount"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn t(debtor: &str, creditor: &str, item: &str, amount: i64) -> (String, String, String, i64) {
    (debtor.into(), creditor.into(), item.into(), amount)
}

fn named(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
    pairs.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect()
}

fn owes(debtor: &Member, creditor: &Member, amount: i64, item: &str) -> Debt {
    Debt {
        debtor_id: debtor.id,
        creditor_id: creditor.id,
        amount,
        item: item.into(),
    }
}

/// A settled pact owing `debts`; returns the entry id.
async fn pact_debts(app: &TestApp, debts: Vec<Debt>) -> Value {
    let pact = insert_pact(app.db(), debts[0].creditor_id).await;
    json!(record_debts(app, pact, debts).await.unwrap())
}

async fn trio(app: &TestApp) -> (Member, Member, Member) {
    (
        app.member("alice").await,
        app.member("bob").await,
        app.member("carol").await,
    )
}

// --- auth and meta ---------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_endpoint_requires_auth(pool: PgPool) {
    let app = TestApp::new(pool);
    let calls = [
        (Method::GET, ENTRIES.to_owned()),
        (Method::POST, ENTRIES.to_owned()),
        (Method::GET, format!("{ENTRIES}/1")),
        (Method::PUT, format!("{ENTRIES}/1")),
        (Method::POST, format!("{ENTRIES}/1/confirm")),
        (Method::POST, format!("{ENTRIES}/1/reject")),
        (Method::POST, format!("{ENTRIES}/1/cancel")),
        (Method::POST, format!("{ENTRIES}/1/attachments")),
        (Method::DELETE, format!("{ENTRIES}/1/attachments/1")),
        (Method::GET, "/api/ledger/balances".to_owned()),
        (Method::GET, "/api/ledger/rates?currency=EUR".to_owned()),
        (Method::GET, "/api/ledger/stats".to_owned()),
        (Method::GET, "/api/ledger/meta".to_owned()),
    ];
    for (method, uri) in calls {
        let response = app.call(method.clone(), &uri, None, Some(json!({}))).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn meta_lists_currencies_with_their_digits_and_polish_categories(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.member("alice").await;
    let body = app.get_as(&alice, "/api/ledger/meta").await.json();
    assert_eq!(body["base_currency"], "PLN");
    assert_eq!(body["currencies"][0]["code"], "PLN");
    assert_eq!(body["currencies"][1]["code"], "EUR");
    assert!(
        body["currencies"]
            .as_array()
            .unwrap()
            .contains(&json!({"code": "JPY", "exponent": 0}))
    );
    assert!(
        body["categories"]
            .as_array()
            .unwrap()
            .contains(&json!({"key": "food", "label": "Restauracje i bary", "emoji": "🍽️"}))
    );
}

// --- expenses ----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_plain_expense_shares_the_cost_and_explains_itself(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let body = expense(
        &app,
        &alice,
        vec![item("Pizza", 9000, &[&alice, &bob, &carol])],
        json!({"category": "food"}),
    )
    .await;
    assert_eq!(
        (
            &body["kind"],
            &body["status"],
            &body["amount"],
            &body["base_amount"]
        ),
        (
            &json!("expense"),
            &json!("confirmed"),
            &json!(9000),
            &json!(9000)
        )
    );
    let today = app::clock::local_today(app.now()).to_string();
    assert_eq!(
        (&body["category"], body["occurred_on"].as_str().unwrap()),
        (&json!("food"), today.as_str())
    );
    let rows: BTreeMap<String, (i64, i64)> = body["breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["user"]["username"].as_str().unwrap().to_owned(),
                (r["paid"].as_i64().unwrap(), r["owed"].as_i64().unwrap()),
            )
        })
        .collect();
    assert_eq!(
        rows,
        BTreeMap::from([
            ("alice".to_owned(), (9000, 3000)),
            ("bob".to_owned(), (0, 3000)),
            ("carol".to_owned(), (0, 3000))
        ])
    );
    assert_eq!(
        body["breakdown"][0]["user"]["username"], "alice",
        "biggest payer first"
    );
    let balance = bilans(&app, &carol).await;
    assert_eq!(
        money_nets(&balance),
        named(&[("alice", 6000), ("bob", -3000), ("carol", -3000)])
    );
    assert_eq!(
        transfers(&balance),
        [t("bob", "alice", "", 3000), t("carol", "alice", "", 3000)]
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_itemized_receipt_charges_each_item_to_its_own_people(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let body = expense(
        &app,
        &alice,
        vec![
            item("Piwo", 1200, &[&bob]),
            split_item(
                "Frytki",
                1000,
                "shares",
                &[(&alice, 2), (&bob, 1), (&carol, 1)],
            ),
            item("Napiwek", 301, &[&alice, &bob, &carol]),
            split_item("Wino", 3000, "exact", &[(&alice, 1000), (&carol, 2000)]),
        ],
        json!({}),
    )
    .await;
    let owed: BTreeMap<String, i64> = body["breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["user"]["username"].as_str().unwrap().to_owned(),
                r["owed"].as_i64().unwrap(),
            )
        })
        .collect();
    // Napiwek: nobody is behind on rounding yet, so the odd grosz goes to the lowest id (alice).
    assert_eq!(
        owed,
        named(&[
            ("alice", 500 + 101 + 1000),
            ("bob", 1200 + 250 + 100),
            ("carol", 250 + 100 + 2000)
        ])
    );
    assert_eq!(body["amount"], 5501);
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 3900), ("bob", -1550), ("carol", -2350)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn several_people_can_pay_one_expense(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    add(
        &app,
        "expense",
        &alice,
        vec![item("Nocleg", 30000, &[&alice, &bob, &carol])],
        &[(&alice, 20000), (&bob, 10000)],
        json!({"category": "lodging"}),
    )
    .await
    .expect(StatusCode::CREATED);
    let balance = bilans(&app, &alice).await;
    assert_eq!(
        money_nets(&balance),
        named(&[("alice", 10000), ("carol", -10000)])
    );
    assert_eq!(transfers(&balance), [t("carol", "alice", "", 10000)]);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_expense_in_another_currency_is_converted_once_and_adds_up(pool: PgPool) {
    let rates = rates_stub("4.25").await;
    let app = TestApp::with_rates(pool, &rates);
    let (alice, bob, carol) = trio(&app).await;
    let day = app::clock::local_today(app.now()) - TimeDelta::days(3);
    let body = expense(
        &app,
        &alice,
        vec![item("Muzeum", 10000, &[&alice, &bob, &carol])],
        json!({"currency": "EUR", "occurred_on": day}),
    )
    .await;
    assert_eq!(
        (&body["currency"], &body["amount"], &body["base_amount"]),
        (&json!("EUR"), &json!(10000), &json!(42500))
    );
    assert_eq!(body["rate"], "4.25");
    assert_eq!(body["rate_date"], json!(day));
    let owed: BTreeMap<String, (i64, i64)> = body["breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["user"]["username"].as_str().unwrap().to_owned(),
                (
                    r["owed"].as_i64().unwrap(),
                    r["owed_base"].as_i64().unwrap(),
                ),
            )
        })
        .collect();
    // 33.34 + 33.33 + 33.33 EUR -> 141.695 + 141.6525 + 141.6525 zł, rounded to sum to 425 zł
    assert_eq!(
        owed,
        BTreeMap::from([
            ("alice".to_owned(), (3334, 14170)),
            ("bob".to_owned(), (3333, 14165)),
            ("carol".to_owned(), (3333, 14165))
        ])
    );
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 42500 - 14170), ("bob", -14165), ("carol", -14165)])
    );
    // the rate is kept: the next entry that day does not ask again
    assert_eq!(app.count("select count(*) from exchange_rates").await, 1);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_currency_with_no_minor_units(pool: PgPool) {
    let rates = rates_stub("0.0262").await;
    let app = TestApp::with_rates(pool, &rates);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let body = expense(
        &app,
        &alice,
        vec![item("Ramen", 3000, &[&bob])],
        json!({"currency": "JPY"}),
    )
    .await;
    assert_eq!(body["base_amount"], 7860); // 78.60 zł
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 7860), ("bob", -7860)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn without_a_rate_nothing_is_stored(pool: PgPool) {
    let app = TestApp::new(pool); // the rates API is unreachable
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let response = add(
        &app,
        "expense",
        &alice,
        vec![item("x", 100, &[&bob])],
        &[],
        json!({"currency": "EUR"}),
    )
    .await;
    response.expect(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.code(), "rate_unavailable");
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn expense_validation(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let ghost = common::insert_user(app.db(), "ghost").await;
    sqlx::query("update users set is_active = false where id = $1")
        .bind(ghost)
        .execute(app.db())
        .await
        .unwrap();
    let tomorrow = app::clock::local_today(app.now()) + TimeDelta::days(2);
    let base = || {
        json!({"kind": "expense", "title": "Pizza", "items": [item("Pizza", 1000, &[&alice, &bob])],
               "payers": [{"user_id": alice.id, "amount": 1000}]})
    };
    let cases: Vec<(Value, &str)> = vec![
        (merge(base(), json!({"payers": []})), "payers_required"),
        (merge(base(), json!({"items": []})), "items_required"),
        (merge(base(), json!({"title": "   "})), "title_required"),
        (
            merge(base(), json!({"currency": "XXX"})),
            "unsupported_currency",
        ),
        (
            merge(base(), json!({"occurred_on": tomorrow})),
            "future_date",
        ),
        (
            merge(
                base(),
                json!({"payers": [{"user_id": alice.id, "amount": 999}]}),
            ),
            "payers_total",
        ),
        (
            merge(
                base(),
                json!({"items": [split_item("Pizza", 1000, "exact", &[(&alice, 1), (&bob, 1)])]}),
            ),
            "exact_total",
        ),
        (
            merge(
                base(),
                json!({"items": [json!({"name": "Pizza", "amount": 1000, "shares": [{"user_id": alice.id}, {"user_id": alice.id}]})]}),
            ),
            "person_twice",
        ),
        (
            merge(
                base(),
                json!({"payers": [{"user_id": alice.id, "amount": 500}, {"user_id": alice.id, "amount": 500}]}),
            ),
            "payer_twice",
        ),
        (
            merge(
                base(),
                json!({"items": [json!({"name": "Pizza", "amount": 1000, "shares": [{"user_id": alice.id}, {"user_id": ghost}]})]}),
            ),
            "unknown_member",
        ),
    ];
    for (body, code) in cases {
        let response = app.post_as(&alice, ENTRIES, body.clone()).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(response.code(), code, "{body}");
    }
    let unknown_kind = app.post_as(&alice, ENTRIES, json!({"kind": "loan"})).await;
    unknown_kind.expect(StatusCode::BAD_REQUEST);
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn anyone_can_edit_an_expense_which_replaces_what_it_means(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let entry = expense(
        &app,
        &alice,
        vec![item("Pizza", 9000, &[&alice, &bob, &carol])],
        json!({}),
    )
    .await;
    let body = json!({"kind": "expense", "title": "Pizza bez Carol", "items": [item("Pizza", 6000, &[&alice, &bob])],
                      "payers": [{"user_id": alice.id, "amount": 6000}], "version": entry["version"]});
    let edited = app
        .put_as(&carol, &format!("{ENTRIES}/{}", entry["id"]), body)
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(
        (&edited["title"], &edited["version"]),
        (&json!("Pizza bez Carol"), &json!(2))
    );
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 3000), ("bob", -3000)])
    );
    assert_eq!(
        app.count("select count(*) from ledger_obligations").await,
        1
    );
    assert_eq!(
        app.count("select count(*) from ledger_entry_shares").await,
        2
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_outdated_edit_is_refused_instead_of_overwriting(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let entry = expense(
        &app,
        &alice,
        vec![item("Pizza", 1000, &[&alice, &bob])],
        json!({}),
    )
    .await;
    let body = json!({"kind": "expense", "title": "Pizza", "items": [item("Pizza", 2000, &[&alice, &bob])],
                      "payers": [{"user_id": alice.id, "amount": 2000}], "version": entry["version"]});
    let url = format!("{ENTRIES}/{}", entry["id"]);
    app.put_as(&bob, &url, body.clone())
        .await
        .expect(StatusCode::OK);
    let stale = app.put_as(&alice, &url, body).await;
    stale.expect(StatusCode::CONFLICT);
    assert_eq!(stale.code(), "stale_version");
    let wrong_kind = json!({"kind": "income", "title": "x", "items": [item("x", 1, &[&alice])],
                            "payers": [{"user_id": alice.id, "amount": 1}], "version": 2});
    assert_eq!(
        app.put_as(&alice, &url, wrong_kind).await.code(),
        "kind_mismatch"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn deleting_an_expense_cancels_it_and_it_stops_counting(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let entry = expense(
        &app,
        &alice,
        vec![item("Pizza", 1000, &[&alice, &bob])],
        json!({}),
    )
    .await;
    let cancelled = act(&app, &bob, &entry["id"], "cancel")
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(bilans(&app, &alice).await["members"], json!([]));
    act(&app, &bob, &entry["id"], "cancel")
        .await
        .expect(StatusCode::CONFLICT);
    let body = json!({"kind": "expense", "title": "x", "items": [item("x", 1, &[&alice])],
                      "payers": [{"user_id": alice.id, "amount": 1}], "version": cancelled["version"]});
    let edit = app
        .put_as(&alice, &format!("{ENTRIES}/{}", entry["id"]), body)
        .await;
    edit.expect(StatusCode::CONFLICT);
    assert_eq!(edit.code(), "entry_deleted");
    assert_eq!(app.count("select count(*) from ledger_entries").await, 1); // never deleted
}

// --- income ------------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_income_is_a_negative_expense(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob, carol) = trio(&app).await;
    subscribe(app.db(), &[alice.id, bob.id, carol.id]).await;
    let body = add(
        &app,
        "income",
        &alice,
        vec![item("Zwrot kaucji", 9000, &[&alice, &bob, &carol])],
        &[],
        json!({"title": "Kaucja"}),
    )
    .await
    .expect(StatusCode::CREATED)
    .json();
    assert_eq!(
        (&body["kind"], &body["base_amount"]),
        (&json!("income"), &json!(9000))
    );
    // the breakdown reads positive: alice received 90 zł, everybody's part is 30 zł
    assert_eq!(
        (&body["breakdown"][0]["paid"], &body["breakdown"][0]["owed"]),
        (&json!(9000), &json!(3000))
    );
    // and alice now owes the others their parts
    assert_eq!(
        money_nets(&bilans(&app, &bob).await),
        named(&[("alice", -6000), ("bob", 3000), ("carol", 3000)])
    );
    let pushes = take_pushes(app.db()).await;
    assert_eq!(
        pushes
            .iter()
            .map(|p| (p.0, p.1.as_str()))
            .collect::<Vec<_>>(),
        [(bob.id, "Nowy przychód"), (carol.id, "Nowy przychód")]
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_income_cancels_out_the_expense_it_refunds(pool: PgPool) {
    let rates = rates_stub("4.25").await;
    let app = TestApp::with_rates(pool, &rates);
    let (alice, bob, carol) = trio(&app).await;
    let people = [&alice, &bob, &carol];
    add(
        &app,
        "expense",
        &alice,
        vec![item("Bilety", 3000, &people)],
        &[],
        json!({"currency": "EUR"}),
    )
    .await
    .expect(StatusCode::CREATED);
    add(
        &app,
        "income",
        &alice,
        vec![item("Zwrot", 3000, &people)],
        &[],
        json!({"currency": "EUR"}),
    )
    .await
    .expect(StatusCode::CREATED);
    assert_eq!(bilans(&app, &alice).await["members"], json!([]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn income_validation_speaks_of_receivers(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let response = add(
        &app,
        "income",
        &alice,
        vec![item("Zwrot", 1000, &[&alice, &bob])],
        &[(&alice, 1)],
        json!({}),
    )
    .await;
    response.expect(StatusCode::BAD_REQUEST);
    assert!(
        response.json()["errors"]["payers"][0]
            .as_str()
            .unwrap()
            .starts_with("Otrzymujący")
    );
}

// --- debts -------------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_pact_settlement_is_one_debt_entry_that_counts_at_once(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let id = pact_debts(
        &app,
        vec![owes(&bob, &alice, 1000, ""), owes(&carol, &alice, 500, "")],
    )
    .await;
    let entry = app.get_as(&alice, &format!("{ENTRIES}/{id}")).await.json();
    assert_eq!(
        (
            &entry["kind"],
            &entry["status"],
            &entry["amount"],
            &entry["base_amount"],
            &entry["source"]
        ),
        (
            &json!("debt"),
            &json!("confirmed"),
            &json!(1500),
            &json!(1500),
            &json!("pact")
        )
    );
    assert_eq!(entry["created_by"], Value::Null);
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 1500), ("bob", -1000), ("carol", -500)])
    );
    // the pact's entries come unpaged by pact id
    let listed = app
        .get_as(&alice, &format!("{ENTRIES}?pact_id={}", entry["pact_id"]))
        .await
        .json();
    assert_eq!(listed["items"][0]["id"], id);
    assert_eq!(listed["next_cursor"], Value::Null);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_settlement_mixing_money_and_goods_has_no_headline_amount(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let id = pact_debts(
        &app,
        vec![
            owes(&bob, &alice, 1000, ""),
            owes(&carol, &alice, 1, "kolacja"),
        ],
    )
    .await;
    let entry = app.get_as(&alice, &format!("{ENTRIES}/{id}")).await.json();
    assert_eq!(
        (&entry["amount"], &entry["currency"], &entry["item"]),
        (&Value::Null, &json!(""), &json!(""))
    );
    assert_eq!(
        nets(&bilans(&app, &alice).await, "kolacja"),
        named(&[("alice", 1), ("carol", -1)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn nothing_owed_records_nothing_and_debts_are_validated(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let pact = insert_pact(app.db(), alice.id).await;
    assert_eq!(record_debts(&app, pact, vec![]).await, None);
    for debts in [
        vec![owes(&bob, &alice, 0, "")],
        vec![owes(&alice, &alice, 100, "")],
    ] {
        let mut uow = app::db::Uow::begin(app.db()).await.unwrap();
        let today = app::clock::local_today(app.now());
        let result = app::ledger::record_debts(&mut uow, &app.ctx, debts, "x", pact, today).await;
        assert!(matches!(result, Err(app::AppError::Invalid(_))));
    }
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_pact_debt_can_only_change_through_the_pact(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let id = pact_debts(&app, vec![owes(&bob, &alice, 1000, "")]).await;
    act(&app, &alice, &id, "confirm")
        .await
        .expect(StatusCode::CONFLICT);
    let cancel = act(&app, &alice, &id, "cancel").await;
    cancel.expect(StatusCode::CONFLICT);
    assert_eq!(cancel.code(), "pact_debt");
    let edit = app
        .put_as(
            &alice,
            &format!("{ENTRIES}/{id}"),
            json!({"kind": "debt", "debtor_id": bob.id, "amount": 5, "version": 1}),
        )
        .await;
    edit.expect(StatusCode::CONFLICT);
    let entry = app.get_as(&alice, &format!("{ENTRIES}/{id}")).await.json();
    assert_eq!(
        entry["actions"],
        json!({"confirm": false, "reject": false, "cancel": false, "edit": false, "attach": true})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_member_records_goods_owed_to_them_and_can_drop_them(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let entry = app
        .post_as(
            &alice,
            ENTRIES,
            json!({"kind": "debt", "debtor_id": bob.id, "item": " kawa ", "amount": 2}),
        )
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        (
            &entry["item"],
            &entry["amount"],
            &entry["source"],
            &entry["title"]
        ),
        (&json!("kawa"), &json!(2), &json!("manual"), &json!("kawa"))
    );
    assert_eq!(
        nets(&bilans(&app, &bob).await, "kawa"),
        named(&[("alice", 2), ("bob", -2)])
    );
    act(&app, &bob, &entry["id"], "cancel")
        .await
        .expect(StatusCode::FORBIDDEN); // only the person owed
    act(&app, &alice, &entry["id"], "cancel")
        .await
        .expect(StatusCode::OK);
    assert_eq!(bilans(&app, &bob).await["members"], json!([]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn debt_validation(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let cases = [
        (json!({"debtor_id": alice.id}), "self_debt"),
        (json!({"debtor_id": bob.id, "amount": 0}), "invalid_amount"),
        (
            json!({"debtor_id": bob.id, "amount": 1001}),
            "too_many_goods",
        ),
        (
            json!({"debtor_id": bob.id, "item": "", "currency": "XYZ"}),
            "unsupported_currency",
        ),
        (json!({"debtor_id": 9999}), "unknown_member"),
        (
            json!({"debtor_id": bob.id, "creditor_id": carol.id}),
            "not_your_debt",
        ),
    ];
    for (bad, code) in cases {
        let body = merge(json!({"kind": "debt", "item": "kawa", "amount": 2}), bad);
        let response = app.post_as(&alice, ENTRIES, body.clone()).await;
        assert_eq!(
            (response.status, response.code().as_str()),
            (StatusCode::BAD_REQUEST, code),
            "{body}"
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_member_records_money_owed_to_them_or_admits_a_debt(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    subscribe(app.db(), &[alice.id, bob.id]).await;
    let owed = app
        .post_as(&alice, ENTRIES, json!({"kind": "debt", "debtor_id": bob.id, "amount": 5000, "title": "Bilety", "note": "za koncert"}))
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        (
            &owed["title"],
            &owed["note"],
            &owed["amount"],
            &owed["currency"],
            &owed["base_amount"]
        ),
        (
            &json!("Bilety"),
            &json!("za koncert"),
            &json!(5000),
            &json!("PLN"),
            &json!(5000)
        )
    );
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            bob.id,
            "Nowe rozliczenie".to_owned(),
            "alice zapisał(a), że jesteś winien(-na) 50 zł".to_owned()
        )]
    );
    let admitted = app
        .post_as(
            &bob,
            ENTRIES,
            json!({"kind": "debt", "debtor_id": bob.id, "creditor_id": alice.id, "amount": 2000}),
        )
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(admitted["title"], "Dług");
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            alice.id,
            "Nowe rozliczenie".to_owned(),
            "bob zapisał(a), że jest Ci winien(-na) 20 zł".to_owned()
        )]
    );
    assert_eq!(
        money_nets(&bilans(&app, &bob).await),
        named(&[("alice", 7000), ("bob", -7000)])
    );
    // still the creditor's to change: the debtor can't wipe it
    act(&app, &bob, &admitted["id"], "cancel")
        .await
        .expect(StatusCode::FORBIDDEN);
    act(&app, &alice, &admitted["id"], "cancel")
        .await
        .expect(StatusCode::OK);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_debt_in_another_currency_is_converted(pool: PgPool) {
    let rates = rates_stub("4.25").await;
    let app = TestApp::with_rates(pool, &rates);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let entry = app
        .post_as(
            &alice,
            ENTRIES,
            json!({"kind": "debt", "debtor_id": bob.id, "amount": 1000, "currency": "EUR"}),
        )
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        (&entry["amount"], &entry["currency"], &entry["base_amount"]),
        (&json!(1000), &json!("EUR"), &json!(4250))
    );
    assert_eq!(entry["obligations"][0]["amount"], 4250);
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 4250), ("bob", -4250)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn money_that_converts_to_nothing_is_refused(pool: PgPool) {
    let rates = rates_stub("0.00026").await;
    let app = TestApp::with_rates(pool, &rates);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let response = app
        .post_as(
            &alice,
            ENTRIES,
            json!({"kind": "debt", "debtor_id": bob.id, "amount": 1, "currency": "IDR"}),
        )
        .await;
    response.expect(StatusCode::BAD_REQUEST);
    assert_eq!(response.code(), "amount_too_small");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_creditor_edits_a_debt(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let entry = app
        .post_as(
            &alice,
            ENTRIES,
            json!({"kind": "debt", "debtor_id": bob.id, "amount": 1000}),
        )
        .await
        .json();
    assert_eq!(entry["actions"]["edit"], true);
    let url = format!("{ENTRIES}/{}", entry["id"]);
    let body = json!({"kind": "debt", "debtor_id": carol.id, "creditor_id": alice.id, "amount": 3, "item": "piwo", "version": entry["version"]});
    app.put_as(&bob, &url, body.clone())
        .await
        .expect(StatusCode::FORBIDDEN);
    let edited = app
        .put_as(&alice, &url, body)
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(
        (&edited["item"], &edited["amount"], &edited["title"]),
        (&json!("piwo"), &json!(3), &json!("piwo"))
    );
    let balance = bilans(&app, &alice).await;
    assert_eq!(money_nets(&balance), BTreeMap::new());
    assert_eq!(
        nets(&balance, "piwo"),
        named(&[("alice", 3), ("carol", -3)])
    );
}

// --- payments ----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_payment_only_counts_once_the_receiver_confirms(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    pact_debts(&app, vec![owes(&bob, &alice, 1500, "")]).await;
    let entry = pay(&app, &bob, &alice, 1000, json!({"note": "BLIK"}))
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        (
            &entry["kind"],
            &entry["status"],
            &entry["title"],
            &entry["note"]
        ),
        (
            &json!("payment"),
            &json!("pending"),
            &json!("Spłata"),
            &json!("BLIK")
        )
    );
    assert_eq!(entry["details"], json!({"from": bob.id, "to": alice.id}));
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 1500), ("bob", -1500)])
    ); // not yet
    let confirmed = act(&app, &alice, &entry["id"], "confirm")
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(confirmed["status"], "confirmed");
    assert!(confirmed["decided_at"].is_string());
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 500), ("bob", -500)])
    );
    act(&app, &alice, &entry["id"], "confirm")
        .await
        .expect(StatusCode::CONFLICT); // only once
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn paying_everything_clears_the_balance_and_overpaying_turns_it_around(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    pact_debts(&app, vec![owes(&bob, &alice, 1500, "")]).await;
    let paid = pay(&app, &bob, &alice, 1500, json!({})).await.json();
    act(&app, &alice, &paid["id"], "confirm")
        .await
        .expect(StatusCode::OK);
    let balance = bilans(&app, &alice).await;
    assert_eq!(
        (&balance["members"], &balance["settlements"]),
        (&json!([]), &json!([]))
    );
    let feed = app.get_as(&alice, ENTRIES).await.json();
    let rows: Vec<_> = feed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["kind"].clone(), r["status"].clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (json!("payment"), json!("confirmed")),
            (json!("debt"), json!("confirmed"))
        ]
    );
    let more = pay(&app, &bob, &alice, 500, json!({})).await.json();
    act(&app, &alice, &more["id"], "confirm")
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        transfers(&bilans(&app, &alice).await),
        [t("alice", "bob", "", 500)]
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_the_right_party_decides_a_payment(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let entry = pay(&app, &bob, &alice, 400, json!({})).await.json();
    assert_eq!(
        entry["actions"],
        json!({"confirm": false, "reject": false, "cancel": true, "edit": false, "attach": true})
    );
    let as_alice = app
        .get_as(&alice, &format!("{ENTRIES}/{}", entry["id"]))
        .await
        .json();
    assert_eq!(as_alice["actions"]["confirm"], true);
    act(&app, &bob, &entry["id"], "confirm")
        .await
        .expect(StatusCode::FORBIDDEN);
    act(&app, &bob, &entry["id"], "reject")
        .await
        .expect(StatusCode::FORBIDDEN);
    act(&app, &alice, &entry["id"], "cancel")
        .await
        .expect(StatusCode::FORBIDDEN);
    act(&app, &carol, &entry["id"], "confirm")
        .await
        .expect(StatusCode::FORBIDDEN);
    act(&app, &carol, &json!(9999), "confirm")
        .await
        .expect(StatusCode::NOT_FOUND);
    let edit = app
        .put_as(
            &bob,
            &format!("{ENTRIES}/{}", entry["id"]),
            json!({"kind": "payment", "to_user_id": alice.id, "amount": 1, "version": 1}),
        )
        .await;
    assert_eq!(
        (edit.status, edit.code().as_str()),
        (StatusCode::CONFLICT, "not_editable")
    );
    // rejected and withdrawn payments never count
    assert_eq!(
        act(&app, &alice, &entry["id"], "reject").await.json()["status"],
        "rejected"
    );
    let second = pay(&app, &bob, &alice, 400, json!({})).await.json();
    assert_eq!(
        act(&app, &bob, &second["id"], "cancel").await.json()["status"],
        "cancelled"
    );
    act(&app, &alice, &second["id"], "confirm")
        .await
        .expect(StatusCode::CONFLICT);
    assert_eq!(bilans(&app, &alice).await["members"], json!([]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn payment_validation(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    assert_eq!(
        pay(&app, &bob, &bob, 100, json!({})).await.code(),
        "self_payment"
    );
    assert_eq!(
        pay(&app, &bob, &alice, 0, json!({})).await.code(),
        "invalid_amount"
    );
    let body = json!({"kind": "payment", "to_user_id": 9999, "amount": 5});
    assert_eq!(
        app.post_as(&bob, ENTRIES, body).await.code(),
        "unknown_member"
    );
    sqlx::query("update users set is_active = false where id = $1")
        .bind(alice.id)
        .execute(app.db())
        .await
        .unwrap();
    assert_eq!(
        pay(&app, &bob, &alice, 100, json!({})).await.code(),
        "unknown_member"
    );
    assert_eq!(app.count("select count(*) from ledger_entries").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn goods_are_returned_the_same_way(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    pact_debts(&app, vec![owes(&bob, &alice, 2, "piwo")]).await;
    let returned = pay(&app, &bob, &alice, 2, json!({"item": "piwo"}))
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        (&returned["title"], &returned["currency"]),
        (&json!("Zwrot"), &json!(""))
    );
    act(&app, &alice, &returned["id"], "confirm")
        .await
        .expect(StatusCode::OK);
    assert_eq!(bilans(&app, &alice).await["members"], json!([]));
}

// --- the Bilans --------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn balances_combine_expenses_pact_debts_and_payments(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    expense(
        &app,
        &alice,
        vec![item("Zakupy", 9000, &[&alice, &bob, &carol])],
        json!({}),
    )
    .await;
    pact_debts(&app, vec![owes(&alice, &carol, 2000, "")]).await; // alice lost a bet to carol
    let never_mind = pay(&app, &bob, &alice, 1000, json!({})).await.json();
    act(&app, &bob, &never_mind["id"], "cancel")
        .await
        .expect(StatusCode::OK);
    let paid = pay(&app, &bob, &alice, 1000, json!({})).await.json();
    act(&app, &alice, &paid["id"], "confirm")
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        money_nets(&bilans(&app, &alice).await),
        named(&[("alice", 3000), ("bob", -2000), ("carol", -1000)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn following_every_suggestion_leaves_everyone_even(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let dave = app.member("dave").await;
    expense(
        &app,
        &alice,
        vec![item("Nocleg", 40000, &[&alice, &bob, &carol, &dave])],
        json!({}),
    )
    .await;
    expense(
        &app,
        &bob,
        vec![item("Paliwo", 12000, &[&alice, &bob, &carol])],
        json!({"category": "transport"}),
    )
    .await;
    expense(
        &app,
        &carol,
        vec![item("Kolacja", 3001, &[&bob, &dave])],
        json!({}),
    )
    .await;
    pact_debts(&app, vec![owes(&dave, &bob, 777, "")]).await;
    let members: BTreeMap<String, &Member> = [&alice, &bob, &carol, &dave]
        .into_iter()
        .map(|m| (m.username.clone(), m))
        .collect();
    for (debtor, creditor, _, amount) in transfers(&bilans(&app, &alice).await) {
        let entry = pay(
            &app,
            members[&debtor],
            members[&creditor],
            amount,
            json!({}),
        )
        .await
        .json();
        act(&app, members[&creditor], &entry["id"], "confirm")
            .await
            .expect(StatusCode::OK);
    }
    let balance = bilans(&app, &alice).await;
    assert_eq!(
        (&balance["members"], &balance["settlements"]),
        (&json!([]), &json!([]))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn suggestions_assume_pending_payments_go_through(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    expense(&app, &alice, vec![item("Pizza", 2000, &[&bob])], json!({})).await;
    let entry = pay(&app, &bob, &alice, 1500, json!({})).await.json();
    let balance = bilans(&app, &alice).await;
    assert_eq!(
        money_nets(&balance),
        named(&[("alice", 2000), ("bob", -2000)])
    ); // confirmed only
    assert_eq!(transfers(&balance), [t("bob", "alice", "", 500)]); // what is left to pay
    let pending = &balance["pending"][0];
    assert_eq!(
        (
            &pending["entry_id"],
            &pending["debtor"]["username"],
            &pending["amount"]
        ),
        (&entry["id"], &json!("bob"), &json!(1500))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn goods_net_per_pair_and_per_item_never_against_money(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    pact_debts(&app, vec![owes(&bob, &alice, 1000, "")]).await;
    pact_debts(&app, vec![owes(&bob, &alice, 2, "Piwo")]).await;
    pact_debts(&app, vec![owes(&alice, &bob, 1, "  piwo ")]).await;
    pact_debts(&app, vec![owes(&carol, &bob, 1, "piwo")]).await;
    pact_debts(&app, vec![owes(&bob, &alice, 1, "kolacja")]).await;
    let mut got = transfers(&bilans(&app, &alice).await);
    got.sort_by_key(|t| (t.2.to_lowercase(), t.0.clone()));
    assert_eq!(
        got,
        [
            t("bob", "alice", "", 1000),
            t("bob", "alice", "kolacja", 1),
            t("bob", "alice", "Piwo", 1),
            t("carol", "bob", "Piwo", 1), // goods are never passed on to somebody else
        ]
    );
}

// --- the feed ----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn everyone_reads_the_feed_newest_first_paged_and_filtered(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let old_day = app::clock::local_today(app.now()) - TimeDelta::days(9);
    let old = expense(
        &app,
        &alice,
        vec![item("Stare", 100, &[&bob])],
        json!({"occurred_on": old_day}),
    )
    .await;
    let first = pact_debts(&app, vec![owes(&bob, &alice, 100, "")]).await;
    let second = expense(&app, &alice, vec![item("Nowe", 100, &[&bob])], json!({})).await;
    let feed = app.get_as(&carol, ENTRIES).await.json();
    let ids: Vec<&Value> = feed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["id"])
        .collect();
    assert_eq!(ids, [&second["id"], &first, &old["id"]]);

    for _ in 0..30 {
        pact_debts(&app, vec![owes(&bob, &alice, 100, "")]).await;
    }
    let page = app.get_as(&alice, ENTRIES).await.json();
    assert_eq!(page["items"].as_array().unwrap().len(), 30);
    let cursor = page["next_cursor"].as_str().unwrap();
    let rest = app
        .get_as(&alice, &format!("{ENTRIES}?cursor={cursor}"))
        .await
        .json();
    assert_eq!(rest["items"].as_array().unwrap().len(), 3);
    assert_eq!(rest["next_cursor"], Value::Null);
    assert_eq!(rest["items"][2]["id"], old["id"]);
    let expenses = app
        .get_as(&alice, &format!("{ENTRIES}?kind=expense"))
        .await
        .json();
    let titles: Vec<&Value> = expenses["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["title"])
        .collect();
    assert_eq!(titles, [&json!("Pizza"), &json!("Pizza")]);
    app.get_as(&alice, &format!("{ENTRIES}?cursor=nope"))
        .await
        .expect(StatusCode::BAD_REQUEST);
}

// --- notifications -----------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn people_in_an_expense_are_pushed_and_everyone_refreshes(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob, carol) = trio(&app).await;
    subscribe(app.db(), &[alice.id, bob.id, carol.id]).await;
    let _listener = app.start_listener().await;
    let mut events = app.ctx.hub.subscribe();
    let entry = expense(
        &app,
        &alice,
        vec![item("Pizza", 3000, &[&alice, &bob])],
        json!({}),
    )
    .await;
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            bob.id,
            "Nowy wydatek".to_owned(),
            "alice: Pizza (30 zł)".to_owned()
        )]
    );
    act(&app, &bob, &entry["id"], "cancel")
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            alice.id,
            "Wydatek usunięty".to_owned(),
            "bob: Pizza (30 zł)".to_owned()
        )]
    );
    for _ in 0..2 {
        let message = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        let app::events::HubMessage::Event(envelope) = message else {
            panic!("expected an event")
        };
        assert_eq!(envelope.audience, app::events::Audience::All);
        assert_eq!(
            envelope.event,
            app::events::Event::LedgerUpdated {
                entry_id: entry["id"].as_i64().unwrap()
            }
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn payments_notify_the_other_side(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    subscribe(app.db(), &[alice.id, bob.id]).await;
    pact_debts(&app, vec![owes(&bob, &alice, 1000, "")]).await;
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            alice.id,
            "Nowe rozliczenie".to_owned(),
            "bob jest Ci winien 10 zł: Zakład".to_owned()
        )]
    );
    let entry = pay(&app, &bob, &alice, 400, json!({})).await.json();
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            alice.id,
            "Wpłata do potwierdzenia".to_owned(),
            "bob twierdzi, że zapłacił(a) Ci 4 zł".to_owned()
        )]
    );
    act(&app, &alice, &entry["id"], "confirm")
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        take_pushes(app.db()).await,
        [(
            bob.id,
            "Wpłata potwierdzona".to_owned(),
            "alice potwierdził(a) wpłatę 4 zł".to_owned()
        )]
    );
}

// --- attachments -------------------------------------------------------------------------------

async fn photo(app: &TestApp, who: &Member) -> Value {
    app.upload(who, "photo", "image/png", png(64, 48))
        .await
        .expect(StatusCode::CREATED)
        .json()["id"]
        .clone()
}

async fn attach(app: &TestApp, who: &Member, entry: &Value) -> TestResponse {
    let upload = photo(app, who).await;
    app.post_as(
        who,
        &format!("{ENTRIES}/{entry}/attachments"),
        json!({"upload_id": upload}),
    )
    .await
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_receipt_photo_on_an_expense(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let entry = expense(
        &app,
        &alice,
        vec![item("Pizza", 1000, &[&alice, &bob])],
        json!({}),
    )
    .await;
    let body = attach(&app, &carol, &entry["id"])
        .await
        .expect(StatusCode::CREATED)
        .json(); // anyone, like editing
    let picture = &body["attachments"][0];
    assert_eq!(picture["uploaded_by"]["username"], "carol");
    assert_eq!(
        (&picture["width"], &picture["height"]),
        (&json!(64), &json!(48))
    );
    let file = app.get(picture["image"]["sm"].as_str().unwrap()).await;
    file.expect(StatusCode::OK);
    let url = format!("{ENTRIES}/{}/attachments/{}", entry["id"], picture["id"]);
    app.delete_as(&bob, &url)
        .await
        .expect(StatusCode::FORBIDDEN); // neither uploader nor author
    let after = app
        .delete_as(&alice, &url)
        .await
        .expect(StatusCode::OK)
        .json(); // the author
    assert_eq!(after["attachments"], json!([]));
    assert_eq!(app.count("select count(*) from media_trash").await, 1);
    app.delete_as(&alice, &url)
        .await
        .expect(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn attachments_are_limited_and_only_for_the_people_involved(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = trio(&app).await;
    let entry = pay(&app, &bob, &alice, 100, json!({})).await.json();
    attach(&app, &carol, &entry["id"])
        .await
        .expect(StatusCode::FORBIDDEN);
    for _ in 0..10 {
        attach(&app, &bob, &entry["id"])
            .await
            .expect(StatusCode::CREATED);
    }
    let eleventh = attach(&app, &alice, &entry["id"]).await;
    assert_eq!(
        (eleventh.status, eleventh.code().as_str()),
        (StatusCode::BAD_REQUEST, "too_many_attachments")
    );
    // somebody else's upload cannot be used
    let fresh = pay(&app, &bob, &alice, 100, json!({})).await.json();
    let upload = photo(&app, &carol).await;
    let stolen = app
        .post_as(
            &alice,
            &format!("{ENTRIES}/{}/attachments", fresh["id"]),
            json!({"upload_id": upload}),
        )
        .await;
    assert_eq!(
        (stolen.status, stolen.code().as_str()),
        (StatusCode::BAD_REQUEST, "upload_not_found")
    );
}
