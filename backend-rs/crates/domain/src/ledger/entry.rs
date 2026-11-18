//! What an entry is and what it means.
//!
//! Every kind is a different way of arriving at the same fact, [`Debt`] (X owes Y n units):
//!
//! - `expense`: somebody paid for something others shared. Its breakdown (who paid, who owes
//!   what) becomes the obligations that settle it up.
//! - `income`: a negative expense. Somebody received money (a refund, a deposit back) that
//!   belongs to the people it is split among, so they are owed their parts.
//! - `debt`: exactly the debts given, from a settled pact or one a member records.
//! - `payment`: somebody paid somebody back. It is pending until the receiver confirms it, and
//!   then the receiver owes the payer that amount, which is what cancels the debt.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::money::{self, BASE_CURRENCY, Breakdown, Debt, settle_up, split_items};

text_enum!(
    /// What kind of thing happened.
    EntryKind {
        Expense = "expense",
        Income = "income",
        Debt = "debt",
        Payment = "payment",
    }
);

text_enum!(
    /// Only `confirmed` entries count. Cancelling (deleting an expense, taking a payment back)
    /// is the only way one stops counting; nothing is ever deleted.
    Status as LedgerStatus {
        /// A payment waiting for the receiver.
        Pending = "pending",
        Confirmed = "confirmed",
        /// The receiver says they weren't paid.
        Rejected = "rejected",
        /// Withdrawn: a deleted expense, a payment taken back.
        Cancelled = "cancelled",
    }
);

text_enum!(
    /// Where an entry comes from.
    Source {
        /// Recorded by a member.
        Manual = "manual",
        /// A pact's settlement (`pact_id`); changes only through the pact.
        Pact = "pact",
        /// Imported from Tricount.
        Tricount = "tricount",
        /// A debt from before sources were recorded.
        Legacy = "legacy",
    }
);

text_enum!(
    /// An expense's category. Keys are stored on entries: rename a label freely, never a key.
    Category {
        Lodging = "lodging",
        Bills = "bills",
        Groceries = "groceries",
        Fun = "fun",
        Health = "health",
        Insurance = "insurance",
        Transport = "transport",
        Food = "food",
        Shopping = "shopping",
        Weed = "weed",
        Other = "other",
    }
);

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lodging => "Zakwaterowanie",
            Self::Bills => "Opłaty",
            Self::Groceries => "Zakupy spożywcze",
            Self::Fun => "Rozrywka",
            Self::Health => "Opieka zdrowotna",
            Self::Insurance => "Ubezpieczenie",
            Self::Transport => "Transport",
            Self::Food => "Restauracje i bary",
            Self::Shopping => "Shopping",
            Self::Weed => "Zioło",
            Self::Other => "Inne",
        }
    }

    /// Kept apart from the name; every category has one.
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Lodging => "🏠",
            Self::Bills => "🧾",
            Self::Groceries => "🛒",
            Self::Fun => "🎉",
            Self::Health => "💊",
            Self::Insurance => "🛡️",
            Self::Transport => "🚗",
            Self::Food => "🍽️",
            Self::Shopping => "🛍️",
            Self::Weed => "🌿",
            Self::Other => "📦",
        }
    }
}

text_enum!(
    /// How an expense item is divided among its people.
    #[derive(Default)]
    Split {
        /// Evenly (weights are ignored).
        #[default]
        Equal = "equal",
        /// In proportion to the weights.
        Shares = "shares",
        /// The weights are the amounts, adding up to the item's amount.
        Exact = "exact",
    }
);

/// Who paid (or, for an income, received) how much, in the entry's currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Payer {
    pub user_id: i64,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Share {
    pub user_id: i64,
    /// 1 for `equal`; a weight for `shares`; the amount for `exact`.
    pub weight: i64,
}

/// One line of an expense, shared by its own people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Item {
    pub name: String,
    pub amount: i64,
    pub split: Split,
    pub shares: Vec<Share>,
}

/// An expense or income: payers and items both add up to the total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct ExpenseDetails {
    pub payers: Vec<Payer>,
    pub items: Vec<Item>,
}

/// The debts as recorded (money in the entry's currency, or goods).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct DebtDetails {
    pub debts: Vec<Debt>,
}

/// `from` paid `to` back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct PaymentDetails {
    pub from: i64,
    pub to: i64,
}

/// Kind-specific content, stored as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum Details {
    Expense(ExpenseDetails),
    Debt(DebtDetails),
    Payment(PaymentDetails),
}

