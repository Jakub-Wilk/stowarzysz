//! `stowarzysz import-django`: copy the Django database (and its media) into the new schema,
//! once, into an empty database.
//!
//! The legacy database is read from one read-only snapshot and never written. Everything goes
//! into the new database in one transaction, through the domain types (a row they cannot read
//! is a problem, not a guess), keeping the old ids. Before committing, the result is checked:
//! row counts, each ledger entry's obligations and amount recomputed against what Django stored,
//! the balances, each finished pact's outcome, each re-sealed Secret Santa pairing. Any problem
//! rolls everything back, including the media files written so far; so does `--dry-run`.

mod accounts;
mod ledger;
pub mod legacy;
mod media;
mod pacts;
mod polls;
mod santa;

use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::Context;
use crate::db::Uow;
use legacy::Legacy;
use media::MediaImport;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The old `MEDIA_ROOT` (needed if anything references a file).
    pub media_from: Option<PathBuf>,
    /// Django's `SECRET_SANTA_KEY` (needed if a Secret Santa round is running).
    pub santa_key: Option<String>,
    /// Check everything, then roll back.
    pub dry_run: bool,
}

/// What was imported.
#[derive(Debug, Default)]
pub struct Report {
    pub dry_run: bool,
    /// (table, rows)
    pub rows: Vec<(&'static str, i64)>,
    pub images: usize,
    /// Santa pairings re-sealed with the new key.
    pub resealed: usize,
    /// References dropped because their file was missing: (what, legacy path).
    pub missing_media: Vec<(String, String)>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verb = if self.dry_run {
            "Checked (dry run, nothing kept)"
        } else {
            "Imported"
        };
        writeln!(f, "{verb}:")?;
        for (table, rows) in &self.rows {
            writeln!(f, "  {table:<28} {rows:>7}")?;
        }
        writeln!(f, "  {:<28} {:>7}", "images", self.images)?;
        writeln!(
            f,
            "  {:<28} {:>7}",
            "re-sealed Santa pairings", self.resealed
        )?;
        if !self.missing_media.is_empty() {
            writeln!(f, "Missing files (references dropped):")?;
            for (what, path) in &self.missing_media {
                writeln!(f, "  {what}: {path}")?;
            }
        }
        Ok(())
    }
}

/// Everything wrong with the data, reported together.
#[derive(Debug, Default)]
pub struct Problems(Vec<String>);

impl Problems {
    pub fn push(&mut self, problem: String) {
        self.0.push(problem);
    }
}

