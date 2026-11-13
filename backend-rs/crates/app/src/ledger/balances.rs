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
    // Per person and unit (an item, case-insensitive; "" for money): the net of confirmed
    // entries, and of confirmed plus pending ones.
    let nets = sqlx::query!(
        r#"select x.user_id as "user_id!", lower(o.item) as "unit!",
                  coalesce(sum(x.signed) filter (where e.status = 'confirmed'), 0)::bigint as "confirmed!",
                  sum(x.signed)::bigint as "planned!",
                  min(o.id) filter (where e.status = 'confirmed') as confirmed_first,
                  (array_agg(o.item order by o.id) filter (where e.status = 'confirmed'))[1] as confirmed_label,
                  min(o.id) as "planned_first!",
                  (array_agg(o.item order by o.id))[1] as "planned_label!"
           from ledger_obligations o
           join ledger_entries e on e.id = o.entry_id
           cross join lateral (values (o.creditor_id, o.amount), (o.debtor_id, -o.amount))
                as x(user_id, signed)
           where e.status in ('confirmed', 'pending')
           group by x.user_id, lower(o.item)"#,
    )
    .fetch_all(&ctx.db)
    .await?;
    let goods = sqlx::query!(
        r#"select o.debtor_id, o.creditor_id, lower(o.item) as "unit!",
                  sum(o.amount)::bigint as "total!"
           from ledger_obligations o
           join ledger_entries e on e.id = o.entry_id
           where e.status in ('confirmed', 'pending') and o.item <> ''
           group by o.debtor_id, o.creditor_id, lower(o.item)"#,
    )
    .fetch_all(&ctx.db)
    .await?;
    let waiting = sqlx::query!(
        r#"select id, item, amount, (details->>'from')::bigint as "payer!",
                  (details->>'to')::bigint as "receiver!"
           from ledger_entries where kind = 'payment' and status = 'pending'
           order by occurred_on desc, id desc"#,
    )
    .fetch_all(&ctx.db)
    .await?;

    // How each unit is spelled: as first recorded, in confirmed entries if there are any.
    let mut confirmed_labels: HashMap<&str, (i64, &str)> = HashMap::new();
    let mut planned_labels: HashMap<&str, (i64, &str)> = HashMap::new();
    for row in &nets {
        keep_first(
            &mut planned_labels,
            &row.unit,
            row.planned_first,
            &row.planned_label,
        );
        if let (Some(first), Some(label)) = (row.confirmed_first, &row.confirmed_label) {
            keep_first(&mut confirmed_labels, &row.unit, first, label);
        }
    }
    let label = |unit: &str| -> String {
        confirmed_labels
            .get(unit)
            .or_else(|| planned_labels.get(unit))
            .map_or_else(String::new, |(_, label)| (*label).to_owned())
    };

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
                .map(|g| (g.debtor_id, g.creditor_id, g.unit, g.total)),
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

fn keep_first<'a>(
    labels: &mut HashMap<&'a str, (i64, &'a str)>,
    unit: &'a str,
    id: i64,
    label: &'a str,
) {
    let known = labels.entry(unit).or_insert((id, label));
    if id < known.0 {
        *known = (id, label);
    }
}
