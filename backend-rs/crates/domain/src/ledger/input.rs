//! What members send, and turning it into [`Content`]: everything about an entry that can be
//! checked without the database. Who is a member is checked by the caller ([`Draft::members`]).

use chrono::NaiveDate;
use serde::Deserialize;

use super::entry::{
    Category, Content, DebtDetails, Details, EntryKind, ExpenseDetails, Item, Payer,
    PaymentDetails, Share, Split,
};
use super::money::{self, BASE_CURRENCY, Debt, MAX_AMOUNT, format_amount};
use crate::DomainError;

pub const TITLE_MAX: usize = 200;
pub const NOTE_MAX: usize = 500;
pub const ITEM_NAME_MAX: usize = 100;
pub const GOODS_MAX: usize = 60;
pub const ITEMS_MAX: usize = 200;
/// Pieces of goods one member may record as owed.
pub const MAX_GOODS: i64 = 1000;

fn base_currency() -> String {
    BASE_CURRENCY.to_owned()
}

fn one() -> i64 {
    1
}

/// Somebody paid for something the others shared (or, as an income, received money the others
/// share: then `payers` are who received it). A plain expense is a single item.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct ExpenseInput {
    pub title: String,
    /// Defaults to today; not in the future.
    #[serde(default)]
    pub occurred_on: Option<NaiveDate>,
    #[serde(default = "base_currency")]
    pub currency: String,
    /// Defaults to `other`.
    #[serde(default)]
    pub category: Option<Category>,
    #[serde(default)]
    pub note: String,
    /// Amounts in minor units of `currency`, adding up to the items' total.
    pub payers: Vec<Payer>,
    /// Up to 200 lines, each shared by its own people.
    pub items: Vec<ItemInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct ItemInput {
    pub name: String,
    pub amount: i64,
    #[serde(default)]
    pub split: Split,
    pub shares: Vec<ShareInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct ShareInput {
    pub user_id: i64,
    /// Ignored for `equal`; a weight for `shares`; the amount for `exact`.
    #[serde(default = "one")]
    pub weight: i64,
}

/// A debt you are a party to: money ("50 zł", any currency) or goods ("2 x kawa"). It counts at
/// once and only the person owed can change it.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct DebtInput {
    pub debtor_id: i64,
    /// Defaults to you.
    #[serde(default)]
    pub creditor_id: Option<i64>,
    /// Defaults to the item, else "Dług".
    #[serde(default)]
    pub title: String,
    /// Empty: money.
    #[serde(default)]
    pub item: String,
    /// Minor units of `currency`, or the quantity of `item`.
    pub amount: i64,
    #[serde(default = "base_currency")]
    pub currency: String,
    #[serde(default)]
    pub occurred_on: Option<NaiveDate>,
    #[serde(default)]
    pub note: String,
}

/// You paid somebody back (grosze, or goods returned); it counts once they confirm.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct PaymentInput {
    pub to_user_id: i64,
    pub amount: i64,
    #[serde(default)]
    pub item: String,
    #[serde(default)]
    pub occurred_on: Option<NaiveDate>,
    #[serde(default)]
    pub note: String,
}

/// Validated content, plus the people who must be members for it to be stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub content: Content,
    /// Every person the entry names, sorted.
    pub members: Vec<i64>,
    /// The field an unknown or inactive person is reported on.
    pub members_field: &'static str,
}

/// An expense's or an income's wording.
struct Words {
    title: &'static str,
    payers_required: &'static str,
    payer_twice: &'static str,
    payers_total: &'static str,
}

const EXPENSE: Words = Words {
    title: "Podaj, za co był wydatek.",
    payers_required: "Podaj, kto płacił.",
    payer_twice: "Każdy płacący może wystąpić tylko raz.",
    payers_total: "Płacący muszą razem wyłożyć",
};

const INCOME: Words = Words {
    title: "Podaj, za co był przychód.",
    payers_required: "Podaj, kto otrzymał pieniądze.",
    payer_twice: "Każdy otrzymujący może wystąpić tylko raz.",
    payers_total: "Otrzymujący muszą razem dostać",
};

/// Whitespace collapsed to single spaces.
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn too_long(text: &str, max: usize) -> bool {
    text.chars().count() > max
}

