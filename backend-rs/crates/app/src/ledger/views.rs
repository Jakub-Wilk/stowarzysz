//! Reading the ledger. A page of entries costs a fixed number of queries (entries, obligations,
//! shares, attachments, people), however long it is.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use domain::ledger::money::{BASE_CURRENCY, CURRENCIES};
use domain::ledger::policy::{self, Actions};
use domain::ledger::{Category, Details, EntryKind, Source, Status};
use rust_decimal::Decimal;
use serde::Serialize;
use utoipa::ToSchema;

use super::EntryRow;
use crate::accounts::{Person, persons};
use crate::auth::Actor;
use crate::media::Image;
use crate::pagination::{Cursor, page};
use crate::{AppError, AppResult, Context};

const PAGE_SIZE: usize = 30;

/// `debtor` owes `creditor` `amount` (grosze, or a quantity of `item`) because of the entry.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ObligationView {
    pub debtor: Person,
    pub creditor: Person,
    pub amount: i64,
    /// Empty for money.
    pub item: String,
}

/// One person in an expense or income: what they paid (received) and what their part is, in
/// the entry's currency and in PLN (the latter is what the balances use).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ShareRow {
    pub user: Person,
    pub paid: i64,
    pub owed: i64,
    pub paid_base: i64,
    pub owed_base: i64,
}

/// A picture on an entry.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AttachmentView {
    pub id: i64,
    pub image: Image,
    pub width: i32,
    pub height: i32,
    pub uploaded_by: Person,
    pub created_at: DateTime<Utc>,
}

/// An entry as everyone sees it. `details` depends on the kind; an expense is explained by its
/// `breakdown`, not by its `obligations` (with several payers their pairing is arbitrary).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct EntryView {
    pub id: i64,
    pub kind: EntryKind,
    pub status: Status,
    pub title: String,
    pub note: String,
    /// Expenses and incomes only.
    pub category: Option<Category>,
    pub occurred_on: NaiveDate,
    /// As typed: minor units of `currency`, or a quantity of `item`; `null` for a settlement
    /// mixing money and goods.
    pub amount: Option<i64>,
    /// Empty for goods.
    pub currency: String,
    pub item: String,
    /// `amount` in grosze; `null` for goods and mixed entries.
    pub base_amount: Option<i64>,
    /// PLN per unit of `currency` (a decimal string, e.g. `4.2537`); `null` for PLN.
    #[schema(value_type = Option<String>)]
    pub rate: Option<Decimal>,
    pub rate_date: Option<NaiveDate>,
    pub details: Details,
    pub source: Source,
    /// The pact this entry settles.
    pub pact_id: Option<i64>,
    /// `null` when recorded by the system (a pact).
    pub created_by: Option<Person>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When it was confirmed, rejected or cancelled.
    pub decided_at: Option<DateTime<Utc>>,
    /// Send it back when editing.
    pub version: i32,
    pub obligations: Vec<ObligationView>,
    /// Expenses and incomes: everyone's part, biggest payer first.
    pub breakdown: Option<Vec<ShareRow>>,
    pub attachments: Vec<AttachmentView>,
    pub actions: Actions,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct EntryPage {
    pub items: Vec<EntryView>,
    /// Pass as `cursor` for the next page; `null` on the last one.
    pub next_cursor: Option<String>,
}

