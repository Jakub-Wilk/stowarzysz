//! Option votes (single, multi, ranked), the open option pool and tie-breaks.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::StatusCode;
use common::{Member, TestApp, subscribe, take_pushes};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn options_poll(
    app: &TestApp,
    creator: &Member,
    kind: &str,
    others: &[&Member],
    extra: Value,
) -> Value {
    let mut body = json!({"kind": kind, "title": "Gdzie na obiad?", "options": ["Pizza", "Sushi", "Kebab"],
                          "participant_ids": others.iter().map(|m| m.id).collect::<Vec<_>>()});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let response = app.post_as(creator, "/api/polls", body).await;
    response.expect(StatusCode::CREATED).json()
}

async fn ballot(app: &TestApp, who: &Member, id: &Value, body: Value) -> common::TestResponse {
    app.put_as(who, &format!("/api/polls/{id}/ballot"), body)
        .await
}

async fn quartet(app: &TestApp) -> (Member, Member, Member, Member) {
    (
        app.member("ala").await,
        app.member("ola").await,
        app.member("ewa").await,
        app.member("iza").await,
    )
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn options_get_ids_and_clean_labels(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    let body = json!({"kind": "multi_choice", "title": "x", "options": ["  Pizza  ", "Sushi\t bar", "Kebab"], "open_options": true, "max_choices": 2});
    let poll = app
        .post_as(&ala, "/api/polls", body)
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        poll["config"],
        json!({"options": [{"id": "o1", "label": "Pizza"}, {"id": "o2", "label": "Sushi bar"}, {"id": "o3", "label": "Kebab"}],
               "open_options": true, "max_choices": 2})
    );
    assert_eq!(poll["actions"]["veto"], false, "no veto for option votes");
    assert_eq!(poll["actions"]["abstain"], true);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn invalid_option_sets_are_refused(pool: PgPool) {
    let app = TestApp::new(pool);
    let ala = app.member("ala").await;
    for (body, code) in [
        (
            json!({"kind": "single_choice", "title": "x", "options": ["a"]}),
            "option_count",
        ),
        (
            json!({"kind": "single_choice", "title": "x", "options": ["a", "A"]}),
            "duplicate_options",
        ),
        (
            json!({"kind": "single_choice", "title": "x", "options": ["a", ""]}),
            "invalid_option",
        ),
        (
            json!({"kind": "ranked_choice", "title": "x", "options": ["a", "b"], "open_options": true}),
            "open_options_not_allowed",
        ),
        (
            json!({"kind": "multi_choice", "title": "x", "options": ["a", "b"], "max_choices": 2}),
            "invalid_max_choices",
        ),
        (
            json!({"kind": "single_choice", "title": "", "options": ["a", "b"]}),
            "title_required",
        ),
        (
            json!({"kind": "single_choice", "title": "x"}),
            "invalid_body",
        ),
    ] {
        let response = app.post_as(&ala, "/api/polls", body.clone()).await;
        response.expect(StatusCode::BAD_REQUEST);
        assert_eq!(response.code(), code, "{body}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn single_choice_counts_picks_and_keeps_them_hidden(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa, iza) = quartet(&app).await;
    let poll = options_poll(&app, &ala, "single_choice", &[&ola, &ewa, &iza], json!({})).await;
    let id = &poll["id"];
    for (body, code) in [
        (json!({"choice": "o9"}), "invalid_choice"),
        (json!({}), "invalid_choice"),
        (json!({"choices": ["o1"]}), "invalid_choice"),
    ] {
        assert_eq!(ballot(&app, &ala, id, body).await.code(), code);
    }
    ballot(&app, &ala, id, json!({"choice": "o1"}))
        .await
        .expect(StatusCode::OK);
    ballot(&app, &ala, id, json!({"choice": "o2"}))
        .await
        .expect(StatusCode::OK); // replaces
    ballot(&app, &ola, id, json!({"choice": "o2"}))
        .await
        .expect(StatusCode::OK);
    let open = app.get_as(&ewa, &format!("/api/polls/{id}")).await.json();
    assert!(
        open["participants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("revealed").is_none())
    );
    app.post_as(&ewa, &format!("/api/polls/{id}/abstain"), json!({}))
        .await
        .expect(StatusCode::OK);
    let closed = ballot(&app, &iza, id, json!({"choice": "o3"}))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(
        closed["result"],
        json!({"votes_cast": 3, "abstain_count": 1, "winners": ["o2"], "tie": false, "tone": "positive",
               "options": [{"id": "o2", "label": "Sushi", "count": 2}, {"id": "o3", "label": "Kebab", "count": 1}, {"id": "o1", "label": "Pizza", "count": 0}]})
    );
    // Without votes, no winner.
    let empty = options_poll(&app, &ala, "single_choice", &[], json!({})).await;
    let closed = app
        .post_as(
            &ala,
            &format!("/api/polls/{}/close", empty["id"]),
            json!({}),
        )
        .await
        .json();
    assert_eq!(
        (
            closed["result"]["winners"].clone(),
            closed["result"]["tone"].clone()
        ),
        (json!([]), json!("neutral"))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn multi_choice_counts_approvals_in_option_order(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ..) = quartet(&app).await;
    let poll = options_poll(
        &app,
        &ala,
        "multi_choice",
        &[&ola],
        json!({"max_choices": 2}),
    )
    .await;
    let id = &poll["id"];
    for body in [
        json!({"choices": []}),
        json!({"choices": ["o1", "o1"]}),
        json!({"choices": ["o7"]}),
    ] {
        assert_eq!(ballot(&app, &ala, id, body).await.code(), "invalid_choices");
    }
    assert_eq!(
        ballot(&app, &ala, id, json!({"choices": ["o1", "o2", "o3"]}))
            .await
            .code(),
        "too_many_choices"
    );
    let mine = ballot(&app, &ala, id, json!({"choices": ["o3", "o1"]}))
        .await
        .json();
    assert_eq!(mine["my_ballot"], json!({"choices": ["o1", "o3"]}));
    let closed = ballot(&app, &ola, id, json!({"choices": ["o3"]}))
        .await
        .json();
    assert_eq!(closed["result"]["winners"], json!(["o3"]));
    assert_eq!(
        closed["result"]["options"][1],
        json!({"id": "o1", "label": "Pizza", "count": 1})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn ranked_choice_uses_schulze_and_accepts_partial_rankings(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa, _) = quartet(&app).await;
    let poll = options_poll(&app, &ala, "ranked_choice", &[&ola, &ewa], json!({})).await;
    let id = &poll["id"];
    for body in [
        json!({"ranking": []}),
        json!({"ranking": ["o1", "o1"]}),
        json!({"ranking": ["o4"]}),
    ] {
        assert_eq!(ballot(&app, &ala, id, body).await.code(), "invalid_ranking");
    }
    let refused = ballot(&app, &ala, id, json!({"new_option": "Tacos"})).await;
    assert_eq!(refused.code(), "options_closed");
    ballot(&app, &ala, id, json!({"ranking": ["o2", "o1", "o3"]}))
        .await
        .expect(StatusCode::OK);
    ballot(&app, &ola, id, json!({"ranking": ["o2"]}))
        .await
        .expect(StatusCode::OK);
    let closed = ballot(&app, &ewa, id, json!({"ranking": ["o1", "o3", "o2"]}))
        .await
        .json();
    let result = &closed["result"];
    assert_eq!(result["winners"], json!(["o2"]));
    assert_eq!(result["condorcet_winner"], "o2");
    assert_eq!(result["pairwise"]["o2"]["o3"], 2);
    assert_eq!(
        result["options"][0],
        json!({"id": "o2", "label": "Sushi", "count": 2, "wins": 2})
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_open_pool_grows_with_votes(pool: PgPool) {
    let app = TestApp::with_push(pool);
    let (ala, ola, ewa, iza) = quartet(&app).await;
    subscribe(app.db(), &[ala.id, ola.id, ewa.id, iza.id]).await;
    let poll = options_poll(
        &app,
        &ala,
        "single_choice",
        &[&ola, &ewa, &iza],
        json!({"open_options": true}),
    )
    .await;
    let id = &poll["id"];
    ballot(&app, &ola, id, json!({"choice": "o1"}))
        .await
        .expect(StatusCode::OK);
    app.post_as(&iza, &format!("/api/polls/{id}/abstain"), json!({}))
        .await
        .expect(StatusCode::OK);
    take_pushes(app.db()).await;

    // ewa votes for a new option: it is added, and those who already voted are told.
    let added = ballot(&app, &ewa, id, json!({"new_option": "  Tacos "}))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(
        added["config"]["options"][3],
        json!({"id": "o4", "label": "Tacos"})
    );
    assert_eq!(added["my_ballot"], json!({"choice": "o4"}));
    let pushes = take_pushes(app.db()).await;
    assert_eq!(
        pushes,
        [(
            ola.id,
            "Nowa opcja w głosowaniu".into(),
            "Dodano opcję „Tacos” w: Gdzie na obiad?. Możesz zmienić swój głos.".into()
        )]
    );

    // An existing label just takes the vote; ewa changing her own vote is not pushed to her.
    let existing = ballot(&app, &ewa, id, json!({"new_option": "pizza"}))
        .await
        .json();
    assert_eq!(existing["my_ballot"], json!({"choice": "o1"}));
    assert_eq!(
        existing["config"]["options"].as_array().unwrap().len(),
        4,
        "Tacos stays"
    );
    ballot(&app, &ewa, id, json!({"new_option": "Burger"}))
        .await
        .expect(StatusCode::OK);
    assert!(
        take_pushes(app.db())
            .await
            .iter()
            .all(|(user, ..)| *user != ewa.id)
    );

    for (body, code) in [
        (json!({"new_option": " "}), "invalid_option"),
        (
            json!({"choice": "o1", "new_option": "Ramen"}),
            "choice_and_new_option",
        ),
    ] {
        assert_eq!(ballot(&app, &ola, id, body).await.code(), code);
    }
    // Outsiders cannot add options.
    let outsider = app.member("obcy").await;
    ballot(&app, &outsider, id, json!({"new_option": "Ramen"}))
        .await
        .expect(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_multi_vote_adds_the_new_option_to_its_picks_and_pools_stop_at_twenty(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ..) = quartet(&app).await;
    let poll = options_poll(
        &app,
        &ala,
        "multi_choice",
        &[&ola],
        json!({"open_options": true}),
    )
    .await;
    let id = &poll["id"];
    let mine = ballot(
        &app,
        &ala,
        id,
        json!({"choices": ["o2"], "new_option": "Tacos"}),
    )
    .await
    .json();
    assert_eq!(mine["my_ballot"], json!({"choices": ["o2", "o4"]}));
    for n in 5..=20 {
        ballot(
            &app,
            &ala,
            id,
            json!({"choices": ["o1"], "new_option": format!("Opcja {n}")}),
        )
        .await
        .expect(StatusCode::OK);
    }
    let full = ballot(
        &app,
        &ala,
        id,
        json!({"choices": ["o1"], "new_option": "Za dużo"}),
    )
    .await;
    full.expect(StatusCode::BAD_REQUEST);
    assert_eq!(full.code(), "option_limit");

    let closed_pool = options_poll(&app, &ala, "single_choice", &[], json!({})).await;
    assert_eq!(
        ballot(
            &app,
            &ala,
            &closed_pool["id"],
            json!({"new_option": "Tacos"})
        )
        .await
        .code(),
        "options_closed"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_tied_poll_gets_one_follow_up_from_its_creator(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ewa, _) = quartet(&app).await;
    let poll = options_poll(
        &app,
        &ala,
        "multi_choice",
        &[&ola, &ewa],
        json!({"open_options": true}),
    )
    .await;
    let id = &poll["id"];
    let tie_break = format!("/api/polls/{id}/tie-break");
    let early = app.post_as(&ala, &tie_break, json!({})).await;
    assert_eq!(early.code(), "poll_open");

    ballot(&app, &ola, id, json!({"choices": ["o1", "o3"]}))
        .await
        .expect(StatusCode::OK);
    let closed = ballot(&app, &ewa, id, json!({"choices": ["o3", "o1"]})).await;
    let closed = closed.json();
    assert_eq!(closed["status"], "open", "ala has not answered");
    let closed = app
        .post_as(&ala, &format!("/api/polls/{id}/close"), json!({}))
        .await
        .json();
    assert_eq!(closed["result"]["winners"], json!(["o1", "o3"]));
    assert_eq!(
        closed["actions"]["tie_break"], true,
        "the creator, who never voted, may call it"
    );
    assert_eq!(
        app.get_as(&ola, &format!("/api/polls/{id}")).await.json()["actions"]["tie_break"],
        false
    );
    let refused = app.post_as(&ola, &tie_break, json!({})).await;
    refused.expect(StatusCode::FORBIDDEN);

    let follow_up = app
        .post_as(&ala, &tie_break, json!({}))
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        follow_up["kind"], "single_choice",
        "a tied multi-select asks to pick one"
    );
    assert_eq!(follow_up["title"], "Dogrywka: Gdzie na obiad?");
    assert_eq!(
        follow_up["config"],
        json!({"options": [{"id": "o1", "label": "Pizza"}, {"id": "o2", "label": "Kebab"}]}),
        "never an open pool"
    );
    assert_eq!(follow_up["tie_break_of"], id.clone());
    assert_eq!(follow_up["participant_count"], 3);

    let again = app
        .post_as(&ala, &tie_break, json!({}))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(again["id"], follow_up["id"]);
    let original = app.get_as(&ala, &format!("/api/polls/{id}")).await.json();
    assert_eq!(
        (
            original["tie_break_poll"].clone(),
            original["actions"]["tie_break"].clone()
        ),
        (follow_up["id"].clone(), json!(false))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn ranked_ties_get_a_ranked_follow_up_and_clear_wins_get_none(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ala, ola, ..) = quartet(&app).await;
    let poll = options_poll(&app, &ala, "ranked_choice", &[&ola], json!({})).await;
    let id = &poll["id"];
    ballot(&app, &ala, id, json!({"ranking": ["o1", "o2"]}))
        .await
        .expect(StatusCode::OK);
    ballot(&app, &ola, id, json!({"ranking": ["o2", "o1"]}))
        .await
        .expect(StatusCode::OK);
    let follow_up = app
        .post_as(&ala, &format!("/api/polls/{id}/tie-break"), json!({}))
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(follow_up["kind"], "ranked_choice");
    let labels: Vec<_> = follow_up["config"]["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["label"].clone())
        .collect();
    assert_eq!(
        labels,
        [json!("Pizza"), json!("Sushi")],
        "only the tied options"
    );

    let clear = options_poll(&app, &ala, "single_choice", &[], json!({})).await;
    ballot(&app, &ala, &clear["id"], json!({"choice": "o1"}))
        .await
        .expect(StatusCode::OK);
    let response = app
        .post_as(
            &ala,
            &format!("/api/polls/{}/tie-break", clear["id"]),
            json!({}),
        )
        .await;
    response.expect(StatusCode::BAD_REQUEST);
    assert_eq!(response.code(), "not_tied");
}