impl Details {
    /// Parse stored details of an entry of `kind`.
    pub fn from_json(kind: EntryKind, value: &Value) -> Result<Self, serde_json::Error> {
        Ok(match kind {
            EntryKind::Expense | EntryKind::Income => {
                Self::Expense(serde_json::from_value(value.clone())?)
            }
            EntryKind::Debt => Self::Debt(serde_json::from_value(value.clone())?),
            EntryKind::Payment => Self::Payment(serde_json::from_value(value.clone())?),
        })
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// What a member (or a pact) says happened: everything stored about an entry except the
/// bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub kind: EntryKind,
    pub title: String,
    pub note: String,
    pub category: Option<Category>,
    pub occurred_on: NaiveDate,
    /// The headline amount as typed: minor units of `currency`, or with `item` a quantity.
    /// `None` when an entry mixes units (a pot plus stakes in kind).
    pub amount: Option<i64>,
    /// Empty for goods (and mixed debts).
    pub currency: String,
    pub item: String,
    pub details: Details,
}

impl Content {
    /// Whether the content needs an exchange rate (money in a currency other than PLN).
    pub fn foreign_currency(&self) -> Option<&str> {
        let currency = self.currency.as_str();
        (!currency.is_empty() && currency != BASE_CURRENCY).then_some(currency)
    }
}

/// An entry with what is needed to tell what it means and who may do what with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub status: Status,
    pub source: Source,
    pub created_by: Option<i64>,
    pub content: Content,
    /// Base-currency units per unit of `content.currency`; `None` for PLN and goods.
    pub rate: Option<Decimal>,
}

impl Entry {
    pub fn kind(&self) -> EntryKind {
        self.content.kind
    }

    /// +1 for an expense (payers are owed their outlay), -1 for an income (receivers owe it on).
    fn sign(&self) -> i64 {
        if self.kind() == EntryKind::Income {
            -1
        } else {
            1
        }
    }

    fn exponent(&self) -> u32 {
        money::exponent(&self.content.currency).unwrap_or(2)
    }

    /// Who paid and who owes what, in the entry's currency: the explanation of an expense or
    /// income (its obligations are not one). `None` for other kinds.
    pub fn breakdown(&self) -> Option<Breakdown> {
        let Details::Expense(details) = &self.content.details else {
            return None;
        };
        let items: Vec<(i64, BTreeMap<i64, i64>)> = details
            .items
            .iter()
            .map(|item| {
                let weights = item.shares.iter().map(|s| (s.user_id, s.weight)).collect();
                (item.amount, weights)
            })
            .collect();
        Some(Breakdown {
            paid: details
                .payers
                .iter()
                .map(|p| (p.user_id, p.amount))
                .collect(),
            owed: split_items(&items),
        })
    }

    /// The breakdown in the base currency (converted once, still balancing to the grosz).
    pub fn base_breakdown(&self) -> Option<Breakdown> {
        let breakdown = self.breakdown()?;
        Some(match self.rate {
            Some(rate) => breakdown.to_base(rate, self.exponent()),
            None => breakdown,
        })
    }

    /// The debts as recorded, in the entry's currency.
    pub fn recorded_debts(&self) -> &[Debt] {
        match &self.content.details {
            Details::Debt(details) => &details.debts,
            _ => &[],
        }
    }

    /// What the entry means, in base-currency minor units or quantities of goods.
    pub fn obligations(&self) -> Vec<Debt> {
        match &self.content.details {
            Details::Expense(_) => {
                let Some(breakdown) = self.base_breakdown() else {
                    return Vec::new();
                };
                let sign = self.sign();
                let nets: BTreeMap<i64, i64> = breakdown
                    .nets()
                    .into_iter()
                    .map(|(user, net)| (user, sign * net))
                    .collect();
                // Both sides of a breakdown add up to the same total, so the nets balance;
                // anything else is a bug upstream, loud in debug builds and tests.
                settle_up(&nets, "").unwrap_or_else(|money::Unbalanced| {
                    debug_assert!(false, "unbalanced expense nets: {nets:?}");
                    Vec::new()
                })
            }
            Details::Debt(details) => match self.rate {
                None => details.debts.clone(),
                Some(rate) => details
                    .debts
                    .iter()
                    .map(|debt| {
                        if debt.item.is_empty() {
                            Debt {
                                amount: money::to_base(debt.amount, rate, self.exponent()),
                                ..debt.clone()
                            }
                        } else {
                            debt.clone()
                        }
                    })
                    .filter(|debt| debt.amount > 0) // a few yen can round to nothing
                    .collect(),
            },
            Details::Payment(payment) => vec![Debt {
                debtor_id: payment.to,
                creditor_id: payment.from,
                amount: self.content.amount.unwrap_or(0),
                item: self.content.item.clone(),
            }],
        }
    }

