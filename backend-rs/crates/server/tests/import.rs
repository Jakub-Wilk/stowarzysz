//! `import-django`: a small Django database (the real schema, `fixtures/django_schema.sql`) is
//! imported, then used through the API as the new backend. No table names overlap, so the old
//! schema lives next to the new one in the test's database.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use app::import::{Options, Report, run};
use axum::http::{Method, StatusCode};
use chrono::TimeDelta;
use common::{TestApp, png};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Made by Django: `make_password("s3cret-pass-123", salt="fixedsaltfixedsalt12")`.
const DJANGO_HASH: &str =
    "pbkdf2_sha256$1500000$fixedsaltfixedsalt12$Ko7BwSPmIe4CpYnNmje8vTQd0RyY7kihGbAZ0kUpvM8=";
const AVATAR: &str = "avatars/0123456789abcdef0123456789abcdef.webp";
const PICTURE: &str = "pact_attachments/fedcba9876543210fedcba9876543210.webp";
const OLD_SECRET: &str = "the-old-django-secret";

/// The old database: four members and one not activated; a vote, a pact settled by a Sejmik
/// ruling (with its ledger debt), a running resolution, an expense, a payment, an active Secret
/// Santa round sealed with `santa_key`, refresh tokens, a push subscription.
async fn seed(pool: &PgPool, santa_key: &str) {
    sqlx::raw_sql(include_str!("fixtures/django_schema.sql"))
        .execute(pool)
        .await
        .unwrap();
    let fernet = fernet::Fernet::new(santa_key).unwrap();
    let seal = |giver: i64, receiver: i64| fernet.encrypt(format!("{giver}:{receiver}").as_bytes());
    let sql = format!(
        r#"
insert into django_migrations (app, name, applied) values
  ('accounts', '0008_user_birthday', now()), ('ledger', '0011_categories', now()),
  ('pacts', '0004_open_to_all_and_judgments', now()), ('push', '0001_initial', now()),
  ('secretsanta', '0003_help_request', now()), ('voting', '0005_pollparticipant_last_reaction', now());
insert into accounts_user (id, password, is_superuser, username, is_active, date_joined, avatar, birthday) values
  (1, '{DJANGO_HASH}', true, 'ala', true, '2025-01-01', '{AVATAR}', '1990-05-03'),
  (2, '{DJANGO_HASH}', false, 'ola', true, '2025-01-02', '', null),
  (3, '{DJANGO_HASH}', false, 'ewa', true, '2025-01-03', '', null),
  (4, '{DJANGO_HASH}', false, 'iza', true, '2025-01-04', '', null),
  (5, '!unusable', false, 'nowy', true, '2025-01-05', '', null);
insert into accounts_activationtoken (id, token_hash, created_at, expires_at, user_id) values
  (1, encode(sha256('raw-token'::bytea), 'hex'), now(), '2027-06-01', 5);
insert into token_blacklist_outstandingtoken (id, token, created_at, expires_at, user_id, jti) values
  (1, 'x', now(), '2027-06-01', 1, 'live'),
  (2, 'x', now(), '2027-06-01', 1, 'blacklisted'),
  (3, 'x', now(), '2026-01-01', 1, 'expired');
insert into token_blacklist_blacklistedtoken (id, blacklisted_at, token_id) values (1, now(), 2);
insert into push_pushsubscription (id, endpoint, p256dh, auth, user_agent, created_at, user_id) values
  (1, 'https://push.test/1', 'k', 'a', 'Firefox', now(), 1);
insert into ledger_exchangerate (id, currency, requested_on, rate, rate_date) values
  (1, 'EUR', '2026-09-27', 4.3718, '2026-09-25');

insert into voting_poll (id, title, kind, config, status, close_reason, result, created_at, closed_at,
                         creator_id, proposed_avatar, reminders_sent, previous_avatar) values
  (10, 'Pizza?', 'score', '{{}}', 'closed', 'auto',
   '{{"score": 3.0, "tone": "positive", "veto_count": 0, "vetoed": false, "votes_cast": 2}}',
   '2026-01-01', '2026-01-02', 1, '', 0, ''),
  (11, 'Spór o umowę: Zakład', 'pact_ruling',
   '{{"proposal_id": 31, "pact_id": 20, "pact_title": "Zakład", "claim": {{"winner": "host"}}}}',
   'closed', 'auto',
   '{{"votes_cast": 1, "upheld": 1, "rejected": 0, "approved": true, "tone": "positive", "applied": true}}',
   '2026-02-01', '2026-02-02', 2, '', 0, '');
insert into voting_pollparticipant (id, ballot, vetoed, voted_at, poll_id, user_id, abstained, reason, last_reaction) values
  (1, '{{"value": 3}}', false, '2026-01-01', 10, 1, false, '', '🔥'),
  (2, '{{"value": 3}}', false, '2026-01-01', 10, 2, false, '', ''),
  (3, '{{"upheld": true}}', false, '2026-02-01', 11, 3, false, '', '');

insert into pacts_pact (id, kind, title, condition, notes, due_at, config, status, outcome, created_at,
                        resolved_at, creator_id) values
  (20, 'bet', 'Zakład', 'Wygram', '', null, '{{}}', 'resolved',
   '{{"wagers": [{{"result": {{"winner": "host"}}, "participant_id": 22}}]}}', '2026-01-10', '2026-02-02', 1),
  (23, 'resolution', 'Rzucę palenie', 'Bez papierosa', '', '2027-06-01', '{{}}', 'active',
   null, '2026-01-10', null, 3);
insert into pacts_pactparticipant (id, role, side, stake_amount, stake_note, state, created_at, pact_id, user_id) values
  (21, 'host', '', null, '', 'active', '2026-01-10', 20, 1),
  (22, 'opponent', '', 1500, '', 'settled', '2026-01-10', 20, 2),
  (24, 'host', '', null, '', 'active', '2026-01-10', 23, 3);
insert into pacts_outcomeproposal (id, result, state, created_at, decided_at, proposed_by_id, pact_id,
                                   wager_id, ruling_poll_id, verdicts) values
  (31, '{{"winner": "host"}}', 'confirmed', '2026-01-20', '2026-02-02', 1, 20, 22, 11,
   '{{"1": "won", "2": "lost"}}');
insert into pacts_pactattachment (id, image, caption, created_at, pact_id, uploaded_by_id) values
  (25, '{PICTURE}', 'serwetka', '2026-01-10', 20, 1);

insert into ledger_ledgerentry (id, amount, currency, title, source_type, source_id, created_at, kind,
    status, decided_at, item, note, category, occurred_on, base_amount, details, created_by_id,
    updated_at, version) values
  (40, 1500, 'PLN', 'Zakład', 'pact', 20, '2026-02-02', 'debt', 'confirmed', null, '', '', '',
   '2026-02-02', 1500, '{{"debts": [{{"debtor_id": 2, "creditor_id": 1, "amount": 1500, "item": ""}}]}}',
   null, '2026-02-02', 1),
  (41, 3000, 'PLN', 'Pizza', '', null, '2026-03-01', 'expense', 'confirmed', null, '', '', 'food',
   '2026-03-01', 3000,
   '{{"items": [{{"name": "Pizza", "split": "equal", "amount": 3000, "shares": [
      {{"user_id": 1, "weight": 1}}, {{"user_id": 2, "weight": 1}}, {{"user_id": 3, "weight": 1}}]}}],
     "payers": [{{"user_id": 1, "amount": 3000}}]}}', 1, '2026-03-01', 1),
  (42, 1000, 'PLN', 'Spłata', '', null, '2026-03-02', 'payment', 'confirmed', '2026-03-03', '', '', '',
   '2026-03-02', 1000, '{{"from": 2, "to": 1}}', 2, '2026-03-03', 1);
