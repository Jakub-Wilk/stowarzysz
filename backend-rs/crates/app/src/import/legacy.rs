//! The Django database as it was left by its last migration, read in full from one consistent
//! snapshot. Queries are checked at runtime (the compile-time checks know only the new schema).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{FromRow, PgConnection};

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password: String,
    pub is_active: bool,
    pub is_superuser: bool,
    pub avatar: String,
    pub birthday: Option<NaiveDate>,
    pub birthday_reminded_on: Option<NaiveDate>,
    pub date_joined: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ActivationToken {
    pub id: i64,
    pub user_id: i64,
    pub token_hash: String,
    pub created_by_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

/// A refresh token that is unexpired and not blacklisted.
#[derive(Debug, Clone, FromRow)]
pub struct RefreshToken {
    pub jti: String,
    pub user_id: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PushSubscription {
    pub id: i64,
    pub user_id: i64,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Poll {
    pub id: i64,
    pub creator_id: i64,
    pub title: String,
    pub kind: String,
    pub config: Value,
    pub status: String,
    pub close_reason: Option<String>,
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub reminders_sent: i16,
    pub proposed_avatar: String,
    pub previous_avatar: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct PollParticipant {
    pub poll_id: i64,
    pub user_id: i64,
    pub ballot: Option<Value>,
    pub vetoed: bool,
    pub abstained: bool,
    pub reason: String,
    pub voted_at: Option<DateTime<Utc>>,
    pub last_reaction: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct Pact {
    pub id: i64,
    pub creator_id: i64,
    pub kind: String,
    pub title: String,
    pub condition: String,
    pub notes: String,
    pub due_at: Option<DateTime<Utc>>,
    pub config: Value,
    pub status: String,
    pub outcome: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub last_nudged_at: Option<DateTime<Utc>>,
    pub judgment_poll_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PactParticipant {
    pub id: i64,
    pub pact_id: i64,
    pub user_id: i64,
    pub role: String,
    pub side: String,
    pub stake_amount: Option<i32>,
    pub stake_note: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Claim {
    pub id: i64,
    pub pact_id: i64,
    pub wager_id: Option<i64>,
    pub proposed_by_id: i64,
    pub result: Value,
    pub verdicts: Value,
    pub state: String,
    pub ruling_poll_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Confirmation {
    pub proposal_id: i64,
    pub user_id: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Consent {
    pub participant_id: i64,
    pub approver_id: i64,
    pub approved: Option<bool>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Attachment {
    pub id: i64,
    /// The pact or ledger entry.
    pub owner_id: i64,
    pub uploaded_by_id: i64,
    pub image: String,
    pub caption: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Entry {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub note: String,
    pub category: String,
    pub occurred_on: NaiveDate,
    pub amount: Option<i32>,
    pub currency: String,
    pub item: String,
    pub base_amount: Option<i32>,
    pub rate: Option<Decimal>,
    pub rate_date: Option<NaiveDate>,
    pub details: Value,
    pub source_type: String,
    pub source_id: Option<i32>,
    pub created_by_id: Option<i64>,
    pub updated_by_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Obligation {
    pub entry_id: i64,
    pub debtor_id: i64,
    pub creditor_id: i64,
    pub amount: i32,
    pub item: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct Rate {
    pub currency: String,
    pub requested_on: NaiveDate,
    pub rate: Decimal,
    pub rate_date: NaiveDate,
}

#[derive(Debug, Clone, FromRow)]
pub struct SantaEvent {
    pub id: i64,
    pub status: String,
    pub mode: String,
    pub deadline: DateTime<Utc>,
    pub gift_tiers: Value,
    pub created_by_id: i64,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SantaAssignment {
    pub id: i64,
    pub event_id: i64,
    pub giver_id: i64,
    pub tier_index: Option<i16>,
    pub payload: String,
    pub receiver_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SantaGift {
    pub id: i64,
    pub assignment_id: i64,
    pub amount: i32,
    pub note: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SantaHelp {
    pub id: i64,
    pub event_id: i64,
    pub receiver_id: i64,
    pub tier_index: Option<i16>,
    pub ideas: Value,
    pub asked_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
}

/// Everything the importer reads.
#[derive(Debug, Default)]
pub struct Legacy {
    pub users: Vec<User>,
    pub activation_tokens: Vec<ActivationToken>,
    pub refresh_tokens: Vec<RefreshToken>,
    pub push_subscriptions: Vec<PushSubscription>,
    pub polls: Vec<Poll>,
    pub poll_participants: Vec<PollParticipant>,
    pub pacts: Vec<Pact>,
    pub pact_participants: Vec<PactParticipant>,
    pub claims: Vec<Claim>,
    pub confirmations: Vec<Confirmation>,
    pub consents: Vec<Consent>,
    pub pact_attachments: Vec<Attachment>,
    pub entries: Vec<Entry>,
    pub obligations: Vec<Obligation>,
    pub entry_attachments: Vec<Attachment>,
    pub rates: Vec<Rate>,
    pub santa_events: Vec<SantaEvent>,
    pub santa_assignments: Vec<SantaAssignment>,
    pub santa_gifts: Vec<SantaGift>,
    pub santa_help: Vec<SantaHelp>,
}

async fn all<T>(conn: &mut PgConnection, sql: &str) -> sqlx::Result<Vec<T>>
where
    T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
{
    sqlx::query_as::<_, T>(sql).fetch_all(conn).await
}

/// Read the whole legacy database (`conn` should be in a read-only repeatable-read
/// transaction, so every table comes from the same moment). `now` decides which refresh tokens
/// are still worth keeping.
pub async fn read(conn: &mut PgConnection, now: DateTime<Utc>) -> anyhow::Result<Legacy> {
    let migrated: Vec<(String, String)> = sqlx::query_as(
        "select app, max(name) from django_migrations
         where app in ('accounts', 'voting', 'pacts', 'ledger', 'secretsanta', 'push') group by app",
    )
    .fetch_all(&mut *conn)
    .await?;
    for (app, latest) in [
        ("accounts", "0008_user_birthday"),
        ("ledger", "0011_categories"),
        ("pacts", "0004_open_to_all_and_judgments"),
        ("push", "0001_initial"),
        ("secretsanta", "0003_help_request"),
        ("voting", "0005_pollparticipant_last_reaction"),
    ] {
        let found = migrated
            .iter()
            .find(|(a, _)| a == app)
            .map(|(_, m)| m.as_str());
        anyhow::ensure!(
            found == Some(latest),
            "the legacy database is not at the schema this importer reads: {app} is at {found:?}, expected {latest}"
        );
    }
    Ok(Legacy {
        users: all(conn, "select id, username, password, is_active, is_superuser, avatar, birthday, birthday_reminded_on, date_joined from accounts_user order by id").await?,
        activation_tokens: all(conn, "select id, user_id, token_hash, created_by_id, created_at, expires_at, used_at from accounts_activationtoken order by id").await?,
        refresh_tokens: sqlx::query_as(
            "select o.jti, o.user_id, o.expires_at from token_blacklist_outstandingtoken o
             where o.user_id is not null and o.expires_at > $1
               and not exists (select 1 from token_blacklist_blacklistedtoken b where b.token_id = o.id)
             order by o.id",
        )
        .bind(now)
        .fetch_all(&mut *conn)
        .await?,
        push_subscriptions: all(conn, "select id, user_id, endpoint, p256dh, auth, user_agent, created_at from push_pushsubscription order by id").await?,
        polls: all(conn, "select id, creator_id, title, kind, config, status, close_reason, result, created_at, closed_at, reminders_sent, proposed_avatar, previous_avatar from voting_poll order by id").await?,
        poll_participants: all(conn, "select poll_id, user_id, ballot, vetoed, abstained, reason, voted_at, last_reaction from voting_pollparticipant order by id").await?,
        pacts: all(conn, "select id, creator_id, kind, title, condition, notes, due_at, config, status, outcome, created_at, resolved_at, last_nudged_at, judgment_poll_id from pacts_pact order by id").await?,
        pact_participants: all(conn, "select id, pact_id, user_id, role, side, stake_amount, stake_note, state, created_at from pacts_pactparticipant order by id").await?,
        claims: all(conn, "select id, pact_id, wager_id, proposed_by_id, result, verdicts, state, ruling_poll_id, created_at, decided_at from pacts_outcomeproposal order by id").await?,
        confirmations: all(conn, "select proposal_id, user_id, created_at from pacts_outcomeconfirmation order by id").await?,
        consents: all(conn, "select participant_id, approver_id, approved from pacts_joinconsent order by id").await?,
        pact_attachments: all(conn, "select id, pact_id as owner_id, uploaded_by_id, image, caption, created_at from pacts_pactattachment order by id").await?,
        entries: all(conn, "select id, kind, status, title, note, category, occurred_on, amount, currency, item, base_amount, rate, rate_date, details, source_type, source_id, created_by_id, updated_by_id, created_at, updated_at, version, decided_at from ledger_ledgerentry order by id").await?,
        obligations: all(conn, "select entry_id, debtor_id, creditor_id, amount, item from ledger_obligation order by id").await?,
        entry_attachments: all(conn, "select id, entry_id as owner_id, uploaded_by_id, image, '' as caption, created_at from ledger_entryattachment order by id").await?,
        rates: all(conn, "select currency, requested_on, rate, rate_date from ledger_exchangerate order by id").await?,
        santa_events: all(conn, "select id, status, mode, deadline, gift_tiers, created_by_id, created_at, ended_at from secretsanta_santaevent order by id").await?,
        santa_assignments: all(conn, "select id, event_id, giver_id, tier_index, payload, receiver_id from secretsanta_santaassignment order by id").await?,
        santa_gifts: all(conn, "select id, assignment_id, amount, note from secretsanta_santagift order by id").await?,
        santa_help: all(conn, "select id, event_id, receiver_id, tier_index, ideas, asked_at, answered_at from secretsanta_santahelprequest order by id").await?,
    })
}