/// A free-text item ("  Piwo " → "Piwo"); empty means money.
pub fn normalize_item(item: &str, field: &'static str) -> Result<String, DomainError> {
    let item = collapse(item);
    if too_long(&item, GOODS_MAX) {
        return Err(DomainError::new(
            "item_too_long",
            format!("Nazwa może mieć najwyżej {GOODS_MAX} znaków."),
        )
        .on(field));
    }
    Ok(item)
}

fn title(raw: &str, missing: &'static str) -> Result<String, DomainError> {
    let title = collapse(raw);
    if title.is_empty() {
        return Err(DomainError::new("title_required", missing).on("title"));
    }
    if too_long(&title, TITLE_MAX) {
        return Err(DomainError::new(
            "title_too_long",
            format!("Tytuł może mieć najwyżej {TITLE_MAX} znaków."),
        )
        .on("title"));
    }
    Ok(title)
}

fn note(raw: &str) -> Result<String, DomainError> {
    let note = raw.trim();
    if too_long(note, NOTE_MAX) {
        return Err(DomainError::new(
            "note_too_long",
            format!("Notatka może mieć najwyżej {NOTE_MAX} znaków."),
        )
        .on("note"));
    }
    Ok(note.to_owned())
}

fn date(given: Option<NaiveDate>, today: NaiveDate) -> Result<NaiveDate, DomainError> {
    let day = given.unwrap_or(today);
    if day > today {
        return Err(
            DomainError::new("future_date", "Data nie może być z przyszłości.").on("occurred_on"),
        );
    }
    Ok(day)
}

fn currency(code: &str) -> Result<String, DomainError> {
    if money::exponent(code).is_none() {
        return Err(
            DomainError::new("unsupported_currency", "Nieobsługiwana waluta.").on("currency"),
        );
    }
    Ok(code.to_owned())
}

/// A positive amount of at most [`MAX_AMOUNT`].
pub fn amount(value: i64, field: &'static str) -> Result<i64, DomainError> {
    if value < 1 {
        return Err(DomainError::new("invalid_amount", "Kwota musi być dodatnia.").on(field));
    }
    if value > MAX_AMOUNT {
        return Err(DomainError::new("amount_too_large", "Kwota jest za duża.").on(field));
    }
    Ok(value)
}

/// An expense (`EntryKind::Expense`) or an income (`EntryKind::Income`).
pub fn expense(
    kind: EntryKind,
    input: &ExpenseInput,
    today: NaiveDate,
) -> Result<Draft, DomainError> {
    let words = if kind == EntryKind::Income {
        &INCOME
    } else {
        &EXPENSE
    };
    let currency = currency(&input.currency)?;
    let title = title(&input.title, words.title)?;
    let occurred_on = date(input.occurred_on, today)?;
    let note = note(&input.note)?;
    if input.items.is_empty() {
        return Err(
            DomainError::new("items_required", "Dodaj co najmniej jedną pozycję.").on("items"),
        );
    }
    if input.items.len() > ITEMS_MAX {
        return Err(DomainError::new(
            "too_many_items",
            format!("Wydatek może mieć najwyżej {ITEMS_MAX} pozycji."),
        )
        .on("items"));
    }
    let items = input
        .items
        .iter()
        .map(|item| clean_item(item, &currency))
        .collect::<Result<Vec<_>, _>>()?;
    let total = amount(items.iter().map(|i| i.amount).sum(), "items")?;

    if input.payers.is_empty() {
        return Err(DomainError::new("payers_required", words.payers_required).on("payers"));
    }
    let mut payers = input.payers.clone();
    payers.sort_by_key(|p| p.user_id);
    for payer in &payers {
        amount(payer.amount, "payers")?;
    }
    if payers
        .windows(2)
        .any(|pair| pair[0].user_id == pair[1].user_id)
    {
        return Err(DomainError::new("payer_twice", words.payer_twice).on("payers"));
    }
    if payers.iter().map(|p| p.amount).sum::<i64>() != total {
        let total = format_amount(total, &currency, "");
        return Err(
            DomainError::new("payers_total", format!("{} {total}.", words.payers_total))
                .on("payers"),
        );
    }

    let mut members: Vec<i64> = payers
        .iter()
        .map(|p| p.user_id)
        .chain(
            items
                .iter()
                .flat_map(|i| i.shares.iter().map(|s| s.user_id)),
        )
        .collect();
    members.sort_unstable();
    members.dedup();
    Ok(Draft {
        content: Content {
            kind,
            title,
            note,
            category: Some(input.category.unwrap_or(Category::Other)),
            occurred_on,
            amount: Some(total),
            currency,
            item: String::new(),
            details: Details::Expense(ExpenseDetails { payers, items }),
        },
        members,
        members_field: "items",
    })
}

