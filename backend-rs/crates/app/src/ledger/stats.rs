//! Spending statistics (the Statystyki view): what the group spent in a period, by category,
//! by person and over time, in PLN. Computed from the materialized per-person shares (expenses,
//! incomes) and money obligations (debts), so the numbers agree with the entries to the grosz.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use domain::DomainError;
use domain::ledger::Category;
use domain::ledger::money::BASE_CURRENCY;
use domain::ledger::stats::{Bucket, DEBTS, monthly_average, months};
use serde::Serialize;
use utoipa::ToSchema;

use crate::accounts::{Person, persons};
use crate::clock::local_today;
use crate::{AppResult, Context};

/// Which days: both ends included; a missing end is open.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatsQuery {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StatsCategory {
    /// An entry category, or `debts`.
    pub key: String,
    pub label: String,
    pub emoji: String,
    /// Grosze, net of incomes (can be negative).
    pub spent: i64,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct StatsPerson {
    pub user: Person,
    /// Their part of what was spent, in grosze.
    pub share: i64,
    /// What they paid out, in grosze.
    pub paid: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StatsPoint {
    /// `YYYY-MM-DD` for a day, `YYYY-MM` for a month.
    pub period: String,
    pub spent: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Stats {
    pub currency: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// What `series` is bucketed by (days up to 62 days, else months).
    pub unit: Bucket,
    /// Expenses and money debts minus incomes, in grosze.
    pub spent: i64,
    pub expenses: i64,
    pub income: i64,
    /// Entries in the period (goods debts included).
    pub count: i64,
    /// Over periods longer than a month.
    pub monthly_average: Option<i64>,
    /// Biggest first.
    pub categories: Vec<StatsCategory>,
    /// Biggest share first.
    pub people: Vec<StatsPerson>,
    /// Zero-filled, oldest first.
    pub series: Vec<StatsPoint>,
}

/// Bounds standing in for "no limit": far outside any date an entry can have.
const EARLIEST: NaiveDate = NaiveDate::from_ymd_opt(1, 1, 1).expect("a valid date");
const LATEST: NaiveDate = NaiveDate::from_ymd_opt(9999, 12, 31).expect("a valid date");

pub async fn stats(ctx: &Context, query: StatsQuery) -> AppResult<Stats> {
    if let (Some(start), Some(end)) = (query.start, query.end)
        && start > end
    {
        return Err(
            DomainError::new("start_after_end", "Początek nie może być po końcu.")
                .on("start")
                .into(),
        );
    }
    // Concrete bounds rather than `$1 is null or ...`: the statement is prepared once and
    // Postgres soon plans it generically, and only a plain range keeps the date index usable.
    let from = query.start.unwrap_or(EARLIEST);
    let to = query.end.unwrap_or(LATEST);
    let rows = sqlx::query!(
        // Each entry's total (its shares' paid amounts, or a debt's money obligations) is looked
        // up once per entry through its index: a month costs a few hundred lookups.
        r#"select e.kind as "kind!",
                  case when e.kind = 'debt' then 'debts'
                       when e.category = '' then 'other' else e.category end as "key!",
                  e.occurred_on as "occurred_on!",
                  coalesce(sum(t.total), 0)::bigint as "total!",
                  count(*) filter (where e.kind <> 'debt' or t.total <> 0) as "counted!",
                  count(*) as "entries!"
           from ledger_entries e
           cross join lateral (
               select case when e.kind = 'debt'
                           then (select coalesce(sum(o.amount), 0) from ledger_obligations o
                                 where o.entry_id = e.id and o.item = '')
                           else (select coalesce(sum(s.paid_base), 0) from ledger_entry_shares s
                                 where s.entry_id = e.id) end as total
               offset 0) t
           where e.status = 'confirmed' and e.kind in ('expense', 'income', 'debt')
             and e.occurred_on between $1 and $2
           group by 1, 2, 3"#,
        from,
        to,
    )
    .fetch_all(&ctx.db)
    .await?;
    let shares = sqlx::query!(
        r#"select user_id as "user_id!", sum(share)::bigint as "share!", sum(paid)::bigint as "paid!"
           from (
               select s.user_id,
                      case when e.kind = 'income' then -s.owed_base else s.owed_base end as share,
                      case when e.kind = 'income' then -s.paid_base else s.paid_base end as paid
               from ledger_entry_shares s join ledger_entries e on e.id = s.entry_id
               where e.status = 'confirmed' and e.kind in ('expense', 'income')
                 and e.occurred_on between $1 and $2
               union all
               select side.user_id, side.share, side.paid
               from ledger_obligations o join ledger_entries e on e.id = o.entry_id
               cross join lateral (values (o.debtor_id, o.amount, 0::bigint),
                                          (o.creditor_id, 0::bigint, o.amount))
                    as side(user_id, share, paid)
               where e.status = 'confirmed' and e.kind = 'debt' and o.item = ''
                 and e.occurred_on between $1 and $2
           ) parts
           group by user_id"#,
        from,
        to,
    )
    .fetch_all(&ctx.db)
    .await?;

    let (mut expenses, mut income, mut count) = (0_i64, 0_i64, 0_i64);
    let mut by_category: HashMap<String, (i64, i64)> = HashMap::new();
    let mut by_day: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for row in &rows {
        count += row.entries;
        let spent = if row.kind == "income" {
            income += row.total;
            -row.total
        } else {
            expenses += row.total;
            row.total
        };
        if row.counted > 0 {
            let category = by_category.entry(row.key.clone()).or_default();
            category.0 += spent;
            category.1 += row.counted;
            *by_day.entry(row.occurred_on).or_default() += spent;
        }
    }

    let first = rows.iter().map(|r| r.occurred_on).min();
    let last = rows.iter().map(|r| r.occurred_on).max();
    let begin = query
        .start
        .or(first)
        .or(query.end)
        .unwrap_or_else(|| local_today(ctx.clock.now()));
    let finish = query.end.or(last).unwrap_or(begin);
    let unit = Bucket::for_period(begin, finish);
    let mut series: BTreeMap<String, i64> = unit
        .all(begin, finish)
        .into_iter()
        .map(|period| (period, 0))
        .collect();
    for (day, spent) in by_day {
        *series.entry(unit.label(day)).or_default() += spent;
    }

    let mut categories: Vec<StatsCategory> = by_category
        .into_iter()
        .map(|(key, (spent, count))| {
            let (label, emoji) = if key == DEBTS.0 {
                (DEBTS.1, DEBTS.2)
            } else {
                Category::parse(&key).map_or(("", ""), |c| (c.label(), c.emoji()))
            };
            StatsCategory {
                label: label.to_owned(),
                emoji: emoji.to_owned(),
                key,
                spent,
                count,
            }
        })
        .collect();
    categories.sort_by(|a, b| b.spent.cmp(&a.spent).then_with(|| a.key.cmp(&b.key)));

    let ids: Vec<i64> = shares.iter().map(|s| s.user_id).collect();
    let people = persons(ctx, &ids).await?;
    let mut shares = shares;
    shares.sort_by_key(|s| (-s.share, s.user_id));
    let spent = expenses - income;
    Ok(Stats {
        currency: BASE_CURRENCY.to_owned(),
        start: begin,
        end: finish,
        unit,
        spent,
        expenses,
        income,
        count,
        monthly_average: monthly_average(spent, months(begin, finish)),
        categories,
        people: shares
            .into_iter()
            .map(|s| StatsPerson {
                user: people.get(&s.user_id).cloned().unwrap_or_else(|| Person {
                    id: s.user_id,
                    username: String::new(),
                    avatar: None,
                }),
                share: s.share,
                paid: s.paid,
            })
            .collect(),
        series: series
            .into_iter()
            .map(|(period, spent)| StatsPoint { period, spent })
            .collect(),
    })
}
