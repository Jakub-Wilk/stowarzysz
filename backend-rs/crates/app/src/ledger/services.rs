//! Changing the ledger. Exchange rates are fetched before a transaction opens; everything else
//! happens in one transaction with the entry locked.

use std::collections::HashMap;

use chrono::NaiveDate;
use domain::DomainError;
use domain::ledger::announce::{self, Change};
use domain::ledger::input::{self, DebtInput, Draft, ExpenseInput, PaymentInput};
use domain::ledger::money::{BASE_CURRENCY, Debt};
use domain::ledger::policy::{self, Action};
use domain::ledger::{Content, Entry, EntryKind, Source, Status};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::rates::{Rate, RateView};
use super::{EntryRow, denied, lock};
use crate::auth::Actor;
use crate::clock::local_today;
use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::media::{self, Purpose};
use crate::push::Notification;
use crate::{AppError, AppResult, Context};

/// Pictures per entry.
pub const MAX_ATTACHMENTS: i64 = 10;

/// A new entry, by kind.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewEntry {
    /// Somebody paid for something the others shared. A plain expense is one item.
    Expense(ExpenseInput),
    /// A negative expense: `payers` received money (a refund, a deposit back) that the people
    /// of the items share, so they are owed their parts.
    Income(ExpenseInput),
    /// A debt you are a party to (money in any currency, or goods).
    Debt(DebtInput),
    /// You paid somebody back; it counts once they confirm.
    Payment(PaymentInput),
}

impl NewEntry {
    fn kind(&self) -> EntryKind {
        match self {
            Self::Expense(_) => EntryKind::Expense,
            Self::Income(_) => EntryKind::Income,
            Self::Debt(_) => EntryKind::Debt,
            Self::Payment(_) => EntryKind::Payment,
        }
    }

    fn draft(&self, author: i64, today: NaiveDate) -> Result<Draft, DomainError> {
        match self {
            Self::Expense(expense) => input::expense(EntryKind::Expense, expense, today),
            Self::Income(income) => input::expense(EntryKind::Income, income, today),
            Self::Debt(debt) => input::debt(debt, author, today),
            Self::Payment(payment) => input::payment(payment, author, today),
        }
    }
}

/// The new content of an expense, an income or a manual debt, with the `version` it was
/// edited from (409 if somebody saved in between). The `kind` cannot change.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct EntryUpdate {
    pub version: i32,
    #[serde(flatten)]
    pub entry: NewEntry,
}

/// The rate an entry in `currency` dated `day` uses (fetched and kept if not known yet).
pub async fn rate(ctx: &Context, currency: &str, day: NaiveDate) -> AppResult<RateView> {
    let today = local_today(ctx.clock.now());
    let rate = ctx.rates.rate(&ctx.db, currency, day, today).await?;
    Ok(RateView {
        currency: currency.to_owned(),
        rate: rate.value.normalize(),
        rate_date: rate.day,
    })
}

async fn rate_for(ctx: &Context, content: &Content) -> AppResult<Option<Rate>> {
    match content.foreign_currency() {
        None => Ok(None),
        Some(currency) => {
            let today = local_today(ctx.clock.now());
            Ok(Some(
                ctx.rates
                    .rate(&ctx.db, currency, content.occurred_on, today)
                    .await?,
            ))
        }
    }
}

/// Record an entry; returns its id.
pub async fn create(ctx: &Context, actor: &Actor, request: NewEntry) -> AppResult<i64> {
    let draft = request.draft(actor.user_id, local_today(ctx.clock.now()))?;
    let rate = rate_for(ctx, &draft.content).await?;
    let mut uow = Uow::begin(&ctx.db).await?;
    require_members(&mut uow, &draft).await?;
    let status = if draft.content.kind == EntryKind::Payment {
        Status::Pending // counts once the receiver confirms
    } else {
        Status::Confirmed
    };
    let entry = Entry {
        status,
        source: Source::Manual,
        created_by: Some(actor.user_id),
        content: draft.content,
        rate: rate.map(|r| r.value),
    };
    refuse_nothing(&entry)?;
    let id = insert(&mut uow, &entry, rate, Some(actor.user_id), None).await?;
    write_meaning(&mut uow, id, &entry).await?;
    announce(&mut uow, ctx, id, &entry, Change::Created, Some(actor)).await?;
    uow.commit().await?;
    Ok(id)
}

