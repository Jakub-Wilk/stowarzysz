//! stowarzysz: the PWA and the Android app (Dioxus). See frontend-rs/README.md.

// `rsx!` expands event handlers into fully qualified paths.
#![allow(unused_qualifications)]
// The foundations: helpers (plurals, money, dates, motion) have no callers until the feature
// pages land. Drop this once they do.
#![allow(dead_code)]

mod components;
mod config;
mod home;
mod layout;
mod pages;
mod platform;
mod polish;
mod profile;
mod query;
mod realtime;
mod routes;
mod session;
mod theme;
mod updates;

use dioxus::prelude::*;

use crate::components::FoilDefs;
use crate::realtime::RealtimeConnection;
use crate::routes::Route;
use crate::session::{Auth, use_client};
use crate::updates::UpdateBanner;

const TAILWIND: Asset = asset!("/assets/tailwind.css");
/// Copied as is (no hash), so `fonts.css` can point at `fonts/<file>` next to the stylesheet.
const FONTS: Asset = asset!("/assets/fonts", AssetOptions::folder());

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_hook(platform::init);
    theme::use_theme_provider();
    let bus = query::use_refetch_bus_provider();
    let session = session::use_session_provider();
    realtime::use_realtime_provider();
    let updates = updates::use_updates_provider();
    let client = use_client();

    // Back in the foreground: refetch what is on screen, and look for updates now and then.
    use_hook(move || {
        spawn(async move {
            let mut eval = document::eval(
                r"
                const fire = () => { if (document.visibilityState === 'visible') dioxus.send(true); };
                document.addEventListener('visibilitychange', fire);
                window.addEventListener('focus', fire);
                ",
            );
            let mut last = chrono::Utc::now();
            while eval.recv::<bool>().await.is_ok() {
                let now = chrono::Utc::now();
                if now - last < chrono::TimeDelta::seconds(2) {
                    continue; // focus and visibilitychange arrive together
                }
                last = now;
                if matches!(*session.auth.peek(), Auth::SignedIn(_)) {
                    bus.bump_all();
                }
                spawn(updates.check(client.clone(), false));
            }
        });
    });

    let signed_in = matches!(*session.auth.read(), Auth::SignedIn(_));
    let main_font = format!("{FONTS}/manrope-latin-wght-normal.woff2");
    rsx! {
        document::Stylesheet { href: TAILWIND }
        document::Link {
            rel: "preload",
            href: main_font,
            r#as: "font",
            r#type: "font/woff2",
            crossorigin: "anonymous",
        }
        FoilDefs {}
        if signed_in {
            RealtimeConnection {}
        }
        div { class: "pointer-events-none fixed inset-x-0 top-[calc(4.5rem+env(safe-area-inset-top))] z-40 mx-auto max-w-2xl [&>*]:pointer-events-auto",
            UpdateBanner {}
        }
        Router::<Route> {}
    }
}
