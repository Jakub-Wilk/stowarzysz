//! Account management for administrators, and the statistics shown with each account.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use app::events::{Event, HubMessage};
use axum::http::{Method, StatusCode};
use common::{TestApp, insert_user};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_endpoint_requires_an_administrator(pool: PgPool) {
    let app = TestApp::new(pool);
    let member = app.member("ala").await;
    let id = member.id;
    let endpoints = [
        (Method::GET, "/api/users".to_owned()),
        (Method::POST, "/api/users".to_owned()),
        (Method::GET, format!("/api/users/{id}")),
        (Method::PATCH, format!("/api/users/{id}")),
        (Method::DELETE, format!("/api/users/{id}")),
        (Method::POST, format!("/api/users/{id}/activation-link")),
        (Method::PUT, format!("/api/users/{id}/avatar")),
        (Method::DELETE, format!("/api/users/{id}/avatar")),
        (Method::POST, format!("/api/users/{id}/revoke-sessions")),
    ];
    for (method, path) in endpoints {
        let body = matches!(method, Method::POST | Method::PATCH | Method::PUT).then(|| json!({}));
        let anonymous = app.call(method.clone(), &path, None, body.clone()).await;
        assert_eq!(
            anonymous.status,
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
        let forbidden = app
            .call(method.clone(), &path, Some(&member.token), body)
            .await;
        assert_eq!(forbidden.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn lists_and_retrieves_accounts(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let list = app.get_as(&admin, "/api/users").await;
    let names: Vec<_> = list
        .expect(StatusCode::OK)
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["username"].clone())
        .collect();
    assert_eq!(names, [json!("ala"), json!("boss")]);

    let one = app
        .get_as(&admin, &format!("/api/users/{}", ala.id))
        .await
        .json();
    assert_eq!(one["has_password"], true);
    assert_eq!(
        one["voting"],
        json!({"votes_cast": 0, "average_score": null, "veto_count": 0, "veto_percent": 0.0})
    );
    assert_eq!(
        one["pacts"],
        json!({"won": 0, "lost": 0, "draw": 0, "money_won": 0, "money_lost": 0, "by_kind": {}})
    );
    app.get_as(&admin, "/api/users/999999")
        .await
        .expect(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn creates_administrators_and_refuses_duplicates(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let created = app
        .post_as(
            &admin,
            "/api/users",
            json!({"username": "drugi", "is_superuser": true, "birthday": "1990-05-03"}),
        )
        .await;
    let created = created.expect(StatusCode::CREATED).json();
    assert_eq!(
        (created["is_superuser"].clone(), created["birthday"].clone()),
        (json!(true), json!("1990-05-03"))
    );

    let duplicate = app
        .post_as(&admin, "/api/users", json!({"username": "drugi"}))
        .await;
    duplicate.expect(StatusCode::BAD_REQUEST);
    assert_eq!(
        duplicate.json()["errors"]["username"],
        json!(["Użytkownik o takiej nazwie już istnieje."])
    );

    let invalid = app
        .post_as(&admin, "/api/users", json!({"username": "a  b"}))
        .await;
    invalid.expect(StatusCode::BAD_REQUEST);
    assert_eq!(invalid.code(), "username_invalid");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn patch_changes_only_what_is_given(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    app.member("ola").await;
    let path = format!("/api/users/{}", ala.id);

    let renamed = app
        .patch_as(
            &admin,
            &path,
            json!({"username": "alicja", "birthday": "1991-02-28"}),
        )
        .await;
    let renamed = renamed.expect(StatusCode::OK).json();
    assert_eq!(
        (renamed["username"].clone(), renamed["birthday"].clone()),
        (json!("alicja"), json!("1991-02-28"))
    );

    // Unknown fields (password, id, removed profile fields) are ignored.
    let ignored = app
        .patch_as(
            &admin,
            &path,
            json!({"password": "x", "id": 1, "first_name": "A"}),
        )
        .await;
    let ignored = ignored.expect(StatusCode::OK).json();
    assert_eq!(
        (ignored["id"].clone(), ignored["username"].clone()),
        (json!(ala.id), json!("alicja"))
    );
    assert_eq!(ignored["birthday"], "1991-02-28", "absent is not null");

    let cleared = app
        .patch_as(&admin, &path, json!({"birthday": null}))
        .await
        .json();
    assert_eq!(cleared["birthday"], json!(null));

    let taken = app
        .patch_as(&admin, &path, json!({"username": "ola"}))
        .await;
    taken.expect(StatusCode::BAD_REQUEST);
    assert_eq!(taken.code(), "username_taken");

    let future = app
        .patch_as(&admin, &path, json!({"birthday": "2999-01-01"}))
        .await;
    future.expect(StatusCode::BAD_REQUEST);
    assert_eq!(
        future.json()["errors"]["birthday"],
        json!(["Data urodzin nie może być z przyszłości."])
    );

    app.put_as(&admin, &path, json!({"username": "x"}))
        .await
        .expect(StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn promote_and_demote(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let path = format!("/api/users/{}", ala.id);
    app.patch_as(&admin, &path, json!({"is_superuser": true}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        app.get_as(&ala, "/api/me").await.json()["is_superuser"],
        true
    );
    app.get_as(&ala, "/api/users").await.expect(StatusCode::OK);
    app.patch_as(&admin, &path, json!({"is_superuser": false}))
        .await
        .expect(StatusCode::OK);
    app.get_as(&ala, "/api/users")
        .await
        .expect(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn administrators_cannot_lock_themselves_out(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let path = format!("/api/users/{}", admin.id);
    let response = app
        .patch_as(
            &admin,
            &path,
            json!({"is_active": false, "is_superuser": false}),
        )
        .await;
    response.expect(StatusCode::BAD_REQUEST);
    let errors = &response.json()["errors"];
    assert_eq!(
        errors["is_active"],
        json!(["Nie możesz dezaktywować samego siebie."])
    );
    assert_eq!(
        errors["is_superuser"],
        json!(["Nie możesz odebrać sobie uprawnień administratora."])
    );
    let delete = app.delete_as(&admin, &path).await;
    delete.expect(StatusCode::BAD_REQUEST);
    assert_eq!(delete.code(), "cannot_delete_self");
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn deactivating_signs_the_person_out_everywhere(pool: PgPool) {
    let app = TestApp::new(pool);
    let _listener = app.start_listener().await;
    let mut events = app.ctx.hub.subscribe();
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let laptop = app.session(ala.id).await;

    app.patch_as(
        &admin,
        &format!("/api/users/{}", ala.id),
        json!({"is_active": false}),
    )
    .await
    .expect(StatusCode::OK);
    app.get_as(&ala, "/api/me")
        .await
        .expect(StatusCode::UNAUTHORIZED);
    for refresh in [&ala.refresh, &laptop.refresh] {
        app.call(
            Method::POST,
            "/api/auth/refresh",
            None,
            Some(json!({"refresh": refresh})),
        )
        .await
        .expect(StatusCode::UNAUTHORIZED);
    }
    // Both open event streams are told to close.
    let mut revoked = 0;
    while revoked < 2 {
        let message = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if let HubMessage::Event(envelope) = message
            && matches!(envelope.event, Event::SessionRevoked { .. })
        {
            revoked += 1;
        }
    }
    // Reactivation does not revive old sessions.
    app.patch_as(
        &admin,
        &format!("/api/users/{}", ala.id),
        json!({"is_active": true}),
    )
    .await
    .expect(StatusCode::OK);
    app.get_as(&ala, "/api/me")
        .await
        .expect(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn revoke_sessions_signs_a_user_out(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = app.member("ala").await;
    let response = app
        .call(
            Method::POST,
            &format!("/api/users/{}/revoke-sessions", ala.id),
            Some(&admin.token),
            None,
        )
        .await;
    assert_eq!(
        response.expect(StatusCode::OK).json(),
        json!({"revoked": 1})
    );
    app.get_as(&ala, "/api/me")
        .await
        .expect(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_accounts_without_history_can_be_deleted(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let mistake = app.member("pomyłka").await;
    app.delete_as(&admin, &format!("/api/users/{}", mistake.id))
        .await
        .expect(StatusCode::NO_CONTENT);
    app.get_as(&admin, &format!("/api/users/{}", mistake.id))
        .await
        .expect(StatusCode::NOT_FOUND);
    app.delete_as(&admin, "/api/users/999999")
        .await
        .expect(StatusCode::NOT_FOUND);

    let veteran = app.member("weteran").await;
    sqlx::query(
        "insert into polls (creator_id, title, kind, deadline_at) values ($1, 'x', 'score', now())",
    )
    .bind(veteran.id)
    .execute(app.db())
    .await
    .unwrap();
    let refused = app
        .delete_as(&admin, &format!("/api/users/{}", veteran.id))
        .await;
    refused.expect(StatusCode::CONFLICT);
    assert_eq!(refused.code(), "user_has_history");
    app.get_as(&admin, &format!("/api/users/{}", veteran.id))
        .await
        .expect(StatusCode::OK);
}

/// A closed poll of `kind` where each `(user, ballot, vetoed)` took part.
async fn closed_poll(
    app: &TestApp,
    kind: &str,
    status: &str,
    entries: &[(i64, Option<i64>, bool)],
) {
    let creator = entries[0].0;
    let (closed_at, reason, result) = if status == "closed" {
        ("now()", "'auto'", "'{}'")
    } else {
        ("null", "null", "null")
    };
    let poll: i64 = sqlx::query_scalar(&format!(
        "insert into polls (creator_id, title, kind, status, deadline_at, closed_at, close_reason, result)
         values ($1, 'x', $2, $3, now(), {closed_at}, {reason}, {result}) returning id"
    ))
    .bind(creator)
    .bind(kind)
    .bind(status)
    .fetch_one(app.db())
    .await
    .unwrap();
    for (user, value, vetoed) in entries {
        sqlx::query(
            "insert into poll_participants (poll_id, user_id, ballot, vetoed, voted_at)
             values ($1, $2, case when $3::bigint is null then null else jsonb_build_object('value', $3) end, $4,
                     case when $3::bigint is null then null else now() end)",
        )
        .bind(poll)
        .bind(user)
        .bind(value)
        .bind(vetoed)
        .execute(app.db())
        .await
        .unwrap();
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn voting_stats_count_closed_score_polls_only(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = insert_user(app.db(), "ala").await;
    let ola = insert_user(app.db(), "ola").await;
    closed_poll(
        &app,
        "score",
        "closed",
        &[(ala, Some(5), false), (ola, Some(-5), true)],
    )
    .await;
    closed_poll(
        &app,
        "score",
        "closed",
        &[(ala, Some(2), false), (ola, None, false)],
    )
    .await; // ola never voted
    closed_poll(&app, "score", "closed", &[(ala, Some(0), false)]).await;
    closed_poll(&app, "score", "open", &[(ala, Some(-5), false)]).await; // hidden until it ends
    closed_poll(&app, "nickname", "closed", &[(ala, Some(-5), false)]).await; // another kind

    let ala_stats = app
        .get_as(&admin, &format!("/api/users/{ala}"))
        .await
        .json()["voting"]
        .clone();
    assert_eq!(
        ala_stats,
        json!({"votes_cast": 3, "average_score": 2.33, "veto_count": 0, "veto_percent": 0.0})
    );
    let ola_stats = app
        .get_as(&admin, &format!("/api/users/{ola}"))
        .await
        .json()["voting"]
        .clone();
    assert_eq!(
        ola_stats,
        json!({"votes_cast": 1, "average_score": -5.0, "veto_count": 1, "veto_percent": 100.0})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn pact_stats_come_from_confirmed_claims_and_pact_settlements(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    let ala = insert_user(app.db(), "ala").await;
    let ola = insert_user(app.db(), "ola").await;
    let db = app.db();
    let pact: i64 = sqlx::query_scalar(
        "insert into pacts (creator_id, kind, title, condition, status) values ($1, 'bet', 'x', 'y', 'resolved') returning id",
    )
    .bind(ala)
    .fetch_one(db)
    .await
    .unwrap();
    for (state, verdicts) in [
        ("confirmed", [("won", ala), ("lost", ola)]),
        ("confirmed", [("void", ala), ("void", ola)]),
        ("disputed", [("won", ola), ("lost", ala)]),
    ] {
        let claim: i64 = sqlx::query_scalar(
            "insert into pact_claims (pact_id, proposed_by, result, state) values ($1, $2, '{}', $3) returning id",
        )
        .bind(pact)
        .bind(ala)
        .bind(state)
        .fetch_one(db)
        .await
        .unwrap();
        for (verdict, user) in verdicts {
            sqlx::query(
                "insert into pact_verdicts (claim_id, user_id, verdict) values ($1, $2, $3)",
            )
            .bind(claim)
            .bind(user)
            .bind(verdict)
            .execute(db)
            .await
            .unwrap();
        }
    }
    for (status, item, amount) in [
        ("confirmed", "", 2500),
        ("confirmed", "piwo", 1),
        ("cancelled", "", 999),
    ] {
        let entry: i64 = sqlx::query_scalar(
            "insert into ledger_entries (kind, status, title, occurred_on, source, pact_id) values ('debt', $1, 'x', current_date, 'pact', $2) returning id",
        )
        .bind(status)
        .bind(pact)
        .fetch_one(db)
        .await
        .unwrap();
        sqlx::query("insert into ledger_obligations (entry_id, debtor_id, creditor_id, amount, item) values ($1, $2, $3, $4, $5)")
            .bind(entry)
            .bind(ola)
            .bind(ala)
            .bind(amount)
            .bind(item)
            .execute(db)
            .await
            .unwrap();
    }

    let users = app.get_as(&admin, "/api/users").await.json();
    let stats = |name: &str| {
        users
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["username"] == name)
            .unwrap()["pacts"]
            .clone()
    };
    assert_eq!(
        stats("ala"),
        json!({"won": 1, "lost": 0, "draw": 0, "money_won": 2500, "money_lost": 0, "by_kind": {"bet": {"won": 1, "lost": 0, "draw": 0}}})
    );
    assert_eq!(
        stats("ola"),
        json!({"won": 0, "lost": 1, "draw": 0, "money_won": 0, "money_lost": 2500, "by_kind": {"bet": {"won": 0, "lost": 1, "draw": 0}}})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn me_reports_superuser(pool: PgPool) {
    let app = TestApp::new(pool);
    let admin = app.admin("boss").await;
    assert_eq!(
        app.get_as(&admin, "/api/me").await.json()["is_superuser"],
        true
    );
}