/// Replace an entry's content (expenses and incomes by anyone, manual debts by the person owed).
pub async fn update(ctx: &Context, actor: &Actor, id: i64, request: EntryUpdate) -> AppResult<()> {
    // Refuse what can never work before validating or fetching a rate.
    let current = find(ctx, id).await?.entry()?;
    if current.kind() == EntryKind::Payment {
        return Err(AppError::conflict(
            "not_editable",
            "Tego wpisu nie można edytować.",
        ));
    }
    policy::check(Action::Edit, &current, actor.user_id).map_err(denied)?;
    if request.entry.kind() != current.kind() {
        return Err(
            DomainError::new("kind_mismatch", "Rodzaju wpisu nie można zmienić.")
                .on("kind")
                .into(),
        );
    }
    let draft = request
        .entry
        .draft(actor.user_id, local_today(ctx.clock.now()))?;
    let rate = rate_for(ctx, &draft.content).await?;

    let mut uow = Uow::begin(&ctx.db).await?;
    let row = lock(&mut uow, id).await?;
    let locked = row.entry()?;
    policy::check(Action::Edit, &locked, actor.user_id).map_err(denied)?;
    if row.version != request.version {
        return Err(AppError::conflict(
            "stale_version",
            "Ktoś zmienił ten wpis w międzyczasie. Odśwież i spróbuj jeszcze raz.",
        ));
    }
    require_members(&mut uow, &draft).await?;
    let entry = Entry {
        content: draft.content,
        rate: rate.map(|r| r.value),
        ..locked
    };
    refuse_nothing(&entry)?;
    let content = &entry.content;
    sqlx::query!(
        "update ledger_entries
         set title = $2, note = $3, category = $4, occurred_on = $5, amount = $6, currency = $7,
             item = $8, base_amount = $9, rate = $10, rate_date = $11, details = $12,
             version = version + 1, updated_by = $13, updated_at = now()
         where id = $1",
        id,
        content.title,
        content.note,
        content.category.map_or("", |c| c.as_str()),
        content.occurred_on,
        content.amount,
        content.currency,
        content.item,
        entry.base_amount(),
        rate.map(|r| r.value),
        rate.map(|r| r.day),
        content.details.to_json(),
        actor.user_id,
    )
    .execute(uow.conn())
    .await?;
    write_meaning(&mut uow, id, &entry).await?;
    announce(&mut uow, ctx, id, &entry, Change::Updated, Some(actor)).await?;
    uow.commit().await
}

/// The receiver confirms a payment: it now counts.
pub async fn confirm(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    decide(ctx, actor, id, Action::Confirm).await
}

/// The receiver says they weren't paid: the payment never counts.
pub async fn reject(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    decide(ctx, actor, id, Action::Reject).await
}

/// Withdraw an entry: delete an expense, take back a payment, drop a manual debt. Nothing is
/// ever deleted; it just stops counting.
pub async fn cancel(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    decide(ctx, actor, id, Action::Cancel).await
}

