//! Sejmik: votes. Everything kind-specific lives here as closed enums: adding a kind means
//! adding a variant, and the compiler points at every `match` that has to handle it.

pub mod ballot;
pub mod config;
pub mod kind;
pub mod policy;
pub mod result;
pub mod tally;

pub use ballot::{Ballot, BallotInput, Resolved};
pub use config::{OptionsConfig, PollConfig, PollOption};
pub use kind::{KindRules, PollKind};
pub use policy::{Action, Denial, PollFacts};

use chrono::TimeDelta;

/// Every poll ends this long after it was created.
pub const DURATION: TimeDelta = TimeDelta::hours(72);
/// Reminders to those who have not answered go out at these ages.
pub const REMINDERS: [TimeDelta; 3] = [
    TimeDelta::hours(24),
    TimeDelta::hours(48),
    TimeDelta::hours(69),
];
/// The reactions a finished vote takes.
pub const REACTIONS: [&str; 5] = ["❤️", "🔥", "😭", "👎", "🤣"];
pub const TITLE_MAX: usize = 200;
pub const REASON_MAX: usize = 300;

/// How a poll ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// Everyone answered.
    Auto,
    /// Ended early by its creator.
    Creator,
    /// Ran out of time.
    Expired,
}

impl CloseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Creator => "creator",
            Self::Expired => "expired",
        }
    }

    /// The reason recorded for whoever never answered.
    pub fn auto_abstain_reason(self) -> &'static str {
        match self {
            Self::Auto => "", // everyone had answered: nobody is left
            Self::Creator => "Brak głosu przed zakończeniem głosowania.",
            Self::Expired => "Brak głosu w terminie.",
        }
    }
}

/// How many reminders are due for a poll of `age` (0 to 3). After downtime only the latest one
/// is sent.
pub fn due_reminder_stage(age: TimeDelta) -> usize {
    REMINDERS.iter().filter(|offset| age >= **offset).count()
}

/// Hours left when the reminder of `stage` (1-based) goes out.
pub fn hours_left_at(stage: usize) -> i64 {
    (DURATION - REMINDERS[stage - 1]).num_hours()
}

/// The push text when a poll ends: the title, plus what came of a change it was about.
pub fn closing_summary(title: &str, result: &serde_json::Value) -> String {
    let Some(applied) = result.get("applied").and_then(serde_json::Value::as_bool) else {
        return title.to_owned();
    };
    if applied {
        format!("{title}: zmiana zastosowana")
    } else if result.get("approved").and_then(serde_json::Value::as_bool) == Some(true) {
        format!("{title}: przyjęto, ale nie udało się zastosować zmiany")
    } else {
        format!("{title}: nie przyjęto")
    }
}

/// The kind of the follow-up vote between the options a poll tied on (a tied multi-select poll
/// asks to pick one); `None` for kinds without tie-breaks.
pub fn tie_break_kind(kind: PollKind) -> Option<PollKind> {
    match kind {
        PollKind::SingleChoice | PollKind::MultiChoice => Some(PollKind::SingleChoice),
        PollKind::RankedChoice => Some(PollKind::RankedChoice),
        _ => None,
    }
}

/// The option ids a finished poll tied on (empty unless several won).
pub fn tied_winners(kind: PollKind, result: Option<&serde_json::Value>) -> Vec<String> {
    let winners: Vec<String> = result
        .and_then(|r| r.get("winners"))
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if tie_break_kind(kind).is_some() && winners.len() > 1 {
        winners
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reminders_are_due_by_age() {
        assert_eq!(due_reminder_stage(TimeDelta::hours(23)), 0);
        assert_eq!(due_reminder_stage(TimeDelta::hours(24)), 1);
        assert_eq!(due_reminder_stage(TimeDelta::hours(70)), 3);
        assert_eq!(
            [hours_left_at(1), hours_left_at(2), hours_left_at(3)],
            [48, 24, 3]
        );
    }

    #[test]
    fn closing_summaries() {
        assert_eq!(closing_summary("Obiad", &json!({"score": 1.0})), "Obiad");
        assert_eq!(
            closing_summary("Nick", &json!({"applied": true})),
            "Nick: zmiana zastosowana"
        );
        assert_eq!(
            closing_summary("Nick", &json!({"applied": false, "approved": true})),
            "Nick: przyjęto, ale nie udało się zastosować zmiany"
        );
        assert_eq!(
            closing_summary("Nick", &json!({"applied": false, "approved": false})),
            "Nick: nie przyjęto"
        );
    }

    #[test]
    fn ties_only_count_for_option_kinds() {
        let tied = json!({"winners": ["o1", "o2"]});
        assert_eq!(
            tied_winners(PollKind::MultiChoice, Some(&tied)),
            ["o1", "o2"]
        );
        assert_eq!(
            tied_winners(PollKind::Score, Some(&tied)),
            Vec::<String>::new()
        );
        assert_eq!(
            tied_winners(PollKind::SingleChoice, Some(&json!({"winners": ["o1"]}))),
            Vec::<String>::new()
        );
        assert_eq!(
            tie_break_kind(PollKind::MultiChoice),
            Some(PollKind::SingleChoice)
        );
    }
}
