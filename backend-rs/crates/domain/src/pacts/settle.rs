//! What follows from a confirmed outcome: who won, and what is owed.

use std::collections::BTreeMap;

use super::ITEM_LENGTH;
use super::kind::PactKind;
use super::outcome::{DRAW, HOST, Outcome};
use crate::ledger::money::{Debt, allocate};

text_enum!(
    /// One person's result in a settled claim.
    Verdict {
        Won = "won",
        Lost = "lost",
        Draw = "draw",
        /// Called off: counts for nobody.
        Void = "void",
    }
);

/// A participant as settling sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    pub user_id: i64,
    pub is_host: bool,
    pub side: String,
    /// Grosze.
    pub stake_amount: Option<i64>,
    /// A stake in kind (used when there is no amount).
    pub stake_note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settlement {
    /// Money in grosze, or one piece of an item (a stake in kind).
    pub debts: Vec<Debt>,
    /// People with no verdict are left out.
    pub verdicts: BTreeMap<i64, Verdict>,
}

/// Settle `outcome` among `parties` (a bet: the host and the one opponent of the wager; other
/// kinds: everyone active).
pub fn settle(kind: PactKind, parties: &[Party], outcome: &Outcome) -> Settlement {
    match (kind, outcome) {
        (_, Outcome::Void { .. }) => Settlement {
            debts: Vec::new(),
            verdicts: parties.iter().map(|p| (p.user_id, Verdict::Void)).collect(),
        },
        (PactKind::Bet, Outcome::Winner { winner }) => bet(parties, winner),
        (PactKind::GroupBet, Outcome::Winner { winner }) => group_bet(parties, winner),
        (PactKind::Resolution, Outcome::Kept { kept }) => Settlement {
            debts: Vec::new(),
            verdicts: parties
                .iter()
                .filter(|p| p.is_host)
                .map(|p| (p.user_id, if *kept { Verdict::Won } else { Verdict::Lost }))
                .collect(),
        },
        (PactKind::SharedGoal, Outcome::Done { done }) => {
            let verdict = if *done { Verdict::Won } else { Verdict::Lost };
            Settlement {
                debts: Vec::new(),
                verdicts: parties.iter().map(|p| (p.user_id, verdict)).collect(),
            }
        }
        // A claim is checked against its kind before it is stored.
        _ => Settlement::default(),
    }
}

/// An owed item: one piece of the stake in kind, as the ledger records goods.
fn item_debt(debtor: i64, creditor: i64, note: &str) -> Debt {
    Debt {
        debtor_id: debtor,
        creditor_id: creditor,
        amount: 1,
        item: note.trim().chars().take(ITEM_LENGTH).collect(),
    }
}

/// The loser of a wager owes the winner the opponent's stake (the host matches it).
fn bet(parties: &[Party], winner: &str) -> Settlement {
    let (Some(host), Some(opponent)) = (
        parties.iter().find(|p| p.is_host),
        parties.iter().find(|p| !p.is_host),
    ) else {
        return Settlement::default();
    };
    if winner == DRAW {
        return Settlement {
            debts: Vec::new(),
            verdicts: BTreeMap::from([
                (host.user_id, Verdict::Draw),
                (opponent.user_id, Verdict::Draw),
            ]),
        };
    }
    let (won, lost) = if winner == HOST {
        (host, opponent)
    } else {
        (opponent, host)
    };
    let mut debts = Vec::new();
    if let Some(stake) = opponent.stake_amount.filter(|s| *s > 0) {
        debts.push(Debt::money(lost.user_id, won.user_id, stake));
    } else if !opponent.stake_note.trim().is_empty() {
        debts.push(item_debt(lost.user_id, won.user_id, &opponent.stake_note));
    }
    Settlement {
        debts,
        verdicts: BTreeMap::from([(won.user_id, Verdict::Won), (lost.user_id, Verdict::Lost)]),
    }
}

