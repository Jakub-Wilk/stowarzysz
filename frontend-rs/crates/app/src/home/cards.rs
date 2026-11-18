//! The home screen's module cards: each is the module's binder (`ModuleSurface`) with a paper
//! label (`PaperLabel`), its own decoration and its counters. A counter is `None` while it
//! loads or if it failed: the card then simply shows no stamp.

use dioxus::prelude::*;

use super::garland::Garland;
use super::summary;
use crate::components::{Module, ModuleSurface, Paper, PaperLabel, Stamp, StampTone};
use crate::polish::{dates, money, plural};

const SANTA_HAT: Asset = asset!("/assets/decor/santa-hat.svg");
const ENVELOPE: Asset = asset!("/assets/decor/envelope.svg");

/// Sejmik: a manila folder on the green binder, a stack of ballots beside it.
#[component]
pub fn SejmikCard(to_answer: Option<usize>) -> Element {
    let module = Module::Sejmik;
    rsx! {
        ModuleSurface { module, to: module.route(), class: "card card-sejmik",
            div { class: "card-sejmik-row",
                PaperLabel {
                    paper: Paper::Manila,
                    tilt: -1.0,
                    class: "card-sejmik-folder",
                    title: rsx! { "{module.title()}" },
                    subtitle: module.home_subtitle(),
                    FolderTabs {}
                    div { class: "card-stamps",
                        if let Some(n) = to_answer {
                            Stamp { text: format!("{n} DO GŁOSU"), tone: StampTone::for_count(n) }
                        }
                    }
                }
                div { class: "card-sejmik-stack", "aria-hidden": "true",
                    div { class: "sheet sheet-back" }
                    div { class: "sheet sheet-front",
                        div { class: "sheet-rule" }
                        div { class: "sheet-rule" }
                        div { class: "sheet-rule" }
                        div { class: "sheet-box" }
                    }
                }
            }
        }
    }
}

/// The folder's tabs and the sheet sticking out of it.
#[component]
fn FolderTabs() -> Element {
    rsx! {
        span { class: "folder-tab folder-tab-1", "aria-hidden": "true" }
        span { class: "folder-tab folder-tab-2", "aria-hidden": "true" }
        span { class: "folder-sheet", "aria-hidden": "true",
            span { class: "folder-sheet-rule" }
        }
    }
}

/// Kasa Skarbowa: a ledger page on the blue binder, ribbon bookmarks, the balance at the foot.
#[component]
pub fn KasaCard(ledger: Option<summary::Ledger>) -> Element {
    let module = Module::Kasa;
    let to_confirm = ledger.map(|l| l.to_confirm);
    rsx! {
        ModuleSurface { module, to: module.route(), class: "card card-kasa",
            div { class: "card-kasa-ribbons", "aria-hidden": "true",
                span { class: "ribbon ribbon-1" }
                span { class: "ribbon ribbon-2" }
                span { class: "ribbon ribbon-3" }
            }
            div { class: "card-kasa-binding", "aria-hidden": "true" }
            PaperLabel {
                paper: Paper::Ledger,
                tilt: 0.8,
                class: "card-kasa-page",
                title: rsx! { "{module.title()}" },
                subtitle: module.home_subtitle(),
                if let Some(n) = to_confirm {
                    Stamp {
                        text: format!("{n} DO POTWIERDZENIA"),
                        tilt: -2.0,
                        tone: StampTone::for_count(n),
                        small: true,
                    }
                }
                if let Some(ledger) = ledger {
                    div { class: "card-kasa-balance",
                        span { class: "card-kasa-balance-label", "saldo" }
                        span {
                            class: "card-kasa-balance-amount",
                            "data-sign": if ledger.balance < 0 { "negative" } else { "positive" },
                            "{money::format_signed_pln(ledger.balance)}"
                        }
                    }
                }
            }
        }
    }
}

/// Umowy: contract paper on the burgundy binder, sealed with wax in the corner.
#[component]
pub fn UmowyCard(overdue: Option<usize>) -> Element {
    let module = Module::Umowy;
    rsx! {
        ModuleSurface { module, to: module.route(), class: "card card-umowy",
            PaperLabel {
                paper: Paper::Legal,
                tilt: -0.8,
                class: "card-umowy-page",
                title: rsx! {
                    span { class: "card-umowy-title", "UMOWY" }
                    Ornament {}
                },
                subtitle: module.home_subtitle(),
                if let Some(n) = overdue {
                    Stamp {
                        text: format!("{n} PO TERMINIE"),
                        tilt: -3.0,
                        tone: StampTone::for_count(n),
                        small: true,
                    }
                }
            }
            div { class: "card-umowy-corner", "aria-hidden": "true",
                WaxSeal { letter: "S" }
            }
        }
    }
}

/// The rule under a contract's heading: two brass lines and a diamond.
#[component]
fn Ornament() -> Element {
    rsx! {
        svg {
            class: "ornament",
            width: "84",
            height: "9",
            view_box: "0 0 140 14",
            "aria-hidden": "true",
            path { class: "ornament-line", d: "M2 7h50M88 7h50" }
            path { class: "ornament-diamond", d: "M70 1l5 6-5 6-5-6z" }
        }
    }
}

/// A wax seal with a letter pressed into it.
#[component]
pub fn WaxSeal(#[props(into)] letter: String, #[props(default = 46)] size: u32) -> Element {
    rsx! {
        svg {
            class: "wax-seal",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 104 104",
            "aria-hidden": "true",
            circle { class: "wax-seal-body", cx: "52", cy: "52", r: "46" }
            circle { class: "wax-seal-rim", cx: "52", cy: "52", r: "46" }
            circle { class: "wax-seal-ring", cx: "52", cy: "52", r: "33" }
            text {
                class: "wax-seal-letter",
                x: "52",
                y: "66",
                "text-anchor": "middle",
                "{letter}"
            }
        }
    }
}

/// Mikołajki (only while an event runs): a note on a cardboard box, a garland, a Santa hat.
#[component]
pub fn MikolajkiCard(santa: summary::Santa) -> Element {
    let module = Module::Mikolajki;
    let deadline = dates::day_month(santa.deadline);
    let requests = santa.help_requests;
    rsx! {
        ModuleSurface { module, to: module.route(), class: "card card-mikolajki",
            div { class: "card-mikolajki-row",
                img { class: "card-mikolajki-hat", src: SANTA_HAT, alt: "", "aria-hidden": "true" }
                img { class: "card-mikolajki-envelope", src: ENVELOPE, alt: "", "aria-hidden": "true" }
                PaperLabel {
                    paper: Paper::Note,
                    tilt: 0.6,
                    class: "card-mikolajki-note",
                    title: rsx! { "{module.title()}" },
                    subtitle: format!("termin: {deadline}"),
                    Stamp {
                        text: format!(
                            "{requests} {} O POMYSŁY",
                            plural::plural(i64::try_from(requests).unwrap_or(i64::MAX), plural::REQUEST)
                                .to_uppercase(),
                        ),
                        tilt: -2.0,
                        tone: StampTone::for_count(requests),
                    }
                }
            }
            Garland {}
        }
    }
}
