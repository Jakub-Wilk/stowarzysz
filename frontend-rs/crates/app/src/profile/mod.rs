//! The profile menu: the bookmark unrolls into a burgundy panel with the member's card
//! (legitymacja) and what they can do about their account. For now only WYLOGUJ works; the
//! other rows are drawn but do nothing yet.

mod legitymacja;

use api::types::Me;
use dioxus::prelude::*;
use legitymacja::Legitymacja;

use crate::components::{FoilCross, FoilText, Stamp, Swipe, focus_without_scrolling, use_swipes};
use crate::session::use_session;

#[component]
pub fn ProfileOverlay(
    me: ReadSignal<Me>,
    open: ReadSignal<bool>,
    on_close: EventHandler<()>,
) -> Element {
    let session = use_session();
    // Opening moves focus into the panel (closing returns it to the bookmark, see `Screen`).
    use_effect(move || {
        if open() {
            focus_without_scrolling("profile-close");
        }
    });
    // Pushing the panel back to the right closes it, as it came.
    use_swipes("profile-panel", move |swipe| {
        if swipe == Swipe::Right {
            on_close.call(());
        }
    });
    let is_superuser = me.read().is_superuser;
    let is_open = open();
    rsx! {
        div {
            class: "profile-backdrop",
            "data-open": is_open,
            "aria-hidden": "true",
            onclick: move |_| on_close.call(()),
        }
        aside {
            id: "profile-panel",
            class: "profile-panel",
            "data-open": is_open,
            "data-admin": is_superuser,
            "aria-label": "Menu profilu",
            // A closed panel is out of reach (`inert` must be absent, not "false", when open).
            "aria-hidden": (!is_open).then_some("true"),
            inert: (!is_open).then_some(true),
            span { class: "profile-panel-stitch-turn", "aria-hidden": "true" }
            div { class: "profile-panel-body",
                div { class: "profile-panel-top",
                    button {
                        id: "profile-close",
                        class: "profile-close",
                        r#type: "button",
                        "aria-label": "Zamknij menu",
                        onclick: move |_| on_close.call(()),
                        FoilCross {}
                    }
                }
                Legitymacja { me,
                    div { class: "menu",
                        NotificationsRow {}
                        MenuRow {
                            title: "Zmień nick lub zdjęcie",
                            hint: "sam tego nie zrobisz: złóż wniosek do Sejmiku",
                        }
                        MenuRow { title: "Moje statystyki", hint: "głosy, zakłady i pakty" }
                    }
                    if is_superuser {
                        AdminSection {}
                    }
                }
                div { class: "profile-footer",
                    FoilText { class: "profile-footer-note",
                        "Hasło zmienia tylko administrator, wystawiając nowy link."
                    }
                    button {
                        class: "profile-logout",
                        r#type: "button",
                        onclick: move |_| async move { session.logout().await },
                        FoilText { "WYLOGUJ" }
                    }
                }
            }
        }
    }
}

/// One line of the menu: a title, an italic hint and a chevron (or something else on the
/// right). Inert until the screens behind it exist.
#[component]
fn MenuRow(
    #[props(into)] title: String,
    #[props(into)] hint: String,
    trailing: Option<Element>,
) -> Element {
    rsx! {
        button { class: "menu-row", r#type: "button",
            span { class: "menu-row-text",
                span { class: "menu-row-title", "{title}" }
                span { class: "menu-row-hint", "{hint}" }
            }
            if let Some(trailing) = trailing {
                {trailing}
            } else {
                Chevron {}
            }
        }
    }
}

#[component]
fn Chevron() -> Element {
    rsx! {
        svg {
            class: "menu-row-chevron",
            width: "20",
            height: "20",
            view_box: "0 0 24 24",
            "aria-hidden": "true",
            path { d: "M9 5l7 7-7 7" }
        }
    }
}

/// Notifications: shown as off with a "WŁĄCZ" stamp (not wired up yet).
#[component]
fn NotificationsRow() -> Element {
    rsx! {
        div { class: "menu-row",
            span { class: "menu-row-text",
                span { class: "menu-row-title", "Powiadomienia" }
                span { class: "menu-row-hint", "wyłączone, nic Cię nie zaczepi" }
            }
            button { class: "stamp-button", r#type: "button", "WŁĄCZ" }
        }
    }
}

/// Administrators: the way into the management menu, under a stamped heading.
#[component]
fn AdminSection() -> Element {
    rsx! {
        div { class: "menu-section-rule", "aria-hidden": "true" }
        div { class: "menu-section-heading",
            span { "ZARZĄDZANIE" }
            Stamp { text: "ADMIN", tilt: -4.0, small: true }
        }
        div { class: "menu",
            // Opens the management menu once it exists.
            MenuRow {
                title: "Panel zarządzania",
                hint: "konta, zdjęcia i urodziny, statystyki, Mikołajki",
            }
        }
    }
}