fn clean_item(item: &ItemInput, currency: &str) -> Result<Item, DomainError> {
    let name = collapse(&item.name);
    if name.is_empty() {
        return Err(
            DomainError::new("item_name_required", "Każda pozycja potrzebuje nazwy.").on("items"),
        );
    }
    if too_long(&name, ITEM_NAME_MAX) {
        return Err(DomainError::new(
            "item_name_too_long",
            format!("Nazwa pozycji może mieć najwyżej {ITEM_NAME_MAX} znaków."),
        )
        .on("items"));
    }
    let amount = amount(item.amount, "items")?;
    if item.shares.is_empty() {
        return Err(DomainError::new(
            "item_people_required",
            format!("„{name}”: wybierz, kto dzieli tę pozycję."),
        )
        .on("items"));
    }
    let mut shares: Vec<Share> = item
        .shares
        .iter()
        .map(|s| Share {
            user_id: s.user_id,
            weight: if item.split == Split::Equal {
                1
            } else {
                s.weight
            },
        })
        .collect();
    shares.sort_by_key(|s| s.user_id);
    if shares
        .windows(2)
        .any(|pair| pair[0].user_id == pair[1].user_id)
    {
        return Err(DomainError::new(
            "person_twice",
            format!("„{name}”: każda osoba może wystąpić tylko raz."),
        )
        .on("items"));
    }
    if shares.iter().any(|s| s.weight < 1 || s.weight > MAX_AMOUNT) {
        return Err(DomainError::new(
            "invalid_weight",
            format!("„{name}”: udziały muszą być dodatnie."),
        )
        .on("items"));
    }
    if item.split == Split::Exact && shares.iter().map(|s| s.weight).sum::<i64>() != amount {
        let amount = format_amount(amount, currency, "");
        return Err(DomainError::new(
            "exact_total",
            format!("„{name}”: kwoty muszą dać razem {amount}."),
        )
        .on("items"));
    }
    Ok(Item {
        name,
        amount,
        split: item.split,
        shares,
    })
}

/// A debt recorded by `author`, who must be one of its parties.
pub fn debt(input: &DebtInput, author: i64, today: NaiveDate) -> Result<Draft, DomainError> {
    let debtor = input.debtor_id;
    let creditor = input.creditor_id.unwrap_or(author);
    if author != debtor && author != creditor {
        return Err(DomainError::new(
            "not_your_debt",
            "Możesz zapisać tylko swój dług albo dług wobec Ciebie.",
        )
        .on("debtor_id"));
    }
    let item = normalize_item(&input.item, "item")?;
    let amount = amount(input.amount, "amount")?;
    if !item.is_empty() && amount > MAX_GOODS {
        return Err(
            DomainError::new("too_many_goods", format!("Najwyżej {MAX_GOODS} sztuk.")).on("amount"),
        );
    }
    let currency = if item.is_empty() {
        currency(&input.currency)?
    } else {
        BASE_CURRENCY.to_owned()
    };
    let named = collapse(&input.title);
    let fallback = if item.is_empty() { "Dług" } else { &item };
    let title = title(if named.is_empty() { fallback } else { &named }, "")?;
    let mut draft = debts(
        vec![Debt {
            debtor_id: debtor,
            creditor_id: creditor,
            amount,
            item,
        }],
        &title,
        date(input.occurred_on, today)?,
        &currency,
    )?;
    draft.content.note = note(&input.note)?;
    draft.members_field = "debtor_id";
    Ok(draft)
}

