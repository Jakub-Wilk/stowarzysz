/// The kinds of vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PollKind {
    /// A score from -5 to +5; the result is the average.
    Score,
    /// Renaming another member.
    Nickname,
    /// Changing or removing another member's picture.
    Avatar,
    /// Members outside a pact decide a disputed claim about it.
    PactRuling,
    /// Members judge whether a resolution was kept.
    ResolutionJudgment,
    SingleChoice,
    MultiChoice,
    /// Ranked, counted with the Schulze method.
    RankedChoice,
}

/// What a kind allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent flags, read individually
pub struct KindRules {
    pub allows_veto: bool,
    /// Abstaining is possible, and whoever never answered counts as abstaining at the end.
    pub allows_abstain: bool,
    /// The creator may end it before everyone answered.
    pub allows_early_close: bool,
    /// Every member votes (the creator does not choose participants).
    pub everyone_participates: bool,
    /// Members can call it through the API (the pact kinds are called by pacts only).
    pub user_creatable: bool,
}

impl PollKind {
    pub const ALL: [Self; 8] = [
        Self::Score,
        Self::Nickname,
        Self::Avatar,
        Self::PactRuling,
        Self::ResolutionJudgment,
        Self::SingleChoice,
        Self::MultiChoice,
        Self::RankedChoice,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Score => "score",
            Self::Nickname => "nickname",
            Self::Avatar => "avatar",
            Self::PactRuling => "pact_ruling",
            Self::ResolutionJudgment => "resolution_judgment",
            Self::SingleChoice => "single_choice",
            Self::MultiChoice => "multi_choice",
            Self::RankedChoice => "ranked_choice",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    pub fn rules(self) -> KindRules {
        let base = KindRules {
            allows_veto: true,
            allows_abstain: true,
            allows_early_close: true,
            everyone_participates: false,
            user_creatable: true,
        };
        match self {
            Self::Score => base,
            Self::Nickname | Self::Avatar => KindRules {
                allows_veto: false,
                allows_abstain: false,
                allows_early_close: false,
                everyone_participates: true,
                ..base
            },
            Self::PactRuling | Self::ResolutionJudgment => KindRules {
                allows_veto: false,
                allows_early_close: false,
                user_creatable: false,
                ..base
            },
            Self::SingleChoice | Self::MultiChoice | Self::RankedChoice => KindRules {
                allows_veto: false,
                ..base
            },
        }
    }

    pub fn is_options(self) -> bool {
        matches!(
            self,
            Self::SingleChoice | Self::MultiChoice | Self::RankedChoice
        )
    }

    /// Whether the vote changes a member's profile when it passes.
    pub fn is_profile_change(self) -> bool {
        matches!(self, Self::Nickname | Self::Avatar)
    }
}

impl std::fmt::Display for PollKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_its_name() {
        for kind in PollKind::ALL {
            assert_eq!(PollKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(PollKind::parse("yes_no"), None);
    }

    #[test]
    fn rules_match_the_kinds() {
        assert!(PollKind::Score.rules().allows_veto);
        let profile = PollKind::Avatar.rules();
        assert!(!profile.allows_veto && !profile.allows_abstain && !profile.allows_early_close);
        assert!(profile.everyone_participates);
        assert!(!PollKind::PactRuling.rules().user_creatable);
        assert!(PollKind::PactRuling.rules().allows_abstain);
        assert!(!PollKind::RankedChoice.rules().allows_veto);
    }
}