insert into ledger_obligation (id, amount, item, creditor_id, debtor_id, entry_id) values
  (1, 1500, '', 1, 2, 40), (2, 1000, '', 1, 2, 41), (3, 1000, '', 1, 3, 41), (4, 1000, '', 2, 1, 42);

insert into secretsanta_santaevent (id, status, deadline, gift_tiers, created_at, created_by_id, mode) values
  (50, 'active', '2027-06-01', '[100]', now(), 1, 'single');
insert into secretsanta_santaassignment (id, payload, giver_id, event_id, tier_index) values
  (51, '{s1}', 1, 50, null), (52, '{s2}', 2, 50, null), (53, '{s3}', 3, 50, null);
insert into secretsanta_santahelprequest (id, ideas, asked_at, answered_at, event_id, receiver_id) values
  (54, '["Książka"]', now(), now(), 50, 2);
"#,
        s1 = seal(1, 2),
        s2 = seal(2, 3),
        s3 = seal(3, 1),
    );
    sqlx::raw_sql(&sql).execute(pool).await.unwrap();
}

#[derive(serde::Serialize)]
/// A `SimpleJWT` refresh token's claims.
struct Claims {
    token_type: &'static str,
    exp: i64,
    iat: i64,
    jti: &'static str,
    user_id: String,
}

/// The old `MEDIA_ROOT` with the two referenced pictures.
fn legacy_media() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, (w, h)) in [(AVATAR, (300, 300)), (PICTURE, (640, 480))] {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, png(w, h)).unwrap();
    }
    dir
}

