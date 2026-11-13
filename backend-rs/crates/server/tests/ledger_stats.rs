//! Spending statistics and exchange rates.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeMap;

use axum::http::StatusCode;
use chrono::{Datelike as _, NaiveDate, TimeDelta};
use common::{Member, TestApp, insert_pact, rates_stub, record_debts};
use domain::ledger::money::Debt;
use serde_json::{Value, json};
use sqlx::PgPool;

const ENTRIES: &str = "/api/ledger/entries";

fn item(name: &str, amount: i64, people: &[&Member]) -> Value {
    json!({"name": name, "amount": amount, "shares": people.iter().map(|m| json!({"user_id": m.id})).collect::<Vec<_>>()})
}

async fn add(app: &TestApp, kind: &str, by: &Member, item: Value, extra: Value) {
    let amount = item["amount"].clone();
    let mut body = json!({"kind": kind, "title": "x", "items": [item], "payers": [{"user_id": by.id, "amount": amount}]});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    app.post_as(by, ENTRIES, body)
        .await
        .expect(StatusCode::CREATED);
}

async fn stats(app: &TestApp, who: &Member, query: &str) -> Value {
    app.get_as(who, &format!("/api/ledger/stats{query}"))
        .await
        .expect(StatusCode::OK)
        .json()
}

fn by_key(body: &Value) -> BTreeMap<String, i64> {
    body["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["key"].as_str().unwrap().to_owned(),
                c["spent"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn people(body: &Value) -> BTreeMap<String, (i64, i64)> {
    body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["user"]["username"].as_str().unwrap().to_owned(),
                (p["share"].as_i64().unwrap(), p["paid"].as_i64().unwrap()),
            )
        })
        .collect()
}

fn named<const N: usize>(pairs: [(&str, i64); N]) -> BTreeMap<String, i64> {
    pairs.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()
}

