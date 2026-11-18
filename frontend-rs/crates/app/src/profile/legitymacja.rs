//! The membership card (legitymacja): who you are in the association, on cream paper.

use api::types::Me;
use dioxus::prelude::*;

use crate::components::{Avatar, image_url};

/// `0007`: the member's number, from their id.
pub fn card_number(id: i64) -> String {
    format!("{id:04}")
}

/// The card, with the menu below its fold (`children`).
#[component]
pub fn Legitymacja(me: ReadSignal<Me>, children: Element) -> Element {
    let me = me();
    let status = if me.is_superuser {
        "administrator"
    } else {
        "członek"
    };
    let image = image_url(me.avatar.as_ref(), 98);
    rsx! {
        div { class: "legitymacja",
            div { class: "legitymacja-heading",
                span { class: "legitymacja-issuer", "STOWARZYSZENIE" }
                span { class: "legitymacja-title", "LEGITYMACJA" }
                span { class: "legitymacja-rule", "aria-hidden": "true" }
            }
            div { class: "legitymacja-details",
                div { class: "legitymacja-photo",
                    Avatar {
                        username: me.username.clone(),
                        image,
                        size: 92,
                        round: false,
                    }
                    span { class: "legitymacja-photo-stamp", "aria-hidden": "true", "S" }
                }
                dl { class: "legitymacja-fields",
                    div {
                        dt { "NICK" }
                        dd { "{me.username}" }
                    }
                    div {
                        dt { "NR LEGITYMACJI" }
                        dd { "{card_number(me.id)}" }
                    }
                    div {
                        dt { "STATUS" }
                        dd { "data-admin": me.is_superuser, "{status}" }
                    }
                }
            }
            div { class: "legitymacja-signature",
                span { class: "legitymacja-signature-name",
                    span { "{me.username}" }
                }
                span { class: "legitymacja-signature-caption", "podpis posiadacza" }
            }
            div { class: "legitymacja-fold", "aria-hidden": "true" }
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers_cards_with_four_digits() {
        assert_eq!(super::card_number(7), "0007");
        assert_eq!(super::card_number(1234), "1234");
        assert_eq!(super::card_number(12345), "12345");
    }
}