    /// The headline amount in the base currency; `None` for goods and mixed entries.
    pub fn base_amount(&self) -> Option<i64> {
        match &self.content.details {
            Details::Expense(_) => self.base_breakdown().map(|b| b.total()),
            Details::Debt(_) => {
                if self.content.amount.is_none() || !self.content.item.is_empty() {
                    return None;
                }
                Some(self.obligations().iter().map(|d| d.amount).sum())
            }
            Details::Payment(_) => self
                .content
                .amount
                .filter(|_| self.content.currency == BASE_CURRENCY),
        }
    }

    /// Everyone the entry concerns.
    pub fn involved(&self) -> BTreeSet<i64> {
        match &self.content.details {
            Details::Expense(_) => self
                .breakdown()
                .map(|b| b.people().collect())
                .unwrap_or_default(),
            Details::Debt(details) => details
                .debts
                .iter()
                .flat_map(|d| [d.debtor_id, d.creditor_id])
                .collect(),
            Details::Payment(payment) => [payment.from, payment.to].into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::*;

    fn expense(kind: EntryKind, currency: &str, rate: Option<&str>) -> Entry {
        let item = Item {
            name: "Pizza".into(),
            amount: 9000,
            split: Split::Equal,
            shares: [1, 2, 3]
                .into_iter()
                .map(|user_id| Share { user_id, weight: 1 })
                .collect(),
        };
        Entry {
            status: Status::Confirmed,
            source: Source::Manual,
            created_by: Some(1),
            rate: rate.map(|r| Decimal::from_str(r).unwrap()),
            content: Content {
                kind,
                title: "Pizza".into(),
                note: String::new(),
                category: Some(Category::Food),
                occurred_on: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                amount: Some(9000),
                currency: currency.into(),
                item: String::new(),
                details: Details::Expense(ExpenseDetails {
                    payers: vec![Payer {
                        user_id: 1,
                        amount: 9000,
                    }],
                    items: vec![item],
                }),
            },
        }
    }

    #[test]
    fn an_expense_is_owed_to_its_payer_and_an_income_the_other_way() {
        let spent = expense(EntryKind::Expense, "PLN", None);
        assert_eq!(
            spent.obligations(),
            [Debt::money(2, 1, 3000), Debt::money(3, 1, 3000)]
        );
        assert_eq!(spent.base_amount(), Some(9000));
        let received = expense(EntryKind::Income, "PLN", None);
        assert_eq!(
            received.obligations(),
            [Debt::money(1, 2, 3000), Debt::money(1, 3, 3000)]
        );
        assert_eq!(received.involved(), BTreeSet::from([1, 2, 3]));
    }

    #[test]
    fn a_foreign_expense_is_converted_once() {
        let entry = expense(EntryKind::Expense, "EUR", Some("4.25"));
        assert_eq!(entry.base_amount(), Some(38250));
        let owed: i64 = entry.obligations().iter().map(|d| d.amount).sum();
        assert_eq!(owed, 38250 / 3 * 2);
    }

    #[test]
    fn details_round_trip_in_the_stored_shapes() {
        let payment = serde_json::json!({"from": 2, "to": 1});
        let parsed = Details::from_json(EntryKind::Payment, &payment).unwrap();
        assert_eq!(parsed, Details::Payment(PaymentDetails { from: 2, to: 1 }));
        assert_eq!(parsed.to_json(), payment);
        let debt = serde_json::json!({"debts": [{"debtor_id": 2, "creditor_id": 1, "amount": 5, "item": "piwo"}]});
        assert_eq!(
            Details::from_json(EntryKind::Debt, &debt)
                .unwrap()
                .to_json(),
            debt
        );
        assert!(Details::from_json(EntryKind::Expense, &payment).is_err());
    }

    #[test]
    fn every_category_has_a_label_and_an_emoji() {
        for category in Category::ALL {
            assert!(!category.label().is_empty() && !category.emoji().is_empty());
            assert_eq!(Category::parse(category.as_str()), Some(*category));
        }
    }
}
