//! The rubber stamp: counts and statuses on paper ("2 DO GŁOSU", "WŁĄCZONE").

use dioxus::prelude::*;

/// The stamp's ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StampTone {
    /// Red: something needs attention.
    #[default]
    Alert,
    /// Blue: nothing to do (a count of 0).
    Calm,
}

impl StampTone {
    /// Red for a count that needs attention, blue for 0.
    pub fn for_count(count: usize) -> Self {
        if count > 0 { Self::Alert } else { Self::Calm }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::Calm => "calm",
        }
    }
}

/// A rubber stamp: double border, typewriter capitals, slightly askew.
#[component]
pub fn Stamp(
    #[props(into)] text: String,
    #[props(default)] tilt: f32,
    #[props(default)] tone: StampTone,
    /// Smaller print, for tight papers.
    #[props(default)]
    small: bool,
) -> Element {
    rsx! {
        span {
            class: if small { "stamp stamp-small" } else { "stamp" },
            "data-tone": tone.id(),
            style: "--tilt: {tilt}deg",
            "{text}"
        }
    }
}