/// A debt entry for `debts` (money in `currency`), e.g. a pact's settlement. One unit gives a
/// headline amount; a mix (a pot plus stakes in kind) has none.
pub fn debts(
    debts: Vec<Debt>,
    title: &str,
    on: NaiveDate,
    currency: &str,
) -> Result<Draft, DomainError> {
    let debts = debts
        .into_iter()
        .map(|d| {
            Ok(Debt {
                item: normalize_item(&d.item, "debts")?,
                amount: amount(d.amount, "amount")?,
                ..d
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    if debts.iter().any(|d| d.debtor_id == d.creditor_id) {
        return Err(
            DomainError::new("self_debt", "Nie można być dłużnikiem samego siebie.")
                .on("debtor_id"),
        );
    }
    let first_unit = debts.first().map(|d| d.item.to_lowercase());
    let single = debts
        .iter()
        .all(|d| Some(d.item.to_lowercase()) == first_unit);
    let item = if single {
        debts.first().map(|d| d.item.clone()).unwrap_or_default()
    } else {
        String::new()
    };
    let mut members: Vec<i64> = debts
        .iter()
        .flat_map(|d| [d.debtor_id, d.creditor_id])
        .collect();
    members.sort_unstable();
    members.dedup();
    Ok(Draft {
        content: Content {
            kind: EntryKind::Debt,
            title: title.chars().take(TITLE_MAX).collect(),
            note: String::new(),
            category: None,
            occurred_on: on,
            amount: single.then(|| debts.iter().map(|d| d.amount).sum()),
            currency: if single && item.is_empty() {
                currency.to_owned()
            } else {
                String::new()
            },
            item,
            details: Details::Debt(DebtDetails { debts }),
        },
        members,
        members_field: "debts",
    })
}

/// `author` paid `to_user_id` back.
pub fn payment(input: &PaymentInput, author: i64, today: NaiveDate) -> Result<Draft, DomainError> {
    let item = normalize_item(&input.item, "item")?;
    let amount = amount(input.amount, "amount")?;
    if input.to_user_id == author {
        return Err(
            DomainError::new("self_payment", "Nie możesz zapłacić samemu sobie.").on("to_user_id"),
        );
    }
    Ok(Draft {
        content: Content {
            kind: EntryKind::Payment,
            title: if item.is_empty() { "Spłata" } else { "Zwrot" }.to_owned(),
            note: note(&input.note)?,
            category: None,
            occurred_on: date(input.occurred_on, today)?,
            amount: Some(amount),
            currency: if item.is_empty() {
                BASE_CURRENCY.to_owned()
            } else {
                String::new()
            },
            item,
            details: Details::Payment(PaymentDetails {
                from: author,
                to: input.to_user_id,
            }),
        },
        members: vec![input.to_user_id],
        members_field: "to_user_id",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
    }

    fn pizza() -> ExpenseInput {
        ExpenseInput {
            title: "  Pizza\t na mieście ".into(),
            occurred_on: None,
            currency: "PLN".into(),
            category: None,
            note: String::new(),
            payers: vec![Payer {
                user_id: 1,
                amount: 1000,
            }],
            items: vec![ItemInput {
                name: "Pizza".into(),
                amount: 1000,
                split: Split::Equal,
                shares: vec![
                    ShareInput {
                        user_id: 2,
                        weight: 7,
                    },
                    ShareInput {
                        user_id: 1,
                        weight: 1,
                    },
                ],
            }],
        }
    }

    fn code(result: Result<Draft, DomainError>) -> &'static str {
        result.unwrap_err().code
    }

    #[test]
    fn an_expense_is_tidied() {
        let draft = expense(EntryKind::Expense, &pizza(), today()).unwrap();
        let content = &draft.content;
        assert_eq!(content.title, "Pizza na mieście");
        assert_eq!(
            (content.occurred_on, content.category),
            (today(), Some(Category::Other))
        );
        let Details::Expense(details) = &content.details else {
            panic!()
        };
        // equal splits ignore weights; shares are ordered by user
        let shares: Vec<_> = details.items[0]
            .shares
            .iter()
            .map(|s| (s.user_id, s.weight))
            .collect();
        assert_eq!(shares, [(1, 1), (2, 1)]);
        assert_eq!(draft.members, [1, 2]);
    }

    #[test]
    fn broken_expenses_are_refused() {
        let spoil = |f: fn(&mut ExpenseInput)| {
            let mut input = pizza();
            f(&mut input);
            code(expense(EntryKind::Expense, &input, today()))
        };
        assert_eq!(spoil(|i| i.payers.clear()), "payers_required");
        assert_eq!(spoil(|i| i.items.clear()), "items_required");
        assert_eq!(spoil(|i| i.title = "  ".into()), "title_required");
        assert_eq!(spoil(|i| i.currency = "XXX".into()), "unsupported_currency");
        assert_eq!(
            spoil(|i| i.occurred_on = NaiveDate::from_ymd_opt(2026, 3, 11)),
            "future_date"
        );
        assert_eq!(spoil(|i| i.payers[0].amount = 999), "payers_total");
        assert_eq!(spoil(|i| i.items[0].split = Split::Exact), "exact_total");
        assert_eq!(
            spoil(|i| {
                let first = i.items[0].shares[0].clone();
                i.items[0].shares.push(first);
            }),
            "person_twice"
        );
        assert_eq!(
            spoil(|i| {
                let first = i.payers[0].clone();
                i.payers.push(first);
            }),
            "payer_twice"
        );
        assert_eq!(
            spoil(|i| i.items[0].amount = MAX_AMOUNT + 1),
            "amount_too_large"
        );
    }

    #[test]
    fn income_words_speak_of_receivers() {
        let mut input = pizza();
        input.payers[0].amount = 1;
        let error = expense(EntryKind::Income, &input, today()).unwrap_err();
        assert!(error.message.starts_with("Otrzymujący"), "{error}");
    }

    fn owed(debtor: i64, creditor: Option<i64>, item: &str, amount: i64) -> DebtInput {
        DebtInput {
            debtor_id: debtor,
            creditor_id: creditor,
            title: String::new(),
            item: item.into(),
            amount,
            currency: "PLN".into(),
            occurred_on: None,
            note: String::new(),
        }
    }

    #[test]
    fn debts_name_themselves_and_are_checked() {
        let goods = debt(&owed(2, None, " kawa ", 2), 1, today())
            .unwrap()
            .content;
        assert_eq!(
            (goods.title.as_str(), goods.item.as_str()),
            ("kawa", "kawa")
        );
        assert_eq!((goods.amount, goods.currency.as_str()), (Some(2), ""));
        let cash = debt(&owed(1, Some(2), "", 500), 1, today())
            .unwrap()
            .content;
        assert_eq!(
            (cash.title.as_str(), cash.currency.as_str()),
            ("Dług", "PLN")
        );
        assert_eq!(code(debt(&owed(1, None, "", 5), 1, today())), "self_debt");
        assert_eq!(
            code(debt(&owed(2, Some(3), "", 5), 1, today())),
            "not_your_debt"
        );
        assert_eq!(
            code(debt(&owed(2, None, "kawa", 1001), 1, today())),
            "too_many_goods"
        );
        assert_eq!(
            code(debt(&owed(2, None, "", 0), 1, today())),
            "invalid_amount"
        );
    }

    #[test]
    fn a_mixed_settlement_has_no_headline_amount() {
        let content = debts(
            vec![
                Debt::money(2, 1, 1000),
                Debt {
                    debtor_id: 3,
                    creditor_id: 1,
                    amount: 1,
                    item: "kolacja".into(),
                },
            ],
            "Zakład",
            today(),
            "PLN",
        )
        .unwrap()
        .content;
        assert_eq!(
            (
                content.amount,
                content.currency.as_str(),
                content.item.as_str()
            ),
            (None, "", "")
        );
    }

    #[test]
    fn payments_are_titled_by_what_was_given() {
        let input = PaymentInput {
            to_user_id: 2,
            amount: 3,
            item: "piwo".into(),
            occurred_on: None,
            note: "dzięki".into(),
        };
        let content = payment(&input, 1, today()).unwrap().content;
        assert_eq!(
            (content.title.as_str(), content.currency.as_str()),
            ("Zwrot", "")
        );
        assert_eq!(code(payment(&input, 2, today())), "self_payment");
    }
}
