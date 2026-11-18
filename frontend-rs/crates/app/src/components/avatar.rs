//! A member's picture: their photo (a signed URL from the API) or, without one, a blue disc
//! with their initial.

use api::types::Image;
use dioxus::prelude::*;

use crate::session::use_client;

/// The signed URL of an image at the size shown (`sm` up to 128 px, `md` above).
pub fn image_url(image: Option<&Image>, size: u32) -> Option<String> {
    image.map(|image| {
        if size > 128 {
            image.md.clone()
        } else {
            image.sm.clone()
        }
    })
}

#[component]
pub fn Avatar(
    #[props(into)] username: String,
    /// `image_url(...)` of the person's avatar.
    image: Option<String>,
    #[props(default = 34)] size: u32,
    /// `round` (default) or `square` (the membership card's photo).
    #[props(default = true)]
    round: bool,
    #[props(into, default)] class: String,
) -> Element {
    let client = use_client();
    let initial = username
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let shape = if round {
        "avatar-round"
    } else {
        "avatar-square"
    };
    rsx! {
        span {
            class: "avatar {shape} {class}",
            style: "--size: {size}px",
            if let Some(image) = image {
                img { src: client.url(&image), alt: "" }
            } else {
                span { class: "avatar-initial", "aria-hidden": "true", "{initial}" }
            }
        }
    }
}
