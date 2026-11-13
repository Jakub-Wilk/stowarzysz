//! Secret Santa: the draw, what each person sees, help requests, ending and history.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use app::events::{Audience, Event, HubMessage};
use axum::http::{Method, StatusCode};
use chrono::TimeDelta;
use common::{Member, TestApp, TestResponse, subscribe, take_pushes};
use serde_json::{Value, json};
use sqlx::PgPool;

const URL: &str = "/api/secret-santa";

/// event, giver, tier, sealed, receiver
type AssignmentRow = (i64, i64, Option<i16>, Option<Vec<u8>>, Option<i64>);

struct Round {
    app: TestApp,
    boss: Member,
    people: Vec<Member>,
}

impl Round {
    async fn new(pool: PgPool) -> Self {
        let app = TestApp::with_push(pool);
        let boss = app.admin("boss").await;
        let mut people = Vec::new();
        for name in ["alice", "bob", "carol", "dave", "erin"] {
            people.push(app.member(name).await);
        }
        Self { app, boss, people }
    }

    fn deadline(&self, days: i64) -> String {
        (self.app.now() + TimeDelta::days(days)).to_rfc3339()
    }

    async fn start_with(
        &self,
        people: &[Member],
        tiers: &[i64],
        mode: Option<&str>,
    ) -> TestResponse {
        let mut body = json!({
            "participant_ids": people.iter().map(|p| p.id).collect::<Vec<_>>(),
            "deadline": self.deadline(10),
            "gift_tiers": tiers,
        });
        if let Some(mode) = mode {
            body["mode"] = json!(mode);
        }
        self.app.post_as(&self.boss, URL, body).await
    }

    /// Everyone, single mode, 100 and 30 zł.
    async fn start(&self) {
        self.start_with(&self.people, &[100, 30], None)
            .await
            .expect(StatusCode::CREATED);
    }

    async fn state(&self, who: &Member) -> Value {
        self.app
            .get_as(who, URL)
            .await
            .expect(StatusCode::OK)
            .json()
    }

    /// giver id -> victim id (single mode).
    async fn pairing(&self) -> BTreeMap<i64, i64> {
        let mut drawn = BTreeMap::new();
        for p in &self.people {
            let victim = self.state(p).await["my_victim"]["id"].as_i64().unwrap();
            drawn.insert(p.id, victim);
        }
        drawn
    }

    /// giver id -> {amount: victim id} (per-tier mode).
    async fn tier_pairing(&self, people: &[Member]) -> BTreeMap<i64, BTreeMap<i64, i64>> {
        let mut drawn = BTreeMap::new();
        for p in people {
            let victims = self.state(p).await["my_tier_victims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    (
                        t["amount"].as_i64().unwrap(),
                        t["victim"]["id"].as_i64().unwrap(),
                    )
                })
                .collect();
            drawn.insert(p.id, victims);
        }
        drawn
    }

    fn by_id(&self, id: i64) -> &Member {
        self.people.iter().find(|p| p.id == id).unwrap()
    }

    async fn end(&self) -> TestResponse {
        self.app
            .post_as(&self.boss, &format!("{URL}/end"), json!({}))
            .await
    }

    async fn history(&self) -> Value {
        self.app
            .get_as(&self.people[0], &format!("{URL}/history"))
            .await
            .expect(StatusCode::OK)
            .json()
    }

    async fn ask(&self, giver: &Member, victim: i64) -> TestResponse {
        self.app
            .post_as(giver, &format!("{URL}/help"), json!({"victim_id": victim}))
            .await
    }

    async fn answer(&self, victim: &Member, request: i64, ideas: Value) -> TestResponse {
        self.app
            .put_as(
                victim,
                &format!("{URL}/help/{request}"),
                json!({"ideas": ideas}),
            )
            .await
    }

    async fn events(&self) -> i64 {
        self.app.count("select count(*) from santa_events").await
    }
}

