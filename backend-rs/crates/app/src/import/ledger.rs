//! Kasa Skarbowa. Every entry is read through the domain rules, and what they make of it (its
//! obligations, its amount in PLN) must equal what Django stored; the obligations are then
//! written by the same code the services use, together with the per-person shares.

use domain::ledger::{Category, Content, Details, Entry, EntryKind, Source, Status};

use crate::db::Uow;
use crate::media::Purpose;

use super::Problems;
use super::legacy::{self, Legacy};
use super::media::MediaImport;

type Debt = (i64, i64, i64, String);

fn source(entry: &legacy::Entry, kind: EntryKind) -> Option<Source> {
    match entry.source_type.as_str() {
        // A debt nobody recorded predates sources: read-only history.
        "" if kind == EntryKind::Debt && entry.created_by_id.is_none() => Some(Source::Legacy),
        "" => Some(Source::Manual),
        other => Source::parse(other),
    }
}

/// Entries' attachments dropped because their file was missing.
pub async fn import(
    uow: &mut Uow,
    data: &Legacy,
    media: &mut MediaImport,
    problems: &mut Problems,
) -> anyhow::Result<usize> {
    let mut stored: std::collections::HashMap<i64, Vec<Debt>> = std::collections::HashMap::new();
    for o in &data.obligations {
        stored.entry(o.entry_id).or_default().push((
            o.debtor_id,
            o.creditor_id,
            i64::from(o.amount),
            o.item.clone(),
        ));
    }
    for e in &data.entries {
        let Some(kind) = EntryKind::parse(&e.kind) else {
            problems.push(format!(
                "ledger entry {} has unknown kind {:?}",
                e.id, e.kind
            ));
            continue;
        };
        let (Some(status), Some(source)) = (Status::parse(&e.status), source(e, kind)) else {
            problems.push(format!(
                "ledger entry {} has an unknown status or source",
                e.id
            ));
            continue;
        };
        let category = Category::parse(&e.category);
        if category.is_none() && !e.category.is_empty() {
            problems.push(format!(
                "ledger entry {} has unknown category {:?}",
                e.id, e.category
            ));
            continue;
        }
        let details = match Details::from_json(kind, &e.details) {
            Ok(details) => details,
            Err(error) => {
                problems.push(format!(
                    "ledger entry {} has unreadable details: {error}",
                    e.id
                ));
                continue;
            }
        };
        let entry = Entry {
            status,
            source,
            created_by: e.created_by_id,
            rate: e.rate,
            content: Content {
                kind,
                title: e.title.clone(),
                note: e.note.clone(),
                category,
                occurred_on: e.occurred_on,
                amount: e.amount.map(i64::from),
                currency: e.currency.clone(),
                item: e.item.clone(),
                details,
            },
        };
        let mut derived: Vec<Debt> = entry
            .obligations()
            .into_iter()
            .map(|d| (d.debtor_id, d.creditor_id, d.amount, d.item))
            .collect();
        let mut had = stored.remove(&e.id).unwrap_or_default();
        derived.sort();
        had.sort();
        if derived != had {
            problems.push(format!(
                "ledger entry {}: the obligations derived now {derived:?} differ from the stored {had:?}",
                e.id
            ));
        }
        let base = entry.base_amount();
        if base != e.base_amount.map(i64::from) {
            problems.push(format!(
                "ledger entry {}: its amount in PLN is {base:?} now, {:?} stored",
                e.id, e.base_amount
            ));
        }
        let (pact_id, source_ref) = match source {
            Source::Pact => (e.source_id.map(i64::from), None),
            Source::Tricount => (None, e.source_id.map(i64::from)),
            Source::Manual | Source::Legacy => (None, None),
        };
        sqlx::query!(
            "insert into ledger_entries (id, kind, status, title, note, category, occurred_on, amount,
                                         currency, item, base_amount, rate, rate_date, details, source,
                                         source_ref, pact_id, created_by, updated_by, created_at,
                                         updated_at, version, decided_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                     $18, $19, $20, $21, $22, $23)",
            e.id,
            kind.as_str(),
            status.as_str(),
            e.title,
            e.note,
            e.category,
            e.occurred_on,
            e.amount.map(i64::from),
            e.currency,
            e.item,
            e.base_amount.map(i64::from),
            e.rate,
            e.rate_date,
            entry.content.details.to_json(),
            source.as_str(),
            source_ref,
            pact_id,
            e.created_by_id,
            e.updated_by_id,
            e.created_at,
            e.updated_at,
            e.version,
            e.decided_at,
        )
        .execute(uow.conn())
        .await?;
        crate::ledger::write_meaning(uow, e.id, &entry).await?;
    }
    for (entry, _) in stored {
        problems.push(format!("obligations of a missing ledger entry {entry}"));
    }
    let mut dropped = 0;
    for a in &data.entry_attachments {
        let what = format!("picture {} of ledger entry {}", a.id, a.owner_id);
        let Some(image) = media.adopt(what, &a.image, Purpose::Photo).await? else {
            dropped += 1;
            continue;
        };
        sqlx::query!(
            "insert into ledger_attachments (id, entry_id, uploaded_by, image_key, width, height, created_at)
             values ($1, $2, $3, $4, $5, $6, $7)",
            a.id,
            a.owner_id,
            a.uploaded_by_id,
            image.key,
            image.width,
            image.height,
            a.created_at,
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(dropped)
}