async fn decide(ctx: &Context, actor: &Actor, id: i64, action: Action) -> AppResult<()> {
    let (status, change) = match action {
        Action::Confirm => (Status::Confirmed, Change::Confirmed),
        Action::Reject => (Status::Rejected, Change::Rejected),
        _ => (Status::Cancelled, Change::Cancelled),
    };
    let mut uow = Uow::begin(&ctx.db).await?;
    let row = lock(&mut uow, id).await?;
    let mut entry = row.entry()?;
    policy::check(action, &entry, actor.user_id).map_err(denied)?;
    sqlx::query!(
        "update ledger_entries
         set status = $2, decided_at = $3, version = version + 1, updated_at = now()
         where id = $1",
        id,
        status.as_str(),
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    entry.status = status;
    announce(&mut uow, ctx, id, &entry, change, Some(actor)).await?;
    uow.commit().await
}

/// Record what a pact decided is owed as one debt entry, inside the caller's transaction. It
/// counts at once and changes only through the pact. Nothing owed (a draw, a void) records
/// nothing. Money is in grosze.
pub async fn record_debts(
    uow: &mut Uow,
    ctx: &Context,
    debts: Vec<Debt>,
    title: &str,
    pact_id: i64,
    on: NaiveDate,
) -> AppResult<Option<i64>> {
    if debts.is_empty() {
        return Ok(None);
    }
    let draft = input::debts(debts, title, on, BASE_CURRENCY)?;
    let entry = Entry {
        status: Status::Confirmed,
        source: Source::Pact,
        created_by: None,
        content: draft.content,
        rate: None,
    };
    let id = insert(uow, &entry, None, None, Some(pact_id)).await?;
    write_meaning(uow, id, &entry).await?;
    announce(uow, ctx, id, &entry, Change::Created, None).await?;
    Ok(Some(id))
}

/// Add a picture (an upload of purpose `photo`): a receipt, proof of a transfer.
pub async fn add_attachment(
    ctx: &Context,
    actor: &Actor,
    entry_id: i64,
    upload_id: Uuid,
) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let row = lock(&mut uow, entry_id).await?;
    policy::check(Action::Attach, &row.entry()?, actor.user_id).map_err(denied)?;
    let count = sqlx::query_scalar!(
        r#"select count(*) as "count!" from ledger_attachments where entry_id = $1"#,
        entry_id
    )
    .fetch_one(uow.conn())
    .await?;
    if count >= MAX_ATTACHMENTS {
        return Err(DomainError::new(
            "too_many_attachments",
            format!("Do wpisu można dodać maksymalnie {MAX_ATTACHMENTS} zdjęć."),
        )
        .into());
    }
    let image = media::take_upload(&mut uow, ctx, actor.user_id, upload_id, Purpose::Photo).await?;
    sqlx::query!(
        "insert into ledger_attachments (entry_id, uploaded_by, image_key, width, height)
         values ($1, $2, $3, $4, $5)",
        entry_id,
        actor.user_id,
        image.key,
        image.width,
        image.height,
    )
    .execute(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::LedgerUpdated { entry_id })
        .await?;
    uow.commit().await
}

/// Take a picture down (its uploader or the entry's author).
pub async fn delete_attachment(
    ctx: &Context,
    actor: &Actor,
    entry_id: i64,
    attachment_id: i64,
) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let row = lock(&mut uow, entry_id).await?;
    let attachment = sqlx::query!(
        "select uploaded_by, image_key from ledger_attachments where id = $1 and entry_id = $2",
        attachment_id,
        entry_id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    if actor.user_id != attachment.uploaded_by && Some(actor.user_id) != row.created_by {
        return Err(AppError::forbidden(
            "not_uploader",
            "Zdjęcie może usunąć jego autor albo autor wpisu.",
        ));
    }
    sqlx::query!(
        "delete from ledger_attachments where id = $1",
        attachment_id
    )
    .execute(uow.conn())
    .await?;
    media::trash(&mut uow, Some(&attachment.image_key)).await?;
    uow.publish(Audience::All, Event::LedgerUpdated { entry_id })
        .await?;
    uow.commit().await
}

/// An entry without a transaction (to refuse early).
async fn find(ctx: &Context, id: i64) -> AppResult<EntryRow> {
    sqlx::query_as!(
        EntryRow,
        "select id, kind, status, title, note, category, occurred_on, amount, currency, item,
                base_amount, rate, rate_date, details, source, pact_id, created_by, created_at,
                updated_at, version, decided_at
         from ledger_entries where id = $1",
        id,
    )
    .fetch_optional(&ctx.db)
    .await?
    .ok_or(AppError::NotFound)
}

/// Everyone the draft names must be a member (active, activated).
async fn require_members(uow: &mut Uow, draft: &Draft) -> AppResult<()> {
    let found = sqlx::query_scalar!(
        r#"select count(*) as "count!" from users
           where id = any($1) and is_active and password_hash is not null"#,
        &draft.members,
    )
    .fetch_one(uow.conn())
    .await?;
    if usize::try_from(found).ok() != Some(draft.members.len()) {
        return Err(
            DomainError::new("unknown_member", "Nieznany lub nieaktywny poseł.")
                .on(draft.members_field)
                .into(),
        );
    }
    Ok(())
}