/// Run the import from `legacy` into the empty database of `ctx`.
pub async fn run(ctx: &Context, legacy: &PgPool, options: &Options) -> anyhow::Result<Report> {
    let now = ctx.clock.now();
    let data = {
        let mut snapshot = legacy.begin().await?;
        sqlx::query("set transaction isolation level repeatable read, read only")
            .execute(&mut *snapshot)
            .await?;
        legacy::read(&mut snapshot, now).await?
    };
    let mut uow = Uow::begin(&ctx.db).await?;
    let users = sqlx::query_scalar!(r#"select count(*) as "n!" from users"#)
        .fetch_one(uow.conn())
        .await?;
    anyhow::ensure!(
        users == 0,
        "the database already has users: the import runs once, into an empty database"
    );
    let mut media = MediaImport::new(&ctx.media, options.media_from.clone());
    let outcome = import(&mut uow, ctx, &data, &mut media, options, now).await;
    let committed = match outcome {
        Ok(report) if !options.dry_run => uow.commit().await.map(|()| report).map_err(Into::into),
        other => other,
    };
    match committed {
        Ok(mut report) => {
            if options.dry_run {
                media.discard();
            }
            report.dry_run = options.dry_run;
            Ok(report)
        }
        Err(error) => {
            media.discard();
            Err(error)
        }
    }
}

async fn import(
    uow: &mut Uow,
    ctx: &Context,
    data: &Legacy,
    media: &mut MediaImport,
    options: &Options,
    now: DateTime<Utc>,
) -> anyhow::Result<Report> {
    let mut problems = Problems::default();
    accounts::import(uow, data, media, &mut problems).await?;
    let links = polls::import(uow, data, media, &mut problems).await?;
    let dropped_pact_pictures = pacts::import(uow, data, media, &mut problems, now).await?;
    polls::link(uow, &links).await?;
    let dropped_entry_pictures = ledger::import(uow, data, media, &mut problems).await?;
    let resealed =
        santa::import(uow, ctx, data, options.santa_key.as_deref(), &mut problems).await?;
    restart_ids(uow).await?;

    pacts::verify_outcomes(uow, data, &mut problems).await?;
    verify_balances(uow, data, &mut problems).await?;
    let expected = [
        ("users", data.users.len()),
        ("activation_tokens", data.activation_tokens.len()),
        ("legacy_refresh_tokens", data.refresh_tokens.len()),
        ("push_subscriptions", data.push_subscriptions.len()),
        ("exchange_rates", data.rates.len()),
        ("polls", data.polls.len()),
        ("poll_participants", data.poll_participants.len()),
        ("pacts", data.pacts.len()),
        ("pact_participants", data.pact_participants.len()),
        ("pact_claims", data.claims.len()),
        ("pact_claim_confirmations", data.confirmations.len()),
        ("pact_join_consents", data.consents.len()),
        (
            "pact_attachments",
            data.pact_attachments.len() - dropped_pact_pictures,
        ),
        ("ledger_entries", data.entries.len()),
        ("ledger_obligations", data.obligations.len()),
        (
            "ledger_attachments",
            data.entry_attachments.len() - dropped_entry_pictures,
        ),
        ("santa_events", data.santa_events.len()),
        ("santa_assignments", data.santa_assignments.len()),
        ("santa_gifts", data.santa_gifts.len()),
        ("santa_help_requests", data.santa_help.len()),
    ];
    let mut rows = Vec::with_capacity(expected.len());
    for (table, want) in expected {
        // Table names come from the list above, never from input.
        let got: i64 = sqlx::query_scalar(&format!("select count(*) from {table}"))
            .fetch_one(uow.conn())
            .await?;
        if usize::try_from(got).ok() != Some(want) {
            problems.push(format!("{table}: {got} rows imported, {want} expected"));
        }
        rows.push((table, got));
    }
    if !problems.0.is_empty() {
        anyhow::bail!(
            "the import was rolled back; fix these and run it again:\n  - {}",
            problems.0.join("\n  - ")
        );
    }
    Ok(Report {
        dry_run: options.dry_run,
        rows,
        images: media.images(),
        resealed,
        missing_media: media.missing.clone(),
    })
}

/// New rows continue after the imported ids.
async fn restart_ids(uow: &mut Uow) -> anyhow::Result<()> {
    for table in [
        "users",
        "activation_tokens",
        "push_subscriptions",
        "polls",
        "pacts",
        "pact_participants",
        "pact_claims",
        "pact_attachments",
        "ledger_entries",
        "ledger_obligations",
        "ledger_attachments",
        "santa_events",
        "santa_assignments",
        "santa_gifts",
        "santa_help_requests",
    ] {
        // Table names come from the list above, never from input.
        sqlx::query(&format!(
            "select setval(pg_get_serial_sequence('{table}', 'id'),
                           coalesce((select max(id) from {table}), 0) + 1, false)"
        ))
        .execute(uow.conn())
        .await?;
    }
    Ok(())
}

/// Who owes whom what (confirmed entries), before and after, must be the same.
async fn verify_balances(
    uow: &mut Uow,
    data: &Legacy,
    problems: &mut Problems,
) -> anyhow::Result<()> {
    use std::collections::BTreeMap;
    let confirmed: std::collections::HashSet<i64> = data
        .entries
        .iter()
        .filter(|e| e.status == "confirmed")
        .map(|e| e.id)
        .collect();
    let mut before: BTreeMap<(i64, i64, String), i64> = BTreeMap::new();
    for o in data
        .obligations
        .iter()
        .filter(|o| confirmed.contains(&o.entry_id))
    {
        *before
            .entry((o.debtor_id, o.creditor_id, o.item.to_lowercase()))
            .or_default() += i64::from(o.amount);
    }
    let after: BTreeMap<(i64, i64, String), i64> = sqlx::query!(
        r#"select o.debtor_id, o.creditor_id, lower(o.item) as "item!", sum(o.amount)::bigint as "amount!"
           from ledger_obligations o join ledger_entries e on e.id = o.entry_id
           where e.status = 'confirmed' group by 1, 2, 3"#
    )
    .fetch_all(uow.conn())
    .await?
    .into_iter()
    .map(|r| ((r.debtor_id, r.creditor_id, r.item), r.amount))
    .collect();
    if before != after {
        problems.push("the balances differ before and after the import".into());
    }
    Ok(())
}
