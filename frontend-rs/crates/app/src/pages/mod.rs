//! Screens. The module pages are placeholders until their own plans: only their routes exist.
//! The home screen lives in `crate::home`.

mod diagnostics;
mod login;

pub use diagnostics::Diagnostics;
use dioxus::prelude::*;
pub use login::Login;

use crate::components::DeskScroll;
use crate::routes::Route;

/// What a not-yet-built screen shows: a note on the desk.
#[component]
fn Placeholder(title: String) -> Element {
    rsx! {
        DeskScroll {
            div { class: "placeholder-note",
                h2 { "{title}" }
                p { "Wkrótce." }
                Link { to: Route::Home {}, "Wróć do ekranu głównego" }
            }
        }
    }
}

#[component]
pub fn Activate(token: String) -> Element {
    let _ = token;
    rsx! {
        div { class: "mx-auto max-w-md p-8",
            h1 { "Aktywacja konta" }
            p { "Wkrótce." }
        }
    }
}

#[component]
pub fn Voting() -> Element {
    rsx! { Placeholder { title: "Sejmik" } }
}

#[component]
pub fn Pacts() -> Element {
    rsx! { Placeholder { title: "Umowy" } }
}

#[component]
pub fn Ledger() -> Element {
    rsx! { Placeholder { title: "Kasa Skarbowa" } }
}

#[component]
pub fn SecretSanta() -> Element {
    rsx! { Placeholder { title: "Mikołajki" } }
}

#[component]
pub fn PollNew() -> Element {
    rsx! { Placeholder { title: "Nowe głosowanie" } }
}

#[component]
pub fn PollDetail(id: i64) -> Element {
    rsx! { Placeholder { title: "Głosowanie #{id}" } }
}

#[component]
pub fn PactNew() -> Element {
    rsx! { Placeholder { title: "Nowa umowa" } }
}

#[component]
pub fn PactDetail(id: i64) -> Element {
    rsx! { Placeholder { title: "Umowa #{id}" } }
}

/// `/ledger/new` is the expense form.
#[component]
pub fn LedgerNew() -> Element {
    let navigator = use_navigator();
    use_hook(move || {
        navigator.replace(Route::EntryNew {
            kind: "expense".to_owned(),
        });
    });
    rsx! {}
}

#[component]
pub fn EntryNew(kind: String) -> Element {
    let title = match kind.as_str() {
        "income" => "Nowy przychód",
        "debt" => "Nowy dług",
        _ => "Nowy wydatek",
    };
    rsx! { Placeholder { title } }
}

#[component]
pub fn EntryDetail(id: i64) -> Element {
    rsx! { Placeholder { title: "Wpis #{id}" } }
}

#[component]
pub fn EntryEdit(id: i64) -> Element {
    rsx! { Placeholder { title: "Edycja wpisu #{id}" } }
}

#[component]
pub fn Manage() -> Element {
    rsx! { Placeholder { title: "Zarządzanie" } }
}

#[component]
pub fn ManageUsers() -> Element {
    rsx! { Placeholder { title: "Konta" } }
}

#[component]
pub fn ManageUserNew() -> Element {
    rsx! { Placeholder { title: "Nowe konto" } }
}

#[component]
pub fn ManageUser(id: i64) -> Element {
    rsx! { Placeholder { title: "Konto #{id}" } }
}

#[component]
pub fn ManageSanta() -> Element {
    rsx! { Placeholder { title: "Mikołajki: zarządzanie" } }
}

/// Unknown URLs go home (or to the login screen, through the guard).
#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let _ = segments;
    let navigator = use_navigator();
    use_hook(move || {
        navigator.replace(Route::Home {});
    });
    rsx! {}
}
