//! Kasa Skarbowa: a Tricount for the whole group. Entries are validated and interpreted by the
//! pure rules in [`domain::ledger`]; every change here is one transaction with the entry locked,
//! its obligations (and an expense's per-person shares) rewritten, and its events and pushes
//! queued in the same transaction. Everyone reads everything; push goes to the people involved.
//! Other contexts write to the ledger only through [`record_debts`].

mod balances;
mod rates;
mod services;
mod stats;
mod views;

use chrono::{DateTime, NaiveDate, Utc};
use domain::ledger::policy::Denial;
use domain::ledger::{Category, Content, Details, Entry, EntryKind, Source, Status};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::db::Uow;
use crate::{AppError, AppResult};

pub use balances::{Balances, MemberBalance, PendingPayment, Transfer, balances};
pub use rates::{Rate, RateView, Rates};
pub use services::{
    EntryUpdate, MAX_ATTACHMENTS, NewEntry, add_attachment, cancel, confirm, create,
    delete_attachment, rate, record_debts, reject, update,
};
pub use stats::{Stats, StatsCategory, StatsPerson, StatsPoint, StatsQuery, stats};
pub use views::{
    AttachmentView, CategoryMeta, CurrencyMeta, EntryFilter, EntryPage, EntryView, Meta,
    ObligationView, ShareRow, detail, list, meta,
};

/// An entry as stored.
#[derive(Debug, Clone)]
pub(crate) struct EntryRow {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub note: String,
    pub category: String,
    pub occurred_on: NaiveDate,
    pub amount: Option<i64>,
    pub currency: String,
    pub item: String,
    pub base_amount: Option<i64>,
    pub rate: Option<Decimal>,
    pub rate_date: Option<NaiveDate>,
    pub details: Value,
    pub source: String,
    pub pact_id: Option<i64>,
    pub created_by: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
    pub decided_at: Option<DateTime<Utc>>,
}

impl EntryRow {
    /// The entry the domain rules work on.
    pub fn entry(&self) -> AppResult<Entry> {
        let corrupt = |what: &str| -> AppError {
            anyhow::anyhow!("ledger entry {} has an invalid {what}", self.id).into()
        };
        let kind = EntryKind::parse(&self.kind).ok_or_else(|| corrupt("kind"))?;
        Ok(Entry {
            status: Status::parse(&self.status).ok_or_else(|| corrupt("status"))?,
            source: Source::parse(&self.source).ok_or_else(|| corrupt("source"))?,
            created_by: self.created_by,
            rate: self.rate,
            content: Content {
                kind,
                title: self.title.clone(),
                note: self.note.clone(),
                category: Category::parse(&self.category),
                occurred_on: self.occurred_on,
                amount: self.amount,
                currency: self.currency.clone(),
                item: self.item.clone(),
                details: Details::from_json(kind, &self.details)
                    .map_err(|error| corrupt(&format!("details ({error})")))?,
            },
        })
    }
}

/// Lock entry `id` for the rest of `uow` (404 if it does not exist).
pub(crate) async fn lock(uow: &mut Uow, id: i64) -> AppResult<EntryRow> {
    sqlx::query_as!(
        EntryRow,
        "select id, kind, status, title, note, category, occurred_on, amount, currency, item,
                base_amount, rate, rate_date, details, source, pact_id, created_by, created_at,
                updated_at, version, decided_at
         from ledger_entries where id = $1 for update",
        id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)
}

/// Map a policy refusal to the API error.
pub(crate) fn denied(denial: Denial) -> AppError {
    match denial {
        Denial::Conflict { code, message } => AppError::conflict(code, message),
        Denial::Forbidden { code, message } => AppError::forbidden(code, message),
    }
}
