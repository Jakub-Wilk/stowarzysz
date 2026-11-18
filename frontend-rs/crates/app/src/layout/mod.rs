//! Guards and the signed-in screen frame: the desk, the header with the profile bookmark, the
//! page, and the profile menu over it.

use api::types::Me;
use dioxus::prelude::*;

use crate::components::{AppHeader, Button, Desk, Icon, Spinner, Svg, focus_without_scrolling};
use crate::profile::ProfileOverlay;
use crate::routes::Route;
use crate::session::{Auth, use_session};

/// Only for signed-in members; everyone else goes to the login screen.
#[component]
pub fn SignedIn() -> Element {
    let session = use_session();
    let navigator = use_navigator();
    let auth = session.auth.read().clone();
    use_effect(move || {
        if matches!(*session.auth.read(), Auth::SignedOut) {
            navigator.replace(Route::Login {});
        }
    });
    match auth {
        Auth::SignedIn(_) => rsx! { Outlet::<Route> {} },
        Auth::Checking | Auth::SignedOut => rsx! { FullScreenLoading {} },
        Auth::Unreachable(message) => rsx! {
            div { class: "flex min-h-svh flex-col items-center justify-center gap-4 p-8 text-center",
                Svg { icon: Icon::WifiOff, size: 40 }
                p { class: "max-w-sm", "{message}" }
                Button { onclick: move |_| session.reload(), "Spróbuj ponownie" }
            }
        },
    }
}

/// Only for administrators.
#[component]
pub fn Superuser() -> Element {
    let session = use_session();
    let navigator = use_navigator();
    let allowed = session.is_superuser();
    use_effect(move || {
        if !session.is_superuser() {
            navigator.replace(Route::Home {});
        }
    });
    if allowed {
        rsx! { Outlet::<Route> {} }
    } else {
        rsx! {}
    }
}

#[component]
pub fn FullScreenLoading() -> Element {
    rsx! {
        div { class: "flex min-h-svh items-center justify-center gap-3 text-muted-foreground",
            Spinner {}
            "Ładowanie…"
        }
    }
}

/// Every signed-in screen: the desk, the header (wordmark and profile bookmark), the page and
/// the profile menu, which works the same on every screen.
#[component]
pub fn Screen() -> Element {
    match use_session().me() {
        Some(me) => rsx! { ScreenFrame { me } },
        None => rsx! {},
    }
}

#[component]
fn ScreenFrame(me: ReadSignal<Me>) -> Element {
    let mut menu_open = use_signal(|| false);
    let mut close = move || {
        if *menu_open.peek() {
            menu_open.set(false);
            // Focus goes back where it came from.
            focus_without_scrolling("profile-bookmark");
        }
    };
    rsx! {
        Desk {
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape {
                    close();
                }
            },
            AppHeader {
                me,
                menu_open,
                on_bookmark: move |_| menu_open.toggle(),
                on_swipe_open: move |()| menu_open.set(true),
            }
            Outlet::<Route> {}
            ProfileOverlay { me, open: menu_open, on_close: move |()| close() }
        }
    }
}
