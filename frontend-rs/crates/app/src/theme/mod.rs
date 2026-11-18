//! Theming. A theme is styling only: one CSS file (`assets/themes/<id>.css`, scoped to
//! `[data-theme='<id>']`) that sets the token contract documented in `assets/themes/default.css`.
//! The theme on screen is the event override (a theme something forces for a while, e.g. a
//! holiday) or else the user's pick; it is set on `<html data-theme>`, and the browser chrome /
//! Android system bars follow the page background.
//!
//! Add a theme: create `assets/themes/<id>.css`, import it in `tailwind.css`, add it to `Theme`.

use dioxus::prelude::*;

use crate::platform::{self, storage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Default,
}

pub const DEFAULT_THEME: Theme = Theme::Default;

const THEME_KEY: &str = "theme";
const OVERRIDE_KEY: &str = "theme-override";

impl Theme {
    pub const ALL: [Self; 1] = [Self::Default];

    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "default",
        }
    }

    /// The picker's name for it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Domyślny",
        }
    }

    pub fn tone(self) -> Tone {
        match self {
            Self::Default => Tone::Dark,
        }
    }

    /// Whether users can pick it (themes for events only apply by themselves).
    pub fn is_selectable(self) -> bool {
        match self {
            Self::Default => true,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn selectable() -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(|t| t.is_selectable())
    }
}

/// The theme state, shared through context (`use_theme`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeState {
    selected: Signal<Theme>,
    event_override: Signal<Option<Theme>>,
}

impl ThemeState {
    /// The theme the user picked.
    pub fn selected(&self) -> Theme {
        *self.selected.read()
    }

    /// The theme on screen.
    pub fn effective(&self) -> Theme {
        self.event_override
            .read()
            .unwrap_or_else(|| self.selected())
    }

    /// Whether an override has taken over the picker.
    pub fn is_locked(&self) -> bool {
        self.effective() != self.selected()
    }

    pub fn select(&mut self, theme: Theme) {
        storage::set(THEME_KEY, Some(theme.id()));
        self.selected.set(theme);
    }

    /// Force a theme over the user's pick while something runs, or `None`. Remembered, so a
    /// reload paints it before the app knows whether it still applies.
    pub fn set_override(&mut self, theme: Option<Theme>) {
        storage::set(OVERRIDE_KEY, theme.map(Theme::id));
        if *self.event_override.peek() != theme {
            self.event_override.set(theme);
        }
    }
}

/// Create the theme state (in `App`) and keep `<html>` in step with it.
pub fn use_theme_provider() -> ThemeState {
    let state = use_context_provider(|| {
        let stored = |key| storage::get(key).as_deref().and_then(Theme::from_id);
        ThemeState {
            selected: Signal::new(
                stored(THEME_KEY)
                    .filter(|t| t.is_selectable())
                    .unwrap_or(DEFAULT_THEME),
            ),
            event_override: Signal::new(stored(OVERRIDE_KEY)),
        }
    });
    use_effect(move || {
        spawn(apply(state.effective()));
    });
    state
}

pub fn use_theme() -> ThemeState {
    use_context()
}

/// Set the theme on `<html>`, then match the browser chrome / system bars to the background.
async fn apply(theme: Theme) {
    let script = format!(
        r"
        document.documentElement.dataset.theme = '{id}';
        await new Promise(requestAnimationFrame);
        const background = getComputedStyle(document.body).backgroundColor;
        const meta = document.querySelector('meta[name=theme-color]');
        if (meta && background) meta.content = background;
        return background || '';
        ",
        id = theme.id(),
    );
    if let Ok(background) = document::eval(&script).join::<String>().await {
        platform::set_system_bar_color(&background);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_id(theme.id()), Some(theme));
        }
        assert_eq!(Theme::from_id("royal"), None); // the React app's themes are gone
    }

    #[test]
    fn the_default_is_selectable() {
        assert!(Theme::selectable().any(|t| t == DEFAULT_THEME));
    }
}
