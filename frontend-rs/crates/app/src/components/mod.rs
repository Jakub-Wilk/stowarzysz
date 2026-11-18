//! Shared building blocks. The desk-and-paper parts (`Desk`, `AppHeader`, `ModuleSurface`,
//! `PaperLabel`, `Stamp`, `Avatar`, foil) are styled by `assets/ui/*.css` from theme tokens.
//! The generic controls keep stable hooks for themes: `[data-slot='button']` on every button
//! and `.lucide` on every icon.

mod avatar;
mod desk;
mod foil;
mod gesture;
mod header;
mod icons;
mod module;
pub mod motion;
mod stamp;

pub use avatar::{Avatar, image_url};
pub use desk::{Desk, DeskScroll, focus_without_scrolling};
use dioxus::prelude::*;
pub use foil::{FoilCross, FoilDefs, FoilText};
pub use gesture::{Swipe, use_swipes};
pub use header::AppHeader;
pub use icons::{Icon, Svg};
pub use module::{Module, ModuleSurface, Paper, PaperLabel};
pub use stamp::{Stamp, StampTone};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Outline,
    /// A text button; always underlined, so it reads as a button.
    Ghost,
    Destructive,
    /// A square icon-only button (give it an `aria_label`).
    Icon,
}

/// Every clickable thing; don't style a bare `button` instead.
#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] small: bool,
    #[props(default)] disabled: bool,
    /// A form's submit button.
    #[props(default)]
    submit: bool,
    #[props(into, default)] aria_label: Option<String>,
    #[props(into, default)] class: String,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let size = match (variant, small) {
        (ButtonVariant::Icon, false) => "size-10",
        (ButtonVariant::Icon, true) => "size-8",
        (_, false) => "h-10 px-4",
        (_, true) => "h-8 px-3",
    };
    let look = match variant {
        ButtonVariant::Primary => {
            "bg-primary text-primary-foreground shadow-xs hover:bg-primary/90"
        }
        ButtonVariant::Outline => {
            "border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground"
        }
        ButtonVariant::Ghost => {
            "underline underline-offset-4 hover:bg-accent hover:text-accent-foreground"
        }
        ButtonVariant::Destructive => "bg-destructive text-white shadow-xs hover:bg-destructive/90",
        ButtonVariant::Icon => "hover:bg-accent hover:text-accent-foreground",
    };
    rsx! {
        button {
            "data-slot": "button",
            r#type: if submit { "submit" } else { "button" },
            class: "inline-flex shrink-0 items-center justify-center gap-2 rounded-md text-sm font-medium whitespace-nowrap outline-none transition-[transform,background-color,opacity] duration-(--duration-fast) active:scale-[0.97] disabled:pointer-events-none disabled:opacity-50 {size} {look} {class}",
            disabled,
            "aria-label": aria_label,
            onclick: move |event| {
                if let Some(handler) = &onclick {
                    handler.call(event);
                }
            },
            {children}
        }
    }
}

#[component]
pub fn Spinner(#[props(default = 20)] size: u32) -> Element {
    rsx! {
        span { class: "inline-flex animate-spin", role: "status", "aria-label": "Ładowanie",
            Svg { icon: Icon::LoaderCircle, size }
        }
    }
}

/// A placeholder block while something loads.
#[component]
pub fn Skeleton(#[props(into, default)] class: String) -> Element {
    rsx! {
        div { class: "animate-shimmer rounded-md bg-muted {class}" }
    }
}