/// Which entries to list.
#[derive(Debug, Clone, Default)]
pub struct EntryFilter {
    pub kind: Option<EntryKind>,
    /// One pact's settlements (few, so unpaged).
    pub pact_id: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CurrencyMeta {
    pub code: String,
    /// Minor-unit digits: 2 for PLN, 0 for JPY.
    pub exponent: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CategoryMeta {
    pub key: Category,
    pub label: String,
    pub emoji: String,
}

/// What the forms offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Meta {
    pub base_currency: String,
    /// The common ones first.
    pub currencies: Vec<CurrencyMeta>,
    pub categories: Vec<CategoryMeta>,
}

pub fn meta() -> Meta {
    Meta {
        base_currency: BASE_CURRENCY.to_owned(),
        currencies: CURRENCIES
            .iter()
            .map(|(code, exponent)| CurrencyMeta {
                code: (*code).to_owned(),
                exponent: *exponent,
            })
            .collect(),
        categories: Category::ALL
            .iter()
            .map(|c| CategoryMeta {
                key: *c,
                label: c.label().to_owned(),
                emoji: c.emoji().to_owned(),
            })
            .collect(),
    }
}

fn day_cursor(row: &EntryRow) -> Cursor {
    Cursor {
        at: row.occurred_on.and_time(NaiveTime::MIN).and_utc(),
        id: row.id,
    }
}

/// The feed, newest first (by the day it happened). Everyone reads every entry.
pub async fn list(ctx: &Context, actor: &Actor, filter: &EntryFilter) -> AppResult<EntryPage> {
    let after = filter.cursor.as_deref().map(Cursor::decode).transpose()?;
    let paged = filter.pact_id.is_none();
    let limit = paged.then(|| i64::try_from(PAGE_SIZE + 1).unwrap_or(i64::MAX));
    let rows = sqlx::query_as!(
        EntryRow,
        "select id, kind, status, title, note, category, occurred_on, amount, currency, item,
                base_amount, rate, rate_date, details, source, pact_id, created_by, created_at,
                updated_at, version, decided_at
         from ledger_entries
         where ($1::text is null or kind = $1)
           and ($2::bigint is null or pact_id = $2)
           and ($3::date is null or (occurred_on, id) < ($3, $4))
         order by occurred_on desc, id desc
         limit $5",
        filter.kind.map(EntryKind::as_str),
        filter.pact_id,
        after.map(|c| c.at.date_naive()),
        after.map_or(0, |c| c.id),
        limit,
    )
    .fetch_all(&ctx.db)
    .await?;
    let (rows, next_cursor) = if paged {
        page(rows, PAGE_SIZE, day_cursor)
    } else {
        (rows, None)
    };
    Ok(EntryPage {
        items: views(ctx, actor, rows).await?,
        next_cursor,
    })
}

pub async fn detail(ctx: &Context, actor: &Actor, id: i64) -> AppResult<EntryView> {
    let row = sqlx::query_as!(
        EntryRow,
        "select id, kind, status, title, note, category, occurred_on, amount, currency, item,
                base_amount, rate, rate_date, details, source, pact_id, created_by, created_at,
                updated_at, version, decided_at
         from ledger_entries where id = $1",
        id,
    )
    .fetch_optional(&ctx.db)
    .await?
    .ok_or(AppError::NotFound)?;
    views(ctx, actor, vec![row])
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// An entry's obligations, shares and attachments.
type Related = (Vec<ObligationView>, Vec<ShareRow>, Vec<AttachmentView>);

async fn views(ctx: &Context, actor: &Actor, rows: Vec<EntryRow>) -> AppResult<Vec<EntryView>> {
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let obligations = sqlx::query!(
        "select entry_id, debtor_id, creditor_id, amount, item from ledger_obligations
         where entry_id = any($1) order by id",
        &ids,
    )
    .fetch_all(&ctx.db)
    .await?;
    let shares = sqlx::query!(
        "select entry_id, user_id, paid, owed, paid_base, owed_base from ledger_entry_shares
         where entry_id = any($1)",
        &ids,
    )
    .fetch_all(&ctx.db)
    .await?;
    let attachments = sqlx::query!(
        "select id, entry_id, uploaded_by, image_key, width, height, created_at
         from ledger_attachments where entry_id = any($1) order by created_at, id",
        &ids,
    )
    .fetch_all(&ctx.db)
    .await?;

    let mut people: BTreeSet<i64> = rows.iter().filter_map(|r| r.created_by).collect();
    people.extend(
        obligations
            .iter()
            .flat_map(|o| [o.debtor_id, o.creditor_id]),
    );
    people.extend(shares.iter().map(|s| s.user_id));
    people.extend(attachments.iter().map(|a| a.uploaded_by));
    let people = persons(ctx, &people.into_iter().collect::<Vec<_>>()).await?;
    let person = |id: i64| {
        people.get(&id).cloned().unwrap_or_else(|| Person {
            id,
            username: String::new(),
            avatar: None,
        })
    };

    let mut by_entry: HashMap<i64, Related> = HashMap::new();
    for o in obligations {
        by_entry
            .entry(o.entry_id)
            .or_default()
            .0
            .push(ObligationView {
                debtor: person(o.debtor_id),
                creditor: person(o.creditor_id),
                amount: o.amount,
                item: o.item,
            });
    }
    for s in shares {
        by_entry.entry(s.entry_id).or_default().1.push(ShareRow {
            user: person(s.user_id),
            paid: s.paid,
            owed: s.owed,
            paid_base: s.paid_base,
            owed_base: s.owed_base,
        });
    }
    let now = ctx.clock.now();
    for a in attachments {
        by_entry
            .entry(a.entry_id)
            .or_default()
            .2
            .push(AttachmentView {
                id: a.id,
                image: ctx.media.image(&a.image_key, now),
                width: a.width,
                height: a.height,
                uploaded_by: person(a.uploaded_by),
                created_at: a.created_at,
            });
    }

    rows.into_iter()
        .map(|row| {
            let entry = row.entry()?;
            let (obligations, mut shares, attachments) =
                by_entry.remove(&row.id).unwrap_or_default();
            let breakdown = matches!(entry.content.details, Details::Expense(_)).then(|| {
                shares.sort_by(|a, b| {
                    b.paid
                        .cmp(&a.paid)
                        .then_with(|| a.user.username.cmp(&b.user.username))
                });
                shares
            });
            Ok(EntryView {
                id: row.id,
                kind: entry.kind(),
                status: entry.status,
                title: row.title,
                note: row.note,
                category: entry.content.category,
                occurred_on: row.occurred_on,
                amount: row.amount,
                currency: row.currency,
                item: row.item,
                base_amount: row.base_amount,
                rate: row.rate.map(|rate| rate.normalize()),
                rate_date: row.rate_date,
                actions: policy::actions(&entry, actor.user_id),
                details: entry.content.details,
                source: entry.source,
                pact_id: row.pact_id,
                created_by: row.created_by.map(person),
                created_at: row.created_at,
                updated_at: row.updated_at,
                decided_at: row.decided_at,
                version: row.version,
                obligations,
                breakdown,
                attachments,
            })
        })
        .collect()
}