/// The winning side splits the losing side's stakes in proportion to their own (largest
/// remainder), so nobody loses more than they put in. A stake in kind goes to one person on the
/// winning side, taking turns among them.
fn group_bet(parties: &[Party], winner: &str) -> Settlement {
    if winner == DRAW {
        return Settlement {
            debts: Vec::new(),
            verdicts: parties.iter().map(|p| (p.user_id, Verdict::Draw)).collect(),
        };
    }
    let right = |p: &&Party| p.side == winner;
    let staked = |p: &&Party| p.stake_amount.is_some_and(|s| s > 0);
    let verdicts = parties
        .iter()
        .map(|p| {
            let verdict = if p.side == winner {
                Verdict::Won
            } else {
                Verdict::Lost
            };
            (p.user_id, verdict)
        })
        .collect();
    let winners: Vec<&Party> = parties.iter().filter(right).filter(staked).collect();
    let losers: Vec<&Party> = parties
        .iter()
        .filter(|p| !right(p))
        .filter(staked)
        .collect();
    let mut debts = Vec::new();
    if !winners.is_empty() {
        let weights: Vec<i64> = winners
            .iter()
            .map(|w| w.stake_amount.unwrap_or(0))
            .collect();
        for loser in &losers {
            let shares = allocate(loser.stake_amount.unwrap_or(0), &weights);
            for (taker, amount) in winners.iter().zip(shares) {
                if amount > 0 {
                    debts.push(Debt::money(loser.user_id, taker.user_id, amount));
                }
            }
        }
    }
    let in_kind = parties
        .iter()
        .filter(|p| !right(p) && !staked(p) && !p.stake_note.trim().is_empty());
    let everyone_right: Vec<&Party> = parties.iter().filter(right).collect();
    if !everyone_right.is_empty() {
        for (turn, loser) in in_kind.enumerate() {
            let taker = everyone_right[turn % everyone_right.len()];
            debts.push(item_debt(loser.user_id, taker.user_id, &loser.stake_note));
        }
    }
    Settlement { debts, verdicts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::money::Debt;

    const A: i64 = 1;
    const B: i64 = 2;
    const C: i64 = 3;

    fn party(user_id: i64, side: &str, stake: Option<i64>, host: bool) -> Party {
        Party {
            user_id,
            is_host: host,
            side: side.into(),
            stake_amount: stake,
            stake_note: String::new(),
        }
    }

    fn winner(side: &str) -> Outcome {
        Outcome::Winner {
            winner: side.into(),
        }
    }

    fn owed(settlement: &Settlement) -> Vec<(i64, i64, i64)> {
        let mut owed: Vec<_> = settlement
            .debts
            .iter()
            .map(|d| (d.debtor_id, d.creditor_id, d.amount))
            .collect();
        owed.sort_unstable();
        owed
    }

    fn group() -> Vec<Party> {
        vec![
            party(A, "tak", Some(2000), true),
            party(B, "nie", Some(3000), false),
            party(C, "nie", Some(5000), false),
        ]
    }

    #[test]
    fn a_bet_pairs_the_host_with_one_opponent() {
        let parties = [party(A, "", None, true), party(B, "", Some(1500), false)];
        let host = settle(PactKind::Bet, &parties, &winner("host"));
        assert_eq!(owed(&host), [(B, A, 1500)]);
        assert_eq!(
            host.verdicts,
            BTreeMap::from([(A, Verdict::Won), (B, Verdict::Lost)])
        );
        assert_eq!(
            owed(&settle(PactKind::Bet, &parties, &winner("opponent"))),
            [(A, B, 1500)]
        );
        let draw = settle(PactKind::Bet, &parties, &winner("draw"));
        assert_eq!(draw.debts, Vec::<Debt>::new());
        assert_eq!(draw.verdicts[&A], Verdict::Draw);
    }

    #[test]
    fn the_documented_group_bet_example() {
        let yes = settle(PactKind::GroupBet, &group(), &winner("tak"));
        assert_eq!(owed(&yes), [(B, A, 3000), (C, A, 5000)]);
        let no = settle(PactKind::GroupBet, &group(), &winner("nie"));
        assert_eq!(owed(&no), [(A, B, 750), (A, C, 1250)]);
        assert_eq!(
            no.verdicts,
            BTreeMap::from([(A, Verdict::Lost), (B, Verdict::Won), (C, Verdict::Won)])
        );
    }

    #[test]
    fn an_empty_side_a_draw_or_no_cash_settle_no_money() {
        let only_winners = [
            party(A, "tak", Some(100), true),
            party(B, "tak", Some(200), false),
        ];
        assert_eq!(
            settle(PactKind::GroupBet, &only_winners, &winner("tak")).debts,
            Vec::<Debt>::new()
        );
        assert_eq!(
            settle(PactKind::GroupBet, &group(), &winner("draw")).debts,
            Vec::<Debt>::new()
        );
        let no_cash = [
            party(A, "tak", None, true),
            party(B, "nie", Some(500), false),
        ];
        assert_eq!(
            settle(PactKind::GroupBet, &no_cash, &winner("tak")).debts,
            Vec::<Debt>::new()
        );
    }

    #[test]
    fn stakes_in_kind_are_spread_over_the_winning_side() {
        let mut parties = vec![party(A, "tak", None, true), party(B, "tak", None, false)];
        for (id, note) in [(4, "piwo"), (5, "kolacja"), (6, "kawa")] {
            parties.push(Party {
                stake_note: note.into(),
                ..party(id, "nie", None, false)
            });
        }
        let debts = settle(PactKind::GroupBet, &parties, &winner("tak")).debts;
        let got: Vec<_> = debts
            .iter()
            .map(|d| (d.debtor_id, d.creditor_id, d.amount, d.item.as_str()))
            .collect();
        assert_eq!(
            got,
            [(4, A, 1, "piwo"), (5, B, 1, "kolacja"), (6, A, 1, "kawa")]
        );
    }

    #[test]
    fn calling_it_off_gives_everyone_void_and_owes_nothing() {
        let settled = settle(PactKind::GroupBet, &group(), &Outcome::VOID);
        assert_eq!(settled.debts, Vec::<Debt>::new());
        assert!(settled.verdicts.values().all(|v| *v == Verdict::Void));
        assert_eq!(settled.verdicts.len(), 3);
    }

    #[test]
    fn a_resolution_judges_its_author_and_a_goal_everyone() {
        let parties = [party(A, "", None, true), party(B, "", None, false)];
        let kept = settle(
            PactKind::Resolution,
            &parties,
            &Outcome::Kept { kept: true },
        );
        assert_eq!(kept.verdicts, BTreeMap::from([(A, Verdict::Won)]));
        let broken = settle(
            PactKind::Resolution,
            &parties,
            &Outcome::Kept { kept: false },
        );
        assert_eq!(broken.verdicts, BTreeMap::from([(A, Verdict::Lost)]));
        let done = settle(
            PactKind::SharedGoal,
            &parties,
            &Outcome::Done { done: true },
        );
        assert_eq!(
            done.verdicts,
            BTreeMap::from([(A, Verdict::Won), (B, Verdict::Won)])
        );
    }
}
