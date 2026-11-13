//! What each kind of pact allows.

use std::collections::BTreeSet;

text_enum!(
    /// What sort of pact it is.
    PactKind {
        /// The host bets the condition happens; each opponent is a separate wager against them.
        Bet = "bet",
        /// Everyone picks a side; with stakes it is a shared pot, without a plain prediction.
        GroupBet = "group_bet",
        /// A personal resolution judged by every other member in a Sejmik vote.
        Resolution = "resolution",
        /// A goal a group commits to together, possibly years away.
        SharedGoal = "shared_goal",
    }
);

/// What a kind allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent flags, read individually
pub struct KindRules {
    /// Settling may move money (or goods).
    pub has_money: bool,
    /// Anyone may ask to join (the participants decide).
    pub joinable: bool,
    /// A Sejmik vote of every other member decides it; nobody is invited.
    pub judged_by_everyone: bool,
    pub needs_due_date: bool,
    /// Each opponent is a separate wager against the host, settled on its own.
    pub wager_based: bool,
    /// Starts only once every invitee has answered (each stake changes everyone's payout).
    pub all_must_answer: bool,
    /// Some claims may be made before the deadline (see [`crate::pacts::Outcome::early_ok`]).
    pub early_claims: bool,
}

impl PactKind {
    pub fn rules(self) -> KindRules {
        let base = KindRules {
            has_money: false,
            joinable: true,
            judged_by_everyone: false,
            needs_due_date: false,
            wager_based: false,
            all_must_answer: false,
            early_claims: false,
        };
        match self {
            Self::Bet => KindRules {
                has_money: true,
                wager_based: true,
                ..base
            },
            Self::GroupBet => KindRules {
                has_money: true,
                all_must_answer: true,
                ..base
            },
            Self::Resolution => KindRules {
                joinable: false,
                judged_by_everyone: true,
                needs_due_date: true,
                ..base
            },
            Self::SharedGoal => KindRules {
                needs_due_date: true,
                early_claims: true,
                ..base
            },
        }
    }

    /// Who must approve a join request: the host for a bet (only their stake is at risk),
    /// everyone already in it otherwise.
    pub fn join_approvers(self, host_id: i64, active_ids: &[i64]) -> BTreeSet<i64> {
        match self {
            Self::Bet | Self::Resolution => BTreeSet::from([host_id]),
            Self::GroupBet | Self::SharedGoal => active_ids.iter().copied().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_and_declare_their_rules() {
        for kind in PactKind::ALL {
            assert_eq!(PactKind::parse(kind.as_str()), Some(*kind));
        }
        assert_eq!(PactKind::parse("nope"), None);
        assert!(PactKind::Bet.rules().wager_based && PactKind::Bet.rules().joinable);
        assert!(!PactKind::Resolution.rules().joinable);
        assert!(PactKind::SharedGoal.rules().early_claims);
        assert!(PactKind::GroupBet.rules().all_must_answer);
    }

    #[test]
    fn group_joins_need_everyone_but_a_bet_only_the_host() {
        assert_eq!(
            PactKind::GroupBet.join_approvers(1, &[1, 2, 3]),
            BTreeSet::from([1, 2, 3])
        );
        assert_eq!(
            PactKind::Bet.join_approvers(1, &[1, 2, 3]),
            BTreeSet::from([1])
        );
    }
}
