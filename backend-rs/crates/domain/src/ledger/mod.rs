//! Kasa Skarbowa: a Tricount for the whole group, in three layers. An *entry* is what happened;
//! its kind turns it into *obligations* (who owes whom, [`money::Debt`]); *balances* sum the
//! obligations of confirmed entries. Everything here is pure; the money maths is checked against
//! the Python implementation.

pub mod announce;
pub mod balances;
mod entry;
pub mod input;
pub mod money;
pub mod policy;
pub mod stats;

pub use entry::{
    Category, Content, DebtDetails, Details, Entry, EntryKind, ExpenseDetails, Item, Payer,
    PaymentDetails, Share, Source, Split, Status,
};