/// Money that converts to nothing (a few yen, rupiah) would be an entry nobody owes anything by.
fn refuse_nothing(entry: &Entry) -> AppResult<()> {
    if entry.base_amount() == Some(0) {
        return Err(DomainError::new(
            "amount_too_small",
            "Po przeliczeniu na złote kwota wynosi 0 zł.",
        )
        .on("amount")
        .into());
    }
    Ok(())
}

async fn insert(
    uow: &mut Uow,
    entry: &Entry,
    rate: Option<Rate>,
    author: Option<i64>,
    pact_id: Option<i64>,
) -> AppResult<i64> {
    let content = &entry.content;
    Ok(sqlx::query_scalar!(
        "insert into ledger_entries (kind, status, title, note, category, occurred_on, amount,
                                     currency, item, base_amount, rate, rate_date, details, source,
                                     pact_id, created_by, updated_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)
         returning id",
        content.kind.as_str(),
        entry.status.as_str(),
        content.title,
        content.note,
        content.category.map_or("", |c| c.as_str()),
        content.occurred_on,
        content.amount,
        content.currency,
        content.item,
        entry.base_amount(),
        rate.map(|r| r.value),
        rate.map(|r| r.day),
        content.details.to_json(),
        entry.source.as_str(),
        pact_id,
        author,
    )
    .fetch_one(uow.conn())
    .await?)
}

/// (Re)derive what entry `id` means: its obligations and, for an expense or income, each
/// person's paid and owed amounts.
pub(crate) async fn write_meaning(uow: &mut Uow, id: i64, entry: &Entry) -> AppResult<()> {
    sqlx::query!("delete from ledger_obligations where entry_id = $1", id)
        .execute(uow.conn())
        .await?;
    sqlx::query!("delete from ledger_entry_shares where entry_id = $1", id)
        .execute(uow.conn())
        .await?;
    let obligations = entry.obligations();
    sqlx::query!(
        "insert into ledger_obligations (entry_id, debtor_id, creditor_id, amount, item)
         select $1, * from unnest($2::bigint[], $3::bigint[], $4::bigint[], $5::text[])",
        id,
        &obligations.iter().map(|d| d.debtor_id).collect::<Vec<_>>(),
        &obligations
            .iter()
            .map(|d| d.creditor_id)
            .collect::<Vec<_>>(),
        &obligations.iter().map(|d| d.amount).collect::<Vec<_>>(),
        &obligations
            .iter()
            .map(|d| d.item.clone())
            .collect::<Vec<_>>(),
    )
    .execute(uow.conn())
    .await?;
    if let (Some(own), Some(base)) = (entry.breakdown(), entry.base_breakdown()) {
        let people: Vec<i64> = own.people().collect();
        let get = |side: &std::collections::BTreeMap<i64, i64>| -> Vec<i64> {
            people
                .iter()
                .map(|p| side.get(p).copied().unwrap_or(0))
                .collect()
        };
        sqlx::query!(
            "insert into ledger_entry_shares (entry_id, user_id, paid, owed, paid_base, owed_base)
             select $1, * from unnest($2::bigint[], $3::bigint[], $4::bigint[], $5::bigint[], $6::bigint[])",
            id,
            &people,
            &get(&own.paid),
            &get(&own.owed),
            &get(&base.paid),
            &get(&base.owed),
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(())
}

/// Refresh everyone's view and push the people `change` concerns.
async fn announce(
    uow: &mut Uow,
    ctx: &Context,
    id: i64,
    entry: &Entry,
    change: Change,
    actor: Option<&Actor>,
) -> AppResult<()> {
    uow.publish(Audience::All, Event::LedgerUpdated { entry_id: id })
        .await?;
    let ids: Vec<i64> = entry.involved().into_iter().collect();
    let names: HashMap<i64, String> =
        sqlx::query!("select id, username from users where id = any($1)", &ids)
            .fetch_all(uow.conn())
            .await?
            .into_iter()
            .map(|row| (row.id, row.username))
            .collect();
    let pushes = announce::announcements(
        entry,
        change,
        actor.map(|a| announce::Actor {
            id: a.user_id,
            name: &a.username,
        }),
        &|user| names.get(&user).cloned().unwrap_or_default(),
    );
    for push in pushes {
        let notification = Notification {
            title: push.title,
            body: push.body,
            url: format!("/ledger/{id}"),
        };
        ctx.push.notify(uow, &push.user_ids, &notification).await?;
    }
    Ok(())
}
