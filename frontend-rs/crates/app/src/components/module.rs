//! The app's four modules and the parts every module screen shares: the coloured binder
//! (`ModuleSurface`) and the paper label with its title (`PaperLabel`). The home cards are built
//! from them now; the module list views will reuse them (a module's section is the same binder,
//! its header label the same paper).

use dioxus::prelude::*;

use crate::routes::Route;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Module {
    Sejmik,
    Kasa,
    Umowy,
    Mikolajki,
}

impl Module {
    /// Used in CSS (`data-module`) to pick the module's colour tokens.
    pub fn id(self) -> &'static str {
        match self {
            Self::Sejmik => "sejmik",
            Self::Kasa => "kasa",
            Self::Umowy => "umowy",
            Self::Mikolajki => "mikolajki",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Sejmik => "Sejmik",
            Self::Kasa => "Kasa Skarbowa",
            Self::Umowy => "Umowy",
            Self::Mikolajki => "Mikołajki",
        }
    }

    /// What the home card says the module is for.
    pub fn home_subtitle(self) -> &'static str {
        match self {
            Self::Sejmik => "głosowania i wnioski",
            Self::Kasa => "wydatki i rozliczenia",
            Self::Umowy => "zakłady, cele i słowo honoru",
            Self::Mikolajki => "losowanie i podarki",
        }
    }

    /// The subtitle in the module's own header (shorter, it shares the row with the bookmark).
    pub fn header_subtitle(self) -> &'static str {
        match self {
            Self::Sejmik => "głosowania i wnioski",
            Self::Kasa => "rozliczenia grupy",
            Self::Umowy => "zakłady i cele",
            Self::Mikolajki => "losowanie i podarki",
        }
    }

    pub fn route(self) -> Route {
        match self {
            Self::Sejmik => Route::Voting {},
            Self::Kasa => Route::Ledger {},
            Self::Umowy => Route::Pacts {},
            Self::Mikolajki => Route::SecretSanta {},
        }
    }
}

/// The kinds of paper labels are written on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    /// A manila folder (Sejmik).
    #[default]
    Manila,
    /// The ledger's blue-grey page (Kasa).
    Ledger,
    /// Contract paper (Umowy).
    Legal,
    /// A note pinned on cardboard (Mikołajki).
    Note,
}

impl Paper {
    fn id(self) -> &'static str {
        match self {
            Self::Manila => "manila",
            Self::Ledger => "ledger",
            Self::Legal => "legal",
            Self::Note => "note",
        }
    }
}

/// A module's coloured binder: dashed stitching, a hard shadow. With `to`, the whole binder is a
/// link (a home card); without, a plain section (a module screen's body).
#[component]
pub fn ModuleSurface(
    module: Module,
    to: Option<Route>,
    #[props(into, default)] class: String,
    children: Element,
) -> Element {
    let id = module.id();
    match to {
        Some(to) => rsx! {
            Link {
                to,
                class: "module-surface {class}",
                "data-module": id,
                "aria-label": module.title(),
                {children}
            }
        },
        None => rsx! {
            section {
                class: "module-surface {class}",
                "data-module": id,
                "aria-label": module.title(),
                {children}
            }
        },
    }
}

/// A piece of paper with a title, an italic subtitle and whatever follows (stamps, sums).
#[component]
pub fn PaperLabel(
    #[props(default)] paper: Paper,
    /// Degrees; paper is never quite straight.
    #[props(default)]
    tilt: f32,
    title: Element,
    #[props(into)] subtitle: String,
    #[props(into, default)] class: String,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "paper-label {class}",
            "data-paper": paper.id(),
            style: "--tilt: {tilt}deg",
            div { class: "paper-label-title", {title} }
            span { class: "paper-label-subtitle", "{subtitle}" }
            {children}
        }
    }
}
