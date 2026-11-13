//! Push notifications about entries, worded by kind. Everyone's view refreshes anyway (over
//! SSE); push goes only to the people an entry concerns.

use super::entry::{Details, Entry, EntryKind};
use super::money::{BASE_CURRENCY, format_amount};

/// What happened to an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Created,
    Updated,
    Confirmed,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    pub user_ids: Vec<i64>,
    pub title: String,
    pub body: String,
}

impl Push {
    fn to(user_id: i64, title: &str, body: String) -> Self {
        Self {
            user_ids: vec![user_id],
            title: title.to_owned(),
            body,
        }
    }
}

/// Who made the change: `None` is the system (a settled pact).
#[derive(Debug, Clone, Copy)]
pub struct Actor<'a> {
    pub id: i64,
    pub name: &'a str,
}

/// The notifications for `change` of `entry` by `actor`. `name` gives a member's username.
pub fn announcements(
    entry: &Entry,
    change: Change,
    actor: Option<Actor<'_>>,
    name: &dyn Fn(i64) -> String,
) -> Vec<Push> {
    let actor_name = actor.map_or("System", |a| a.name);
    let content = &entry.content;
    match &content.details {
        Details::Expense(_) => {
            let income = entry.kind() == EntryKind::Income;
            let title = match (change, income) {
                (Change::Created, false) => "Nowy wydatek",
                (Change::Updated, false) => "Wydatek zmieniony",
                (Change::Cancelled, false) => "Wydatek usunięty",
                (Change::Created, true) => "Nowy przychód",
                (Change::Updated, true) => "Przychód zmieniony",
                (Change::Cancelled, true) => "Przychód usunięty",
                _ => return Vec::new(),
            };
            let recipients: Vec<i64> = entry
                .involved()
                .into_iter()
                .filter(|user| Some(*user) != actor.map(|a| a.id))
                .collect();
            if recipients.is_empty() {
                return Vec::new();
            }
            let amount = format_amount(content.amount.unwrap_or(0), &content.currency, "");
            vec![Push {
                user_ids: recipients,
                title: title.to_owned(),
                body: format!("{actor_name}: {} ({amount})", content.title),
            }]
        }
        Details::Debt(details) => {
            let currency = if content.currency.is_empty() {
                BASE_CURRENCY
            } else {
                &content.currency
            };
            let mut pushes = Vec::new();
            for debt in &details.debts {
                let what = format_amount(debt.amount, currency, &debt.item);
                let (debtor, creditor) = (debt.debtor_id, debt.creditor_id);
                match (change, actor) {
                    (Change::Created, None) => pushes.push(Push::to(
                        creditor,
                        "Nowe rozliczenie",
                        format!("{} jest Ci winien {what}: {}", name(debtor), content.title),
                    )),
                    (Change::Created, Some(by)) if by.id == creditor => pushes.push(Push::to(
                        debtor,
                        "Nowe rozliczenie",
                        format!("{} zapisał(a), że jesteś winien(-na) {what}", by.name),
                    )),
                    (Change::Created, Some(by)) if by.id == debtor => pushes.push(Push::to(
                        creditor,
                        "Nowe rozliczenie",
                        format!("{} zapisał(a), że jest Ci winien(-na) {what}", by.name),
                    )),
                    (Change::Updated, _) => pushes.push(Push::to(
                        debtor,
                        "Dług zmieniony",
                        format!("{actor_name} zmienił(a) wpis: {} ({what})", content.title),
                    )),
                    (Change::Cancelled, _) => pushes.push(Push::to(
                        debtor,
                        "Wpis usunięty",
                        format!("{actor_name} usunął(-ęła) wpis: {what}"),
                    )),
                    _ => {}
                }
            }
            pushes
        }
        Details::Payment(payment) => {
            let goods = !content.item.is_empty();
            let what = format_amount(
                content.amount.unwrap_or(0),
                &content.currency,
                &content.item,
            );
            let noun = if goods { "zwrot" } else { "wpłatę" };
            let pick = |for_goods: &'static str, for_money: &'static str| {
                if goods { for_goods } else { for_money }
            };
            match change {
                Change::Created => vec![Push::to(
                    payment.to,
                    pick("Oddane do potwierdzenia", "Wpłata do potwierdzenia"),
                    format!(
                        "{actor_name} twierdzi, że {} Ci {what}",
                        pick("oddał(a)", "zapłacił(a)")
                    ),
                )],
                Change::Confirmed => vec![Push::to(
                    payment.from,
                    pick("Zwrot potwierdzony", "Wpłata potwierdzona"),
                    format!("{actor_name} potwierdził(a) {noun} {what}"),
                )],
                Change::Rejected => vec![Push::to(
                    payment.from,
                    pick("Zwrot odrzucony", "Wpłata odrzucona"),
                    format!(
                        "{actor_name} nie potwierdza {} {what}",
                        pick("zwrotu", "wpłaty")
                    ),
                )],
                Change::Cancelled => vec![Push::to(
                    payment.to,
                    pick("Zwrot wycofany", "Wpłata wycofana"),
                    format!("{actor_name} wycofał(a) {noun} {what}"),
                )],
                Change::Updated => Vec::new(),
            }
        }
    }
}
