//! The group's Bilans: everyone's totals and who should pay whom, straight from the obligations.
//! Suggestions assume pending payments go through (so "Zapłać" does not ask for the same money
//! twice); those payments are listed separately until the receiver answers.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use domain::ledger::balances::{goods_pairs, money_transfers};
use domain::ledger::money::BASE_CURRENCY;
use serde::Serialize;
use utoipa::ToSchema;

use crate::accounts::{Person, persons};
use crate::{AppResult, Context};

/// One person's total in one unit. `net` > 0: the others owe them; < 0: they owe.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct MemberBalance {
    pub user: Person,
    /// Empty for money (grosze).
    pub item: String,
    pub net: i64,
}

/// `debtor` should give `creditor` `amount` (grosze, or a quantity of `item`).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Transfer {
    pub debtor: Person,
    pub creditor: Person,
    /// Empty for money.
    pub item: String,
    pub amount: i64,
}

/// A payment waiting for its receiver: `debtor` paid `creditor`.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PendingPayment {
    pub entry_id: i64,
    pub debtor: Person,
    pub creditor: Person,
    pub item: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Balances {
    pub currency: String,
    /// Confirmed entries only.
    pub members: Vec<MemberBalance>,
    /// What to pay to be even (money across the group, goods per pair and item).
    pub settlements: Vec<Transfer>,
    pub pending: Vec<PendingPayment>,
}

pub async fn balances(ctx: &Context) -> AppResult<Balances> {
    // Who owes whom, per unit (an item, case-insensitive; "" for money), split into confirmed
    // and pending: one pass with a hash aggregate, a row per pair, however long the history.
    let pairs = sqlx::query!(
        r#"select o.debtor_id, o.creditor_id, lower(o.item) as "unit!",
                  e.status = 'confirmed' as "confirmed!", sum(o.amount)::bigint as "total!"
           from ledger_obligations o
           join ledger_entries e on e.id = o.entry_id
           where e.status in ('confirmed', 'pending')
           group by 1, 2, 3, 4"#,
    )
    .fetch_all(&ctx.db)
    .await?;
    // How each item is spelled: as first recorded, in a confirmed entry if there is one.
    let labels: HashMap<String, String> = sqlx::query!(
        r#"select distinct on (lower(o.item)) lower(o.item) as "unit!", o.item
           from ledger_obligations o
           join ledger_entries e on e.id = o.entry_id
           where e.status in ('confirmed', 'pending') and o.item <> ''
           order by lower(o.item), e.status <> 'confirmed', o.id"#,
    )
    .fetch_all(&ctx.db)
    .await?
    .into_iter()
    .map(|r| (r.unit, r.item))
    .collect();
    let waiting = sqlx::query!(
        r#"select id, item, amount, (details->>'from')::bigint as "payer!",
                  (details->>'to')::bigint as "receiver!"
           from ledger_entries where kind = 'payment' and status = 'pending'
           order by occurred_on desc, id desc"#,
    )
    .fetch_all(&ctx.db)
    .await?;
    let label = |unit: &str| labels.get(unit).cloned().unwrap_or_default();

    // Each person's net per unit: confirmed only, and with pending entries assumed through.
    let mut nets: BTreeMap<(String, i64), Net> = BTreeMap::new();
    let mut goods: BTreeMap<(i64, i64, String), i64> = BTreeMap::new();
    for pair in &pairs {
        for (user, signed) in [
            (pair.creditor_id, pair.total),
            (pair.debtor_id, -pair.total),
        ] {
            let net = nets.entry((pair.unit.clone(), user)).or_default();
            net.planned += signed;
            if pair.confirmed {
                net.confirmed += signed;
            }
        }
        if !pair.unit.is_empty() {
            *goods
                .entry((pair.debtor_id, pair.creditor_id, pair.unit.clone()))
                .or_default() += pair.total;
        }
    }
    let nets: Vec<NetRow> = nets
        .into_iter()
        .map(|((unit, user_id), net)| NetRow {
            user_id,
            unit,
            confirmed: net.confirmed,
            planned: net.planned,
        })
        .collect();

    let money: BTreeMap<i64, i64> = nets
        .iter()
        .filter(|r| r.unit.is_empty())
        .map(|r| (r.user_id, r.planned))
        .collect();
    let mut transfers: Vec<(i64, i64, String, i64)> = money_transfers(&money)
        .into_iter()
        .map(|d| (d.debtor_id, d.creditor_id, String::new(), d.amount))
        .collect();
    transfers.extend(
        goods_pairs(
            goods
                .into_iter()
                .map(|((debtor, creditor, unit), total)| (debtor, creditor, unit, total)),
        )
        .into_iter()
        .map(|((debtor, creditor, unit), amount)| (debtor, creditor, label(&unit), amount)),
    );

    let mut ids: BTreeSet<i64> = nets.iter().map(|r| r.user_id).collect();
    ids.extend(transfers.iter().flat_map(|t| [t.0, t.1]));
    ids.extend(waiting.iter().flat_map(|w| [w.payer, w.receiver]));
    let people = persons(ctx, &ids.into_iter().collect::<Vec<_>>()).await?;
    let person = |id: i64| {
        people.get(&id).cloned().unwrap_or_else(|| Person {
            id,
            username: String::new(),
            avatar: None,
        })
    };

    let mut members: Vec<MemberBalance> = nets
        .iter()
        .filter(|r| r.confirmed != 0)
        .map(|r| MemberBalance {
            user: person(r.user_id),
            item: label(&r.unit),
            net: r.confirmed,
        })
        .collect();
    members.sort_by(|a, b| {
        (a.item.to_lowercase(), -a.net, &a.user.username).cmp(&(
            b.item.to_lowercase(),
            -b.net,
            &b.user.username,
        ))
    });
    let mut settlements: Vec<Transfer> = transfers
        .into_iter()
        .map(|(debtor, creditor, item, amount)| Transfer {
            debtor: person(debtor),
            creditor: person(creditor),
            item,
            amount,
        })
        .collect();
    settlements.sort_by(|a, b| {
        (a.item.to_lowercase(), -a.amount, &a.debtor.username).cmp(&(
            b.item.to_lowercase(),
            -b.amount,
            &b.debtor.username,
        ))
    });
    let pending = waiting
        .into_iter()
        .map(|w| PendingPayment {
            entry_id: w.id,
            debtor: person(w.payer),
            creditor: person(w.receiver),
            item: w.item,
            amount: w.amount.unwrap_or(0),
        })
        .collect();
    Ok(Balances {
        currency: BASE_CURRENCY.to_owned(),
        members,
        settlements,
        pending,
    })
}

#[derive(Debug, Default, Clone, Copy)]
struct Net {
    confirmed: i64,
    planned: i64,
}

/// One person's net in one unit.
struct NetRow {
    user_id: i64,
    unit: String,
    confirmed: i64,
    planned: i64,
}