async fn owes(app: &TestApp, debtor: &Member, creditor: &Member, amount: i64, item: &str) {
    let pact = insert_pact(app.db(), creditor.id).await;
    let debt = Debt {
        debtor_id: debtor.id,
        creditor_id: creditor.id,
        amount,
        item: item.into(),
    };
    record_debts(app, pact, vec![debt]).await.unwrap();
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_empty_period_is_all_zeros_with_a_filled_series(pool: PgPool) {
    let app = TestApp::new(pool);
    let alice = app.member("alice").await;
    let body = stats(&app, &alice, "?start=2026-03-01&end=2026-03-05").await;
    assert_eq!(
        (
            &body["spent"],
            &body["count"],
            &body["categories"],
            &body["people"]
        ),
        (&json!(0), &json!(0), &json!([]), &json!([]))
    );
    assert_eq!(body["unit"], "day");
    let periods: Vec<&str> = body["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["period"].as_str().unwrap())
        .collect();
    assert_eq!(
        periods,
        [
            "2026-03-01",
            "2026-03-02",
            "2026-03-03",
            "2026-03-04",
            "2026-03-05"
        ]
    );
    let all_time = stats(&app, &alice, "").await;
    assert_eq!(all_time["spent"], 0);
    assert_eq!(
        all_time["series"].as_array().unwrap().len(),
        1,
        "today only"
    );
    app.get_as(&alice, "/api/ledger/stats?start=2026-05-02&end=2026-05-01")
        .await
        .expect(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_period_includes_its_boundary_days_and_nothing_else(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    for (day, amount) in [
        ("2026-02-28", 100),
        ("2026-03-01", 200),
        ("2026-03-31", 400),
        ("2026-04-01", 800),
    ] {
        add(
            &app,
            "expense",
            &alice,
            item("x", amount, &[&alice, &bob]),
            json!({"occurred_on": day}),
        )
        .await;
    }
    let body = stats(&app, &alice, "?start=2026-03-01&end=2026-03-31").await;
    assert_eq!(
        (&body["spent"], &body["count"], &body["unit"]),
        (&json!(600), &json!(2), &json!("day"))
    );
    let series = body["series"].as_array().unwrap();
    assert_eq!(series.len(), 31);
    let nonzero: BTreeMap<&str, i64> = series
        .iter()
        .filter(|p| p["spent"] != 0)
        .map(|p| (p["period"].as_str().unwrap(), p["spent"].as_i64().unwrap()))
        .collect();
    assert_eq!(
        nonzero,
        BTreeMap::from([("2026-03-01", 200), ("2026-03-31", 400)])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn categories_people_and_totals_add_up(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = (
        app.member("alice").await,
        app.member("bob").await,
        app.member("carol").await,
    );
    add(
        &app,
        "expense",
        &alice,
        item("Kolacja", 9000, &[&alice, &bob, &carol]),
        json!({"category": "food"}),
    )
    .await;
    add(
        &app,
        "expense",
        &bob,
        item("Taxi", 3000, &[&alice, &bob]),
        json!({"category": "transport"}),
    )
    .await;
    let body = stats(&app, &alice, "").await;
    assert_eq!(
        (
            &body["spent"],
            &body["expenses"],
            &body["income"],
            &body["count"]
        ),
        (&json!(12000), &json!(12000), &json!(0), &json!(2))
    );
    assert_eq!(by_key(&body), named([("food", 9000), ("transport", 3000)]));
    assert_eq!(body["categories"][0]["key"], "food", "biggest first");
    assert_eq!(body["categories"][0]["emoji"], "🍽️");
    assert_eq!(
        people(&body),
        BTreeMap::from([
            ("alice".to_owned(), (4500, 9000)),
            ("bob".to_owned(), (4500, 3000)),
            ("carol".to_owned(), (3000, 0))
        ])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_income_nets_out_of_its_category_and_people(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    add(
        &app,
        "expense",
        &alice,
        item("Bilety", 10000, &[&alice, &bob]),
        json!({"category": "fun"}),
    )
    .await;
    add(
        &app,
        "income",
        &bob,
        item("Zwrot", 4000, &[&alice, &bob]),
        json!({"category": "fun"}),
    )
    .await;
    let body = stats(&app, &alice, "").await;
    assert_eq!(
        (&body["spent"], &body["expenses"], &body["income"]),
        (&json!(6000), &json!(10000), &json!(4000))
    );
    assert_eq!(by_key(&body), named([("fun", 6000)]));
    assert_eq!(
        people(&body),
        BTreeMap::from([
            ("alice".to_owned(), (3000, 10000)),
            ("bob".to_owned(), (3000, -4000))
        ])
    );
    // a fully refunded expense counts as nothing, but its category stays listed
    add(
        &app,
        "income",
        &alice,
        item("Zwrot", 6000, &[&alice, &bob]),
        json!({"category": "fun"}),
    )
    .await;
    let refunded = stats(&app, &alice, "").await;
    assert_eq!(
        (&refunded["spent"], by_key(&refunded)),
        (&json!(0), named([("fun", 0)]))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn money_debts_are_a_category_of_their_own(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob, carol) = (
        app.member("alice").await,
        app.member("bob").await,
        app.member("carol").await,
    );
    add(
        &app,
        "expense",
        &alice,
        item("Kolacja", 6000, &[&alice, &bob]),
        json!({"category": "food"}),
    )
    .await;
    owes(&app, &bob, &alice, 1500, "").await;
    owes(&app, &carol, &alice, 2, "piwo").await; // goods are not money
    app.post_as(
        &carol,
        ENTRIES,
        json!({"kind": "debt", "debtor_id": carol.id, "creditor_id": bob.id, "amount": 500}),
    )
    .await
    .expect(StatusCode::CREATED);
    let body = stats(&app, &alice, "").await;
    let debts = body["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "debts")
        .unwrap();
    assert_eq!(
        (
            &debts["label"],
            &debts["emoji"],
            &debts["spent"],
            &debts["count"]
        ),
        (&json!("Długi"), &json!("🤝"), &json!(2000), &json!(2))
    );
    // the goods debt counts as an entry, not as money
    assert_eq!((&body["spent"], &body["count"]), (&json!(8000), &json!(4)));
    let people = people(&body);
    assert_eq!(people["alice"], (3000, 7500)); // half the dinner, and the 15 zł she is owed
    assert_eq!(people["bob"], (4500, 500)); // half the dinner and 15 zł owed; paid out 5 zł
    assert_eq!(people["carol"], (500, 0));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn cancelled_entries_and_payments_are_not_spending(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    add(
        &app,
        "expense",
        &alice,
        item("x", 1000, &[&alice, &bob]),
        json!({}),
    )
    .await;
    let gone: i64 = app.count("select max(id) from ledger_entries").await;
    app.post_as(&bob, &format!("{ENTRIES}/{gone}/cancel"), json!({}))
        .await
        .expect(StatusCode::OK);
    owes(&app, &bob, &alice, 700, "").await;
    let paid = app
        .post_as(
            &bob,
            ENTRIES,
            json!({"kind": "payment", "to_user_id": alice.id, "amount": 700}),
        )
        .await
        .json();
    app.post_as(
        &alice,
        &format!("{ENTRIES}/{}/confirm", paid["id"]),
        json!({}),
    )
    .await
    .expect(StatusCode::OK);
    let body = stats(&app, &alice, "").await;
    assert_eq!((&body["spent"], &body["count"]), (&json!(700), &json!(1))); // only the debt
    assert_eq!(by_key(&body), named([("debts", 700)]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_foreign_expense_counts_in_zloty(pool: PgPool) {
    let rates = rates_stub("4.25").await;
    let app = TestApp::with_rates(pool, &rates);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    add(
        &app,
        "expense",
        &alice,
        item("Hotel", 2000, &[&alice, &bob]),
        json!({"currency": "EUR", "category": "lodging"}),
    )
    .await;
    let body = stats(&app, &alice, "").await;
    assert_eq!(
        (&body["spent"], by_key(&body)),
        (&json!(8500), named([("lodging", 8500)]))
    ); // 20 EUR at 4.25
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn long_periods_are_charted_by_month_and_all_time_spans_the_entries(pool: PgPool) {
    let app = TestApp::new(pool);
    let (alice, bob) = (app.member("alice").await, app.member("bob").await);
    let today = app::clock::local_today(app.now());
    let old = today.with_day(1).unwrap() - TimeDelta::days(200);
    add(
        &app,
        "expense",
        &alice,
        item("Stare", 100, &[&alice, &bob]),
        json!({"occurred_on": old}),
    )
    .await;
    add(
        &app,
        "expense",
        &alice,
        item("Nowe", 300, &[&alice, &bob]),
        json!({}),
    )
    .await;
    let body = stats(&app, &alice, "").await;
    assert_eq!(body["unit"], "month");
    assert_eq!((&body["start"], &body["end"]), (&json!(old), &json!(today)));
    let months: Vec<&str> = body["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["period"].as_str().unwrap())
        .collect();
    assert_eq!(months[0], old.format("%Y-%m").to_string());
    assert_eq!(*months.last().unwrap(), today.format("%Y-%m").to_string());
    assert!(months.len() >= 7);
    let total: i64 = body["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["spent"].as_i64().unwrap())
        .sum();
    assert_eq!(total, 400);
    let count = i64::try_from(months.len()).unwrap();
    assert_eq!(
        body["monthly_average"],
        json!(domain::ledger::stats::monthly_average(400, count))
    );
    let year = today.year();
    let whole_year = stats(
        &app,
        &alice,
        &format!("?start={year}-01-01&end={year}-12-31"),
    )
    .await;
    assert_eq!(whole_year["series"].as_array().unwrap().len(), 12);
}

// --- exchange rates ------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_rate_is_fetched_once_and_kept(pool: PgPool) {
    let rates = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/2026-03-14"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(
            r#"{"amount":1.0,"base":"EUR","date":"2026-03-13","rates":{"PLN":4.2537}}"#,
            "application/json",
        ))
        .expect(1)
        .mount(&rates)
        .await;
    let app = TestApp::with_rates(pool, &rates);
    let alice = app.member("alice").await;
    for _ in 0..2 {
        let body = app
            .get_as(&alice, "/api/ledger/rates?currency=EUR&date=2026-03-14")
            .await
            .expect(StatusCode::OK)
            .json();
        // a weekend uses Friday's rate
        assert_eq!(
            body,
            json!({"currency": "EUR", "rate": "4.2537", "rate_date": "2026-03-13"})
        );
    }
    let pln = app
        .get_as(&alice, "/api/ledger/rates?currency=PLN&date=2026-03-14")
        .await
        .json();
    assert_eq!(pln["rate"], "1");
    let unknown = app.get_as(&alice, "/api/ledger/rates?currency=XXX").await;
    assert_eq!(
        (unknown.status, unknown.code().as_str()),
        (StatusCode::BAD_REQUEST, "unsupported_currency")
    );
    rates.verify().await;
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn concurrent_requests_share_one_fetch(pool: PgPool) {
    let rates = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(
                    r#"{"date":"2026-03-13","rates":{"PLN":4.2537}}"#,
                    "application/json",
                )
                .set_delay(std::time::Duration::from_millis(200)),
        )
        .expect(1)
        .mount(&rates)
        .await;
    let app = TestApp::with_rates(pool, &rates);
    let alice = app.member("alice").await;
    let calls =
        (0..5).map(|_| app.get_as(&alice, "/api/ledger/rates?currency=EUR&date=2026-03-13"));
    for response in futures_util::future::join_all(calls).await {
        response.expect(StatusCode::OK);
    }
    rates.verify().await;
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn today_is_not_kept_before_the_ecb_publishes(pool: PgPool) {
    let rates = rates_stub("4.3").await; // answers with the day asked for
    let app = TestApp::with_rates(pool, &rates);
    let alice = app.member("alice").await;
    let today = app::clock::local_today(app.now());
    let yesterday: NaiveDate = today - TimeDelta::days(1);
    // a stand-in that answers with yesterday's rate, as Frankfurter does before ~16:00
    let early = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"date":"{yesterday}","rates":{{"PLN":4.3}}}}"#),
            "application/json",
        ))
        .mount(&early)
        .await;
    let early_app = TestApp::with_rates(app.db().clone(), &early);
    let member = early_app.session(alice.id).await;
    let response = early_app
        .call(
            axum::http::Method::GET,
            &format!("/api/ledger/rates?currency=EUR&date={today}"),
            Some(&member.access),
            None,
        )
        .await;
    response.expect(StatusCode::OK);
    assert_eq!(app.count("select count(*) from exchange_rates").await, 0);
    // a past day's answer is kept for good
    let past = today - TimeDelta::days(3);
    app.get_as(
        &alice,
        &format!("/api/ledger/rates?currency=EUR&date={past}"),
    )
    .await
    .expect(StatusCode::OK);
    assert_eq!(app.count("select count(*) from exchange_rates").await, 1);
}
