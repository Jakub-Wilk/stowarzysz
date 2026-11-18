//! The desk every screen lies on, and the scrolling area under the header.

use dioxus::prelude::*;

/// The full-screen desk with a centred column (phone width on large screens). The column holds
/// the header, the content and overlays (the profile panel slides in from its right edge).
#[component]
pub fn Desk(
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "desk",
            div {
                class: "desk-column",
                onkeydown: move |event| {
                    if let Some(handler) = &onkeydown {
                        handler.call(event);
                    }
                },
                {children}
            }
        }
    }
}

/// Move keyboard focus to the element with `id` without scrolling anything to it (an element
/// sliding in from off-screen must not drag its container along).
pub fn focus_without_scrolling(id: &str) {
    document::eval(&format!(
        "document.getElementById({id:?})?.focus({{ preventScroll: true }});"
    ));
}

/// The scrolling area below the header.
#[component]
pub fn DeskScroll(#[props(into, default)] class: String, children: Element) -> Element {
    rsx! {
        main { class: "desk-scroll {class}", {children} }
    }
}
