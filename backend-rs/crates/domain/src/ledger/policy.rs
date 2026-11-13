//! Who may do what with an entry. The services enforce [`check`]; the API's `actions` block is
//! [`actions`], computed by the same function, so the two cannot disagree.

use serde::Serialize;

use super::entry::{Details, Entry, EntryKind, Source, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Accept a payment (its receiver).
    Confirm,
    /// Say a payment never arrived (its receiver).
    Reject,
    /// Delete an expense or a manual debt, take back a payment.
    Cancel,
    Edit,
    /// Add a picture.
    Attach,
}

/// Why an action is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// The entry's kind or state does not allow it (409).
    Conflict {
        code: &'static str,
        message: &'static str,
    },
    /// Not this person (403).
    Forbidden {
        code: &'static str,
        message: &'static str,
    },
}

const NOT_ANY_MORE: Denial = Denial::Conflict {
    code: "entry_locked",
    message: "Tego wpisu nie można już zmienić.",
};

/// What the caller can do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[allow(clippy::struct_excessive_bools)] // one flag per action
pub struct Actions {
    pub confirm: bool,
    pub reject: bool,
    pub cancel: bool,
    pub edit: bool,
    pub attach: bool,
}

pub fn actions(entry: &Entry, user_id: i64) -> Actions {
    let can = |action| check(action, entry, user_id).is_ok();
    Actions {
        confirm: can(Action::Confirm),
        reject: can(Action::Reject),
        cancel: can(Action::Cancel),
        edit: can(Action::Edit),
        attach: can(Action::Attach),
    }
}

/// Whether `user_id` may do `action` to `entry` now.
pub fn check(action: Action, entry: &Entry, user_id: i64) -> Result<(), Denial> {
    match &entry.content.details {
        Details::Expense(_) => {
            if !matches!(action, Action::Edit | Action::Cancel | Action::Attach) {
                return Err(NOT_ANY_MORE);
            }
            if entry.status != Status::Confirmed {
                let message = if entry.kind() == EntryKind::Income {
                    "Ten przychód został usunięty."
                } else {
                    "Ten wydatek został usunięty."
                };
                return Err(Denial::Conflict {
                    code: "entry_deleted",
                    message,
                });
            }
            Ok(())
        }
        Details::Debt(details) => {
            if action == Action::Attach {
                if !entry.involved().contains(&user_id) {
                    return Err(Denial::Forbidden {
                        code: "not_involved",
                        message: "Zdjęcie mogą dodać tylko osoby z tego wpisu.",
                    });
                }
                return Ok(());
            }
            if !matches!(action, Action::Cancel | Action::Edit) || entry.status != Status::Confirmed
            {
                return Err(NOT_ANY_MORE);
            }
            if entry.source != Source::Manual {
                return Err(Denial::Conflict {
                    code: "pact_debt",
                    message: "Rozliczenie umowy zmienia się tylko przez umowę.",
                });
            }
            if !details.debts.iter().any(|d| d.creditor_id == user_id) {
                return Err(Denial::Forbidden {
                    code: "not_creditor",
                    message: "Wpis może zmienić lub usunąć tylko osoba, której ktoś jest winien.",
                });
            }
            Ok(())
        }
        Details::Payment(payment) => {
            if action == Action::Attach {
                if user_id != payment.from && user_id != payment.to {
                    return Err(Denial::Forbidden {
                        code: "not_involved",
                        message: "Zdjęcie mogą dodać tylko płacący i odbiorca.",
                    });
                }
                return Ok(());
            }
            if !matches!(action, Action::Confirm | Action::Reject | Action::Cancel) {
                return Err(NOT_ANY_MORE);
            }
            if entry.status != Status::Pending {
                return Err(Denial::Conflict {
                    code: "payment_decided",
                    message: "Ta wpłata nie czeka już na decyzję.",
                });
            }
            if action == Action::Cancel && user_id != payment.from {
                return Err(Denial::Forbidden {
                    code: "not_payer",
                    message: "Wpłatę może wycofać tylko płacący.",
                });
            }
            if action != Action::Cancel && user_id != payment.to {
                return Err(Denial::Forbidden {
                    code: "not_receiver",
                    message: "Wpłatę potwierdza lub odrzuca jej odbiorca.",
                });
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::ledger::entry::{Content, DebtDetails, PaymentDetails};
    use crate::ledger::money::Debt;

    fn entry(details: Details, status: Status, source: Source) -> Entry {
        let kind = match details {
            Details::Debt(_) => EntryKind::Debt,
            Details::Payment(_) => EntryKind::Payment,
            Details::Expense(_) => EntryKind::Expense,
        };
        Entry {
            status,
            source,
            created_by: Some(1),
            rate: None,
            content: Content {
                kind,
                title: "x".into(),
                note: String::new(),
                category: None,
                occurred_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                amount: Some(100),
                currency: "PLN".into(),
                item: String::new(),
                details,
            },
        }
    }

    #[test]
    fn a_payment_is_decided_by_its_receiver_and_withdrawn_by_its_payer() {
        let pending = entry(
            Details::Payment(PaymentDetails { from: 2, to: 1 }),
            Status::Pending,
            Source::Manual,
        );
        let payer = actions(&pending, 2);
        assert_eq!(
            (
                payer.confirm,
                payer.reject,
                payer.cancel,
                payer.edit,
                payer.attach
            ),
            (false, false, true, false, true)
        );
        assert!(actions(&pending, 1).confirm);
        assert!(!actions(&pending, 3).attach);
        let done = Entry {
            status: Status::Confirmed,
            ..pending
        };
        assert_eq!(
            check(Action::Confirm, &done, 1).unwrap_err(),
            Denial::Conflict {
                code: "payment_decided",
                message: "Ta wpłata nie czeka już na decyzję."
            }
        );
    }

    #[test]
    fn a_pact_debt_only_takes_pictures() {
        let debts = Details::Debt(DebtDetails {
            debts: vec![Debt::money(2, 1, 100)],
        });
        let pact = entry(debts.clone(), Status::Confirmed, Source::Pact);
        let can = actions(&pact, 1);
        assert_eq!(
            (can.confirm, can.reject, can.cancel, can.edit, can.attach),
            (false, false, false, false, true)
        );
        let manual = entry(debts, Status::Confirmed, Source::Manual);
        assert!(actions(&manual, 1).cancel);
        assert!(matches!(
            check(Action::Cancel, &manual, 2),
            Err(Denial::Forbidden { .. })
        ));
    }
}
