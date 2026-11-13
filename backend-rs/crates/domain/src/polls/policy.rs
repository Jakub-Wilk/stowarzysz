//! Who may do what to a poll. The services enforce these checks and the API's `actions` block
//! is computed from the same function, so the two cannot disagree.

use super::PollKind;

/// Something one can do to a poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Vote,
    Veto,
    Abstain,
    Close,
    TieBreak,
}

/// Why an action is refused; each maps to its own HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// The poll is over (409).
    Closed,
    /// Not this person's to do (403).
    Forbidden {
        code: &'static str,
        message: &'static str,
    },
    /// Not possible for this poll (400).
    Invalid {
        code: &'static str,
        message: &'static str,
    },
}

/// What the policy needs to know about a poll and the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent facts
pub struct PollFacts {
    pub kind: PollKind,
    pub open: bool,
    pub is_creator: bool,
    /// The caller takes part.
    pub participating: bool,
    pub vetoed: bool,
    /// The finished poll tied (option kinds only).
    pub tied: bool,
    pub has_tie_break: bool,
}

const NOT_PARTICIPANT: Denial = Denial::Forbidden {
    code: "not_participant",
    message: "Nie bierzesz udziału w tym głosowaniu.",
};
const ALREADY_VETOED: Denial = Denial::Invalid {
    code: "already_vetoed",
    message: "Zawetowano już to głosowanie; weta nie można cofnąć.",
};

/// Whether the caller may do `action`, in the order the checks are reported.
pub fn check(action: Action, facts: &PollFacts) -> Result<(), Denial> {
    let rules = facts.kind.rules();
    match action {
        Action::Vote => {
            require_open(facts)?;
            require(facts.participating, NOT_PARTICIPANT)?;
            require(!facts.vetoed, ALREADY_VETOED)
        }
        Action::Veto => {
            require_open(facts)?;
            require(
                rules.allows_veto,
                Denial::Invalid {
                    code: "veto_not_allowed",
                    message: "To głosowanie nie podlega wetu.",
                },
            )?;
            require(facts.participating, NOT_PARTICIPANT)
        }
        Action::Abstain => {
            require_open(facts)?;
            require(
                rules.allows_abstain,
                Denial::Invalid {
                    code: "abstain_not_allowed",
                    message: "W tym głosowaniu nie można się wstrzymać.",
                },
            )?;
            require(facts.participating, NOT_PARTICIPANT)?;
            require(!facts.vetoed, ALREADY_VETOED)
        }
        Action::Close => {
            require_open(facts)?;
            require(
                facts.is_creator,
                Denial::Forbidden {
                    code: "not_creator",
                    message: "Tylko osoba, która rozpoczęła głosowanie, może je zakończyć.",
                },
            )?;
            require(
                rules.allows_early_close,
                Denial::Invalid {
                    code: "early_close_not_allowed",
                    message: "Tego głosowania nie można zakończyć wcześniej.",
                },
            )
        }
        Action::TieBreak => {
            require(
                facts.is_creator,
                Denial::Forbidden {
                    code: "not_creator",
                    message: "Tylko osoba, która rozpoczęła głosowanie, może rozpisać dogrywkę.",
                },
            )?;
            require(
                !facts.open,
                Denial::Invalid {
                    code: "poll_open",
                    message: "Dogrywkę można rozpisać dopiero po zakończeniu głosowania.",
                },
            )?;
            require(
                facts.tied,
                Denial::Invalid {
                    code: "not_tied",
                    message: "To głosowanie nie zakończyło się remisem.",
                },
            )
        }
    }
}

/// Whether the caller can do `action` now (for the `actions` block). A tie-break that already
/// exists is not offered again (asking again would only return it).
pub fn can(action: Action, facts: &PollFacts) -> bool {
    check(action, facts).is_ok() && !(action == Action::TieBreak && facts.has_tie_break)
}

fn require_open(facts: &PollFacts) -> Result<(), Denial> {
    require(facts.open, Denial::Closed)
}

fn require(condition: bool, denial: Denial) -> Result<(), Denial> {
    if condition { Ok(()) } else { Err(denial) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(kind: PollKind) -> PollFacts {
        PollFacts {
            kind,
            open: true,
            is_creator: false,
            participating: true,
            vetoed: false,
            tied: false,
            has_tie_break: false,
        }
    }

    #[test]
    fn voting_needs_an_open_poll_a_participant_and_no_veto() {
        assert!(can(Action::Vote, &facts(PollKind::Score)));
        assert_eq!(
            check(
                Action::Vote,
                &PollFacts {
                    open: false,
                    ..facts(PollKind::Score)
                }
            ),
            Err(Denial::Closed)
        );
        assert_eq!(
            check(
                Action::Vote,
                &PollFacts {
                    participating: false,
                    ..facts(PollKind::Score)
                }
            ),
            Err(NOT_PARTICIPANT)
        );
        assert_eq!(
            check(
                Action::Vote,
                &PollFacts {
                    vetoed: true,
                    ..facts(PollKind::Score)
                }
            ),
            Err(ALREADY_VETOED)
        );
    }

    #[test]
    fn veto_and_abstain_follow_the_kind() {
        assert!(can(Action::Veto, &facts(PollKind::Score)));
        assert!(!can(Action::Veto, &facts(PollKind::SingleChoice)));
        assert!(can(Action::Abstain, &facts(PollKind::SingleChoice)));
        assert!(!can(Action::Abstain, &facts(PollKind::Nickname)));
        assert!(!can(
            Action::Abstain,
            &PollFacts {
                vetoed: true,
                ..facts(PollKind::Score)
            }
        ));
    }

    #[test]
    fn only_creators_close_early_and_not_profile_votes() {
        let mine = PollFacts {
            is_creator: true,
            ..facts(PollKind::Score)
        };
        assert!(can(Action::Close, &mine));
        assert!(!can(Action::Close, &facts(PollKind::Score)));
        assert!(matches!(
            check(
                Action::Close,
                &PollFacts {
                    kind: PollKind::Avatar,
                    ..mine
                }
            ),
            Err(Denial::Invalid { .. })
        ));
    }

    #[test]
    fn tie_breaks_for_creators_of_finished_tied_polls_once() {
        let tied = PollFacts {
            open: false,
            is_creator: true,
            tied: true,
            participating: false,
            ..facts(PollKind::SingleChoice)
        };
        assert!(can(Action::TieBreak, &tied));
        assert!(
            check(
                Action::TieBreak,
                &PollFacts {
                    has_tie_break: true,
                    ..tied.clone()
                }
            )
            .is_ok(),
            "asking again is allowed"
        );
        assert!(
            !can(
                Action::TieBreak,
                &PollFacts {
                    has_tie_break: true,
                    ..tied.clone()
                }
            ),
            "but not offered"
        );
        assert!(!can(
            Action::TieBreak,
            &PollFacts {
                tied: false,
                ..tied.clone()
            }
        ));
        assert!(!can(Action::TieBreak, &PollFacts { open: true, ..tied }));
    }
}