// --- access -----------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_endpoint_requires_authentication(pool: PgPool) {
    let app = TestApp::new(pool);
    for (method, path) in [
        (Method::GET, URL.to_owned()),
        (Method::POST, URL.to_owned()),
        (Method::PATCH, URL.to_owned()),
        (Method::POST, format!("{URL}/end")),
        (Method::GET, format!("{URL}/history")),
        (Method::PATCH, format!("{URL}/gifts/1")),
        (Method::POST, format!("{URL}/help")),
        (Method::PUT, format!("{URL}/help/1")),
    ] {
        let response = app.call(method.clone(), &path, None, Some(json!({}))).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn changes_are_for_superusers_only(pool: PgPool) {
    let round = Round::new(pool).await;
    let member = &round.people[0];
    for (method, path) in [
        (Method::POST, URL.to_owned()),
        (Method::PATCH, URL.to_owned()),
        (Method::POST, format!("{URL}/end")),
    ] {
        let response = round
            .app
            .call(method.clone(), &path, Some(&member.token), Some(json!({})))
            .await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn nothing_is_on_by_default(pool: PgPool) {
    let round = Round::new(pool).await;
    assert_eq!(
        round.state(&round.people[0]).await,
        json!({
            "active": false,
            "event": null,
            "my_victim": null,
            "my_tier_victims": [],
            "my_help_requests": [],
            "help_requests_for_me": [],
        })
    );
}

// --- the draw ---------------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_draw_is_one_valid_circle(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let drawn = round.pairing().await;
    let everyone: BTreeSet<i64> = round.people.iter().map(|p| p.id).collect();
    assert_eq!(drawn.keys().copied().collect::<BTreeSet<_>>(), everyone);
    assert_eq!(drawn.values().copied().collect::<BTreeSet<_>>(), everyone);
    for (giver, receiver) in &drawn {
        assert_ne!(giver, receiver);
        assert_ne!(drawn[receiver], *giver, "no swaps");
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn every_giver_is_pushed_whom_they_drew(pool: PgPool) {
    let round = Round::new(pool).await;
    let ids: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    subscribe(round.app.db(), &ids).await;
    let _listener = round.app.start_listener().await;
    let mut rx = round.app.ctx.hub.subscribe();
    round.start().await;
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(HubMessage::Event(envelope)) = rx.recv().await {
                return envelope;
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(event.event, Event::SantaUpdated { .. }));
    assert_eq!(event.audience, Audience::All);
    let pushes = take_pushes(round.app.db()).await;
    assert_eq!(pushes.len(), round.people.len());
    let drawn = round.pairing().await;
    for (giver, title, body) in pushes {
        assert_eq!(title, "Secret Santa — wylosowano!");
        let victim = &round.by_id(drawn[&giver]).username;
        assert_eq!(
            body,
            format!("Obdarowujesz: {victim}. Kwoty: 100 zł, 30 zł.")
        );
    }
    let urls: Vec<String> = sqlx::query_scalar("select payload->>'url' from push_outbox")
        .fetch_all(round.app.db())
        .await
        .unwrap();
    assert!(urls.iter().all(|u| u == "/secret-santa"));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_pairing_is_sealed_in_the_database(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let rows: Vec<AssignmentRow> = sqlx::query_as(
        "select event_id, giver_id, tier_index, sealed, receiver_id from santa_assignments",
    )
    .fetch_all(round.app.db())
    .await
    .unwrap();
    let drawn = round.pairing().await;
    for (event, giver, tier, sealed, receiver) in &rows {
        assert_eq!(*receiver, None);
        let sealed = sealed.as_ref().unwrap();
        for p in &round.people {
            assert!(!sealed.windows(8).any(|w| w == p.id.to_be_bytes()));
        }
        let opened = round.app.ctx.santa.open(*event, *giver, *tier, sealed);
        assert_eq!(opened, Ok(drawn[giver]));
        let other = round.people.iter().find(|p| p.id != *giver).unwrap();
        assert!(
            round
                .app
                .ctx
                .santa
                .open(*event, other.id, *tier, sealed)
                .is_err(),
            "a seal copied under another giver does not open"
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn victims_are_hidden_from_everyone_else(pool: PgPool) {
    let round = Round::new(pool).await;
    round
        .start_with(&round.people[..3], &[50], None)
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(round.state(&round.boss).await["my_victim"], Value::Null);
    let outsider = round.state(&round.people[4]).await;
    assert_eq!(outsider["active"], true);
    assert_eq!(outsider["my_victim"], Value::Null);
    assert_eq!(outsider["event"]["is_participant"], false);
    assert_eq!(
        outsider["event"]["participants"].as_array().unwrap().len(),
        3
    );
    assert_eq!(
        round.state(&round.people[0]).await["event"]["is_participant"],
        true
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn starting_is_validated(pool: PgPool) {
    let round = Round::new(pool).await;
    let everyone: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    let cases = [
        (json!({"gift_tiers": []}), "tiers_required"),
        (json!({"gift_tiers": [0]}), "invalid_tier"),
        (json!({"gift_tiers": [-5]}), "invalid_tier"),
        (
            json!({"deadline": "2000-01-01T00:00:00Z"}),
            "deadline_in_past",
        ),
        (
            json!({"participant_ids": &everyone[..2]}),
            "too_few_participants",
        ),
        (
            json!({"participant_ids": [everyone[0], everyone[1], 99_999]}),
            "unknown_participant",
        ),
        (
            json!({"gift_tiers": [100, 50, 30], "mode": "per_tier", "participant_ids": &everyone[..3]}),
            "too_many_tiers",
        ),
    ];
    for (over, code) in cases {
        let mut body = json!({"participant_ids": everyone, "deadline": round.deadline(10), "gift_tiers": [50]});
        body.as_object_mut()
            .unwrap()
            .extend(over.as_object().unwrap().clone());
        let response = round.app.post_as(&round.boss, URL, body).await;
        assert_eq!(
            (response.status, response.code()),
            (StatusCode::BAD_REQUEST, code.to_owned()),
            "{over}"
        );
    }
    let unknown_mode = round.start_with(&round.people, &[50], Some("nope")).await;
    assert_eq!(unknown_mode.status, StatusCode::BAD_REQUEST);
    assert_eq!(round.events().await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_one_round_runs_at_a_time(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let second = round.start_with(&round.people, &[50], None).await;
    assert_eq!(
        (second.status, second.code()),
        (StatusCode::CONFLICT, "santa_active".into())
    );
}

// --- per-tier rounds --------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_per_tier_round_gives_a_different_victim_per_tier(pool: PgPool) {
    let round = Round::new(pool).await;
    round
        .start_with(&round.people, &[100, 50, 30], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
    let drawn = round.tier_pairing(&round.people).await;
    let everyone: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    for (giver, victims) in &drawn {
        assert_eq!(victims.keys().copied().collect::<Vec<_>>(), [30, 50, 100]);
        assert_eq!(victims.values().collect::<BTreeSet<_>>().len(), 3);
        assert!(!victims.values().any(|v| v == giver));
    }
    for amount in [100, 50, 30] {
        let mut receivers: Vec<i64> = drawn.values().map(|v| v[&amount]).collect();
        receivers.sort_unstable();
        assert_eq!(receivers, everyone, "everyone receives each tier");
        for (giver, victims) in &drawn {
            assert_ne!(drawn[&victims[&amount]][&amount], *giver, "no swaps");
        }
    }
    for p in &round.people {
        assert_eq!(round.state(p).await["my_victim"], Value::Null);
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_per_tier_round_as_outsiders_see_it(pool: PgPool) {
    let round = Round::new(pool).await;
    round
        .start_with(&round.people[..4], &[100, 30], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
    let outsider = round.state(&round.people[4]).await;
    assert_eq!(outsider["event"]["mode"], "per_tier");
    assert_eq!(outsider["my_tier_victims"], json!([]));
    assert_eq!(
        outsider["event"]["participants"].as_array().unwrap().len(),
        4,
        "listed once, not once per tier"
    );
    assert_eq!(round.state(&round.boss).await["my_tier_victims"], json!([]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn single_mode_is_the_default(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let state = round.state(&round.people[0]).await;
    assert_eq!(state["event"]["mode"], "single");
    assert!(state["my_victim"].is_object());
    assert_eq!(state["my_tier_victims"], json!([]));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_per_tier_push_lists_each_victim(pool: PgPool) {
    let round = Round::new(pool).await;
    let ids: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    subscribe(round.app.db(), &ids).await;
    round
        .start_with(&round.people, &[100, 30], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
    let pushes = take_pushes(round.app.db()).await;
    assert_eq!(pushes.len(), round.people.len());
    let drawn = round.tier_pairing(&round.people).await;
    for (giver, _, body) in pushes {
        let name = |amount: i64| round.by_id(drawn[&giver][&amount]).username.clone();
        assert_eq!(
            body,
            format!(
                "Twoi podopieczni: {} (100 zł), {} (30 zł).",
                name(100),
                name(30)
            )
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn three_people_allow_two_tiers(pool: PgPool) {
    let round = Round::new(pool).await;
    let three = &round.people[..3];
    let too_many = round
        .start_with(three, &[100, 50, 30], Some("per_tier"))
        .await;
    assert_eq!(too_many.status, StatusCode::BAD_REQUEST);
    assert_eq!(round.events().await, 0);
    round
        .start_with(three, &[100, 50], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_per_tier_update_changes_amounts_not_the_draw(pool: PgPool) {
    let round = Round::new(pool).await;
    round
        .start_with(&round.people, &[100, 30], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
    let before = round.tier_pairing(&round.people).await;
    round
        .app
        .patch_as(&round.boss, URL, json!({"gift_tiers": [80, 20]}))
        .await
        .expect(StatusCode::OK);
    let after = round.tier_pairing(&round.people).await;
    for (giver, victims) in &before {
        assert_eq!(after[giver][&80], victims[&100]);
        assert_eq!(after[giver][&20], victims[&30]);
    }
    for tiers in [json!([80]), json!([80, 20, 5])] {
        let response = round
            .app
            .patch_as(&round.boss, URL, json!({"gift_tiers": tiers}))
            .await;
        assert_eq!(
            (response.status, response.code()),
            (StatusCode::BAD_REQUEST, "tier_count_fixed".into())
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn ending_a_per_tier_round_gives_one_gift_per_pairing(pool: PgPool) {
    let round = Round::new(pool).await;
    round
        .start_with(&round.people, &[100, 30], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
    let drawn = round.tier_pairing(&round.people).await;
    round.end().await.expect(StatusCode::OK);
    let history = round.history().await;
    let event = &history["items"][0];
    assert_eq!(event["mode"], "per_tier");
    let pairings = event["pairings"].as_array().unwrap();
    assert_eq!(pairings.len(), round.people.len() * 2);
    for pairing in pairings {
        let gifts = pairing["gifts"].as_array().unwrap();
        assert_eq!(gifts.len(), 1);
        let giver = pairing["giver"]["id"].as_i64().unwrap();
        let amount = gifts[0]["amount"].as_i64().unwrap();
        assert_eq!(
            drawn[&giver][&amount],
            pairing["receiver"]["id"].as_i64().unwrap()
        );
    }
}

// --- updating, ending, history ----------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn an_update_keeps_the_draw_and_tells_the_givers(pool: PgPool) {
    let round = Round::new(pool).await;
    subscribe(round.app.db(), &[round.people[0].id]).await;
    round.start().await;
    let before = round.pairing().await;
    take_pushes(round.app.db()).await;
    let state = round
        .app
        .patch_as(
            &round.boss,
            URL,
            json!({"deadline": round.deadline(30), "gift_tiers": [200, 50, 20]}),
        )
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(state["event"]["gift_tiers"], json!([200, 50, 20]));
    assert_eq!(round.pairing().await, before);
    assert_eq!(
        take_pushes(round.app.db()).await,
        [(
            round.people[0].id,
            "Secret Santa — zmiana kwot".into(),
            "Nowe kwoty: 200 zł, 50 zł, 20 zł.".into()
        )]
    );
    // Only the deadline: nobody is told.
    round
        .app
        .patch_as(&round.boss, URL, json!({"deadline": round.deadline(40)}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(take_pushes(round.app.db()).await, Vec::new());
    let past = round
        .app
        .patch_as(
            &round.boss,
            URL,
            json!({"deadline": "2000-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(past.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn updating_or_ending_needs_a_round(pool: PgPool) {
    let round = Round::new(pool).await;
    let update = round
        .app
        .patch_as(&round.boss, URL, json!({"gift_tiers": [10]}))
        .await;
    assert_eq!(update.status, StatusCode::NOT_FOUND);
    assert_eq!(round.end().await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn ending_reveals_the_pairings_in_history(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let drawn = round.pairing().await;
    assert_eq!(round.history().await["items"], json!([]));
    let state = round.end().await.expect(StatusCode::OK).json();
    assert_eq!(state["active"], false);
    assert_eq!(
        round
            .app
            .count("select count(*) from santa_assignments where sealed is not null")
            .await,
        0
    );
    let history = round.history().await;
    let event = &history["items"][0];
    assert_eq!(event["gift_tiers"], json!([100, 30]));
    let shown: BTreeMap<i64, i64> = event["pairings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["giver"]["id"].as_i64().unwrap(),
                p["receiver"]["id"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(shown, drawn);
    for pairing in event["pairings"].as_array().unwrap() {
        let amounts: Vec<i64> = pairing["gifts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["amount"].as_i64().unwrap())
            .collect();
        assert_eq!(amounts, [100, 30]);
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_new_round_can_start_after_one_ends(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    round.end().await.expect(StatusCode::OK);
    round.start().await;
    assert_eq!(round.events().await, 2);
    for _ in 0..11 {
        round.end().await.expect(StatusCode::OK);
        round.start().await;
    }
    let first = round.history().await;
    assert_eq!(first["items"].as_array().unwrap().len(), 10);
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = round
        .app
        .get_as(&round.people[0], &format!("{URL}/history?cursor={cursor}"))
        .await
        .json();
    assert_eq!(second["items"].as_array().unwrap().len(), 2);
    assert_eq!(second["next_cursor"], Value::Null);
}

// --- gift notes -------------------------------------------------------------------------------

/// A gift `giver` gave in an ended round.
async fn ended_gift(round: &Round, giver: &Member) -> i64 {
    round.start().await;
    round.end().await.expect(StatusCode::OK);
    sqlx::query_scalar(
        "select g.id from santa_gifts g join santa_assignments a on a.id = g.assignment_id
         where a.giver_id = $1 order by g.id limit 1",
    )
    .bind(giver.id)
    .fetch_one(round.app.db())
    .await
    .unwrap()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_giver_and_superusers_can_note_a_gift(pool: PgPool) {
    let round = Round::new(pool).await;
    let giver = &round.people[0];
    let gift = ended_gift(&round, giver).await;
    let path = format!("{URL}/gifts/{gift}");
    let noted = round
        .app
        .patch_as(giver, &path, json!({"note": " Książka "}))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(noted["note"], "Książka");
    round
        .app
        .patch_as(&round.boss, &path, json!({"note": "Kubek"}))
        .await
        .expect(StatusCode::OK);
    let history = round.history().await;
    let notes: Vec<&str> = history["items"][0]["pairings"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["gifts"].as_array().unwrap())
        .filter(|g| g["id"] == gift)
        .map(|g| g["note"].as_str().unwrap())
        .collect();
    assert_eq!(notes, ["Kubek"]);
    let other = round
        .app
        .patch_as(&round.people[1], &path, json!({"note": "x"}))
        .await;
    assert_eq!(
        (other.status, other.code()),
        (StatusCode::FORBIDDEN, "not_your_gift".into())
    );
    let long = round
        .app
        .patch_as(giver, &path, json!({"note": "x".repeat(301)}))
        .await;
    assert_eq!(long.status, StatusCode::BAD_REQUEST);
    let missing = round
        .app
        .patch_as(
            &round.boss,
            &format!("{URL}/gifts/99999"),
            json!({"note": "x"}),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn notes_wait_for_the_round_to_end(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let gift: i64 = sqlx::query_scalar(
        "insert into santa_gifts (assignment_id, amount)
         select id, 5 from santa_assignments limit 1 returning id",
    )
    .fetch_one(round.app.db())
    .await
    .unwrap();
    let response = round
        .app
        .patch_as(
            &round.boss,
            &format!("{URL}/gifts/{gift}"),
            json!({"note": "x"}),
        )
        .await;
    assert_eq!(
        (response.status, response.code()),
        (StatusCode::BAD_REQUEST, "santa_running".into())
    );
}

// --- help requests ----------------------------------------------------------------------------

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn help_reaches_the_victim_anonymously(pool: PgPool) {
    let round = Round::new(pool).await;
    let ids: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    subscribe(round.app.db(), &ids).await;
    round.start().await;
    let giver = &round.people[0];
    let victim = round.by_id(round.pairing().await[&giver.id]);
    take_pushes(round.app.db()).await;
    let _listener = round.app.start_listener().await;
    let mut rx = round.app.ctx.hub.subscribe();

    let state = round
        .ask(giver, victim.id)
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(
        state["my_help_requests"],
        json!([{"victim_id": victim.id, "amount": null, "pending": true, "ideas": []}])
    );
    let pushes = take_pushes(round.app.db()).await;
    assert_eq!(
        pushes,
        [(
            victim.id,
            "Secret Santa — prośba o pomoc".into(),
            "Twój Secret Santa nie wie, co Ci kupić, i prosi o pomoc z prezentem. Podsuń kilka pomysłów!".into()
        )]
    );
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(HubMessage::Event(envelope)) = rx.recv().await {
                return envelope;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        event.audience,
        Audience::User(victim.id),
        "only the victim hears"
    );

    let mine = round.state(victim).await["help_requests_for_me"].clone();
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let keys: BTreeSet<&str> = mine[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["id", "amount", "pending", "ideas"]),
        "nothing about the giver"
    );
    for other in std::iter::once(&round.boss).chain(
        round
            .people
            .iter()
            .filter(|p| p.id != giver.id && p.id != victim.id),
    ) {
        let state = round.state(other).await;
        assert_eq!(state["help_requests_for_me"], json!([]));
        assert!(
            state["my_help_requests"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["victim_id"] != victim.id)
        );
    }
    let columns: Vec<String> = sqlx::query_scalar(
        "select column_name::text from information_schema.columns where table_name = 'santa_help_requests'",
    )
    .fetch_all(round.app.db())
    .await
    .unwrap();
    assert!(
        !columns
            .iter()
            .any(|c| c.contains("giver") || c.contains("assignment"))
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn the_victim_answers_and_the_giver_is_told(pool: PgPool) {
    let round = Round::new(pool).await;
    let ids: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    subscribe(round.app.db(), &ids).await;
    round.start().await;
    let giver = &round.people[0];
    let victim = round.by_id(round.pairing().await[&giver.id]);
    round
        .ask(giver, victim.id)
        .await
        .expect(StatusCode::CREATED);
    let request = round.state(victim).await["help_requests_for_me"][0]["id"]
        .as_i64()
        .unwrap();
    take_pushes(round.app.db()).await;

    let state = round
        .answer(victim, request, json!([" Książka ", "", "Kubek"]))
        .await
        .expect(StatusCode::OK)
        .json();
    assert_eq!(
        state["help_requests_for_me"][0]["ideas"],
        json!(["Książka", "Kubek"])
    );
    assert_eq!(
        take_pushes(round.app.db()).await,
        [(
            giver.id,
            "Secret Santa — są podpowiedzi".into(),
            format!("Masz nowe pomysły na prezent dla: {}.", victim.username)
        )]
    );
    assert_eq!(
        round.state(giver).await["my_help_requests"],
        json!([{"victim_id": victim.id, "amount": null, "pending": false, "ideas": ["Książka", "Kubek"]}])
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn asking_again_waits_for_an_answer(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let giver = &round.people[0];
    let victim = round.by_id(round.pairing().await[&giver.id]);
    round
        .ask(giver, victim.id)
        .await
        .expect(StatusCode::CREATED);
    let waiting = round.ask(giver, victim.id).await;
    assert_eq!(
        (waiting.status, waiting.code()),
        (StatusCode::BAD_REQUEST, "help_pending".into())
    );
    let request = round.state(victim).await["help_requests_for_me"][0]["id"]
        .as_i64()
        .unwrap();
    round
        .answer(victim, request, json!(["Skarpetki"]))
        .await
        .expect(StatusCode::OK);
    let state = round
        .ask(giver, victim.id)
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert_eq!(state["my_help_requests"][0]["pending"], true);
    assert_eq!(
        state["my_help_requests"][0]["ideas"],
        json!(["Skarpetki"]),
        "earlier ideas stay"
    );
    assert_eq!(
        round
            .app
            .count("select count(*) from santa_help_requests")
            .await,
        1
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn you_can_only_ask_about_your_own_victim(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let giver = &round.people[0];
    let drawn = round.pairing().await;
    let not_mine = round
        .people
        .iter()
        .find(|p| p.id != giver.id && p.id != drawn[&giver.id])
        .unwrap();
    for (who, victim) in [
        (giver, not_mine.id),
        (&round.boss, drawn[&giver.id]),
        (giver, 99_999),
    ] {
        let response = round.ask(who, victim).await;
        assert_eq!(
            (response.status, response.code()),
            (StatusCode::BAD_REQUEST, "not_your_victim".into())
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn only_the_victim_can_answer(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let giver = &round.people[0];
    round
        .ask(giver, round.pairing().await[&giver.id])
        .await
        .expect(StatusCode::CREATED);
    let request: i64 = sqlx::query_scalar("select id from santa_help_requests")
        .fetch_one(round.app.db())
        .await
        .unwrap();
    for (who, id) in [(giver, request), (&round.boss, request), (giver, 99_999)] {
        assert_eq!(
            round.answer(who, id, json!(["x"])).await.status,
            StatusCode::NOT_FOUND
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn answers_are_validated(pool: PgPool) {
    let round = Round::new(pool).await;
    round.start().await;
    let giver = &round.people[0];
    let victim = round.by_id(round.pairing().await[&giver.id]);
    round
        .ask(giver, victim.id)
        .await
        .expect(StatusCode::CREATED);
    let request = round.state(victim).await["help_requests_for_me"][0]["id"]
        .as_i64()
        .unwrap();
    for ideas in [
        json!([]),
        json!(["", "  "]),
        json!(vec!["x"; 6]),
        json!(["x".repeat(201)]),
    ] {
        assert_eq!(
            round.answer(victim, request, ideas.clone()).await.status,
            StatusCode::BAD_REQUEST,
            "{ideas}"
        );
    }
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn help_needs_a_running_round(pool: PgPool) {
    let round = Round::new(pool).await;
    assert_eq!(
        round.ask(&round.people[0], round.people[1].id).await.status,
        StatusCode::NOT_FOUND
    );
    round.start().await;
    let giver = &round.people[0];
    let victim = round.by_id(round.pairing().await[&giver.id]);
    round
        .ask(giver, victim.id)
        .await
        .expect(StatusCode::CREATED);
    let request = round.state(victim).await["help_requests_for_me"][0]["id"]
        .as_i64()
        .unwrap();
    round.end().await.expect(StatusCode::OK);
    assert_eq!(
        round.answer(victim, request, json!(["x"])).await.status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn per_tier_help_is_per_pairing(pool: PgPool) {
    let round = Round::new(pool).await;
    let ids: Vec<i64> = round.people.iter().map(|p| p.id).collect();
    subscribe(round.app.db(), &ids).await;
    round
        .start_with(&round.people, &[100, 30], Some("per_tier"))
        .await
        .expect(StatusCode::CREATED);
    let giver = &round.people[0];
    let victims = round.tier_pairing(std::slice::from_ref(giver)).await[&giver.id].clone();
    take_pushes(round.app.db()).await;
    for victim in victims.values() {
        round.ask(giver, *victim).await.expect(StatusCode::CREATED);
    }
    let state = round.state(giver).await;
    let mut amounts: Vec<i64> = state["my_help_requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["amount"].as_i64().unwrap())
        .collect();
    amounts.sort_unstable();
    assert_eq!(amounts, [30, 100]);
    assert!(
        take_pushes(round.app.db())
            .await
            .iter()
            .any(|(_, _, body)| body.contains("prezentem za 100 zł"))
    );

    // The 100 zł victim answers; only their giver for that tier hears about it.
    let victim = round.by_id(victims[&100]);
    let request = round.state(victim).await["help_requests_for_me"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["amount"] == 100)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    round
        .answer(victim, request, json!(["Gra"]))
        .await
        .expect(StatusCode::OK);
    let told: Vec<i64> = take_pushes(round.app.db())
        .await
        .into_iter()
        .map(|(u, _, _)| u)
        .collect();
    assert_eq!(told, [giver.id]);
    let mine: BTreeMap<i64, Value> = round.state(giver).await["my_help_requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["amount"].as_i64().unwrap(), r.clone()))
        .collect();
    assert_eq!(mine[&100]["ideas"], json!(["Gra"]));
    assert_eq!(mine[&30]["pending"], true);
}
