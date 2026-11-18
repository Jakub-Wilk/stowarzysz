//! The header on every screen: something on the left (the wordmark on the home screen, a
//! module's paper label in a module), the profile bookmark on the right, a groove below.

use api::types::Me;
use dioxus::prelude::*;

use super::avatar::{Avatar, image_url};
use super::foil::FoilText;
use super::gesture::{Swipe, use_swipes};
use crate::polish::dates;

#[component]
pub fn AppHeader(
    me: ReadSignal<Me>,
    /// Whether the profile panel is open (the bookmark slides into it).
    menu_open: ReadSignal<bool>,
    on_bookmark: EventHandler<MouseEvent>,
    /// The bookmark was pulled to the left: open the profile.
    on_swipe_open: EventHandler<()>,
    /// The left side; the wordmark when absent.
    left: Option<Element>,
) -> Element {
    rsx! {
        header { class: "app-header",
            div { class: "app-header-left",
                if let Some(left) = left {
                    {left}
                } else {
                    Brand {}
                }
            }
            ProfileBookmark {
                me,
                open: menu_open,
                onclick: on_bookmark,
                on_swipe_open,
            }
        }
        div { class: "groove", "aria-hidden": "true" }
    }
}

/// `Stowarzysz` with `KANCELARIA · SOB. 14.11` under it.
#[component]
pub fn Brand() -> Element {
    let date = dates::desk_date(dates::now());
    rsx! {
        div { class: "brand",
            span { class: "brand-name", "Stowarzysz" }
            span { class: "brand-line", "KANCELARIA · {date}" }
        }
    }
}

/// The burgundy ribbon with the member's picture and name in gold foil; opens the profile when
/// tapped or pulled to the left (the way it unrolls).
#[component]
pub fn ProfileBookmark(
    me: ReadSignal<Me>,
    open: ReadSignal<bool>,
    onclick: EventHandler<MouseEvent>,
    on_swipe_open: EventHandler<()>,
) -> Element {
    let me = me();
    let image = image_url(me.avatar.as_ref(), 34);
    use_swipes("profile-bookmark", move |swipe| {
        if swipe == Swipe::Left && !*open.peek() {
            on_swipe_open.call(());
        }
    });
    rsx! {
        button {
            id: "profile-bookmark",
            class: "bookmark",
            r#type: "button",
            "aria-label": "Profil: {me.username}",
            "aria-expanded": open(),
            "aria-controls": "profile-panel",
            onclick: move |event| onclick.call(event),
            BookmarkShape {}
            Avatar { username: me.username.clone(), image, size: 34, class: "bookmark-avatar" }
            span { class: "bookmark-text",
                FoilText { class: "bookmark-name", "{me.username}" }
                FoilText { class: "bookmark-caption", "PROFIL" }
            }
        }
    }
}

/// The ribbon: a pointed end on the left, dashed stitching along it.
#[component]
fn BookmarkShape() -> Element {
    rsx! {
        svg {
            class: "bookmark-shape",
            width: "100%",
            height: "56",
            view_box: "0 0 195 56",
            "preserveAspectRatio": "none",
            "aria-hidden": "true",
            defs {
                linearGradient { id: "bookmark-gradient", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", style: "stop-color: var(--bookmark-top)" }
                    stop { offset: "1", style: "stop-color: var(--bookmark-bottom)" }
                }
            }
            path {
                class: "bookmark-ribbon",
                fill: "url(#bookmark-gradient)",
                d: "M26 1 H195 V55 H26 Q20 55 16 49 L4 33 Q0 28 4 23 L16 7 Q20 1 26 1 Z",
            }
            path {
                class: "bookmark-stitch",
                d: "M195 6 H28 Q24 6 21 10 L10 24 Q8 28 10 32 L21 46 Q24 50 28 50 H195",
            }
        }
    }
}