fn santa_key() -> String {
    fernet::Fernet::generate_key()
}

fn options(media: &Path, santa_key: &str) -> Options {
    Options {
        media_from: Some(media.to_path_buf()),
        santa_key: Some(santa_key.to_owned()),
        dry_run: false,
    }
}

async fn import(app: &TestApp, options: &Options) -> anyhow::Result<Report> {
    run(&app.ctx, app.db(), options).await
}

fn stored_images(app: &TestApp) -> usize {
    ["avatars", "photos"]
        .iter()
        .map(|prefix| {
            std::fs::read_dir(app.ctx.media.root().join(prefix)).map_or(0, Iterator::count)
        })
        .sum()
}

async fn login(app: &TestApp, username: &str) -> String {
    let response = app
        .call(
            Method::POST,
            "/api/auth/login",
            None,
            Some(json!({"username": username, "password": "s3cret-pass-123"})),
        )
        .await;
    response.expect(StatusCode::OK).json()["access"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn get(app: &TestApp, token: &str, uri: &str) -> Value {
    app.call(Method::GET, uri, Some(token), None)
        .await
        .expect(StatusCode::OK)
        .json()
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn everything_arrives_and_works_in_the_new_backend(pool: PgPool) {
    let app = TestApp::with_env(pool, &[("LEGACY_SECRET_KEY", OLD_SECRET)]);
    let key = santa_key();
    seed(app.db(), &key).await;
    let media = legacy_media();
    let report = import(&app, &options(media.path(), &key)).await.unwrap();
    assert_eq!((report.images, report.resealed), (2, 3));
    assert_eq!(report.missing_media, Vec::new());
    let rows: std::collections::HashMap<&str, i64> = report.rows.iter().copied().collect();
    assert_eq!(
        (
            rows["users"],
            rows["legacy_refresh_tokens"],
            rows["ledger_obligations"]
        ),
        (5, 1, 4),
        "the blacklisted and the expired refresh token stay behind"
    );

    // Members sign in with their old passwords (upgraded on the way in).
    let ala = login(&app, "ala").await;
    let hash: String = sqlx::query_scalar("select password_hash from users where id = 1")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    let never: Option<String> = sqlx::query_scalar("select password_hash from users where id = 5")
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(never, None, "an unusable password means not activated");
    let activation = app
        .call(
            Method::POST,
            "/api/activation/validate",
            None,
            Some(json!({"token": "raw-token"})),
        )
        .await;
    assert_eq!(activation.expect(StatusCode::OK).json()["username"], "nowy");

    // A browser still holding a Django refresh token keeps its session.
    let old_token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &Claims {
            token_type: "refresh",
            exp: (app.now() + TimeDelta::days(30)).timestamp(),
            iat: 0,
            jti: "live",
            user_id: "1".into(),
        },
        &jsonwebtoken::EncodingKey::from_secret(OLD_SECRET.as_bytes()),
    )
    .unwrap();
    app.call(
        Method::POST,
        "/api/auth/refresh",
        None,
        Some(json!({"refresh": old_token})),
    )
    .await
    .expect(StatusCode::OK);

    // The profile picture moved to the new layout and is served with its variants.
    let me = get(&app, &ala, "/api/me").await;
    let avatar = me["avatar"]["sm"].as_str().unwrap();
    assert!(
        avatar.contains("avatars/01234567-89ab-cdef-0123-456789abcdef/sm.webp"),
        "{avatar}"
    );
    let served = app.get(avatar).await;
    assert_eq!(served.status, StatusCode::OK);

    // The settled pact, its ruling, its picture and its debt.
    let pact = get(&app, &ala, "/api/pacts/20").await;
    assert_eq!(pact["status"], "resolved");
    assert_eq!(
        pact["outcome"],
        json!({"wagers": [{"participant_id": 22, "result": {"winner": "host"}}]})
    );
    assert_eq!(pact["attachments"][0]["caption"], "serwetka");
    assert_eq!(pact["attachments"][0]["width"], 640);
    let ruling = get(&app, &ala, "/api/polls/11").await;
    assert_eq!(ruling["kind"], "pact_ruling");
    let subject: Option<i64> =
        sqlx::query_scalar("select subject_claim_id from polls where id = 11")
            .fetch_one(app.db())
            .await
            .unwrap();
    assert_eq!(subject, Some(31));
    let resolution = get(&app, &ala, "/api/pacts/23").await;
    assert_eq!(resolution["status"], "active");
    let debts = get(&app, &ala, "/api/ledger/entries?pact_id=20").await;
    assert_eq!(debts["items"][0]["source"], "pact");

    // The ledger: the expense's shares were derived, the balances add up.
    let expense = get(&app, &ala, "/api/ledger/entries/41").await;
    assert_eq!(expense["breakdown"].as_array().unwrap().len(), 3);
    assert_eq!(expense["category"], "food");
    let balances = get(&app, &ala, "/api/ledger/balances").await;
    let net = |id: i64| {
        balances["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["user"]["id"] == id)
            .map_or(0, |m| m["net"].as_i64().unwrap())
    };
    // ala: +1500 (pact) +2000 (pizza) -1000 (ola paid her back); ola: -1500 -1000 +1000.
    assert_eq!((net(1), net(2), net(3)), (2500, -1500, -1000));

    // The running Secret Santa round, re-sealed: each giver still sees their victim.
    let santa = get(&app, &ala, "/api/secret-santa").await;
    assert_eq!(santa["my_victim"]["id"], 2);
    let ola = login(&app, "ola").await;
    let hers = get(&app, &ola, "/api/secret-santa").await;
    assert_eq!(hers["my_victim"]["id"], 3);
    assert_eq!(hers["help_requests_for_me"][0]["ideas"], json!(["Książka"]));
    assert_eq!(
        app.count("select count(*) from santa_assignments where sealed is null")
            .await,
        0
    );

    // New rows continue after the imported ids.
    let created = app
        .call(
            Method::POST,
            "/api/pacts",
            Some(&ala),
            Some(json!({"kind": "bet", "title": "Nowy", "condition": "c"})),
        )
        .await
        .expect(StatusCode::CREATED)
        .json();
    assert!(created["id"].as_i64().unwrap() > 23);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_mismatch_rolls_everything_back(pool: PgPool) {
    let app = TestApp::new(pool);
    let key = santa_key();
    seed(app.db(), &key).await;
    sqlx::query("update ledger_obligation set amount = 999 where id = 2")
        .execute(app.db())
        .await
        .unwrap();
    let media = legacy_media();
    let error = import(&app, &options(media.path(), &key))
        .await
        .unwrap_err();
    let text = format!("{error:#}");
    assert!(text.contains("ledger entry 41"), "{text}");
    assert!(text.contains("balances differ"), "{text}");
    assert_eq!(app.count("select count(*) from users").await, 0);
    assert_eq!(
        stored_images(&app),
        0,
        "the files written so far are removed"
    );
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_dry_run_checks_everything_and_keeps_nothing(pool: PgPool) {
    let app = TestApp::new(pool);
    let key = santa_key();
    seed(app.db(), &key).await;
    let media = legacy_media();
    let report = import(
        &app,
        &Options {
            dry_run: true,
            ..options(media.path(), &key)
        },
    )
    .await
    .unwrap();
    assert!(report.dry_run);
    assert!(
        report
            .to_string()
            .starts_with("Checked (dry run, nothing kept):")
    );
    assert_eq!(app.count("select count(*) from users").await, 0);
    assert_eq!(stored_images(&app), 0);
    // And the real run afterwards still works.
    import(&app, &options(media.path(), &key)).await.unwrap();
    assert_eq!(app.count("select count(*) from users").await, 5);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_running_santa_round_needs_the_old_key(pool: PgPool) {
    let app = TestApp::new(pool);
    seed(app.db(), &santa_key()).await;
    let media = legacy_media();
    let missing = import(
        &app,
        &Options {
            santa_key: None,
            ..options(media.path(), "")
        },
    )
    .await
    .unwrap_err();
    assert!(format!("{missing:#}").contains("LEGACY_SANTA_KEY"));
    let wrong = import(&app, &options(media.path(), &santa_key()))
        .await
        .unwrap_err();
    assert!(
        format!("{wrong:#}").contains("cannot be opened"),
        "{wrong:#}"
    );
    assert_eq!(app.count("select count(*) from users").await, 0);
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn it_runs_once_into_an_empty_database(pool: PgPool) {
    let app = TestApp::new(pool);
    let key = santa_key();
    seed(app.db(), &key).await;
    app.member("already").await;
    let media = legacy_media();
    let error = import(&app, &options(media.path(), &key))
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("already has users"));
}

#[sqlx::test(migrator = "app::db::MIGRATOR")]
async fn a_missing_file_drops_its_reference_and_is_reported(pool: PgPool) {
    let app = TestApp::new(pool);
    let key = santa_key();
    seed(app.db(), &key).await;
    let media = legacy_media();
    std::fs::remove_file(media.path().join(PICTURE)).unwrap();
    let report = import(&app, &options(media.path(), &key)).await.unwrap();
    assert_eq!(
        report.missing_media,
        [("picture 25 of pact 20".to_owned(), PICTURE.to_owned())]
    );
    assert_eq!(app.count("select count(*) from pact_attachments").await, 0);
    assert!(report.to_string().contains("Missing files"));
}
