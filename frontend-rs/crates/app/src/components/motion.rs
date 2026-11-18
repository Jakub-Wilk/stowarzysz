//! The motion system (tokens and keyframes in `tailwind.css`). Animate transform and opacity
//! only, take colours from theme tokens, and use these instead of hand-rolled animation.
//! Reduced motion is handled globally in the CSS.

use std::time::Duration;

use dioxus::prelude::*;

/// Fades a page in whenever `id` changes (pass the route's path).
#[component]
pub fn PageTransition(id: String, children: Element) -> Element {
    rsx! {
        div { key: "{id}", class: "animate-fade-up", {children} }
    }
}

/// One block entering, delayed by its `index` in a list (capped, see the `stagger` utility).
#[component]
pub fn Reveal(
    #[props(default)] index: usize,
    #[props(into, default)] class: String,
    children: Element,
) -> Element {
    rsx! {
        div { class: "animate-fade-up stagger {class}", style: "--i: {index}", {children} }
    }
}

/// Counts from the previous value to `value` over `duration` (cubic ease-out).
pub fn use_count_up(value: f64, duration: Duration) -> ReadSignal<f64> {
    let mut shown = use_signal(|| value);
    let mut target = use_signal(|| value);
    use_effect(use_reactive!(|value| {
        if (*target.peek() - value).abs() < f64::EPSILON {
            return;
        }
        target.set(value);
        let from = *shown.peek();
        spawn(async move {
            const FRAME: Duration = Duration::from_millis(16);
            let steps = (duration.as_millis() / FRAME.as_millis()).max(1);
            for step in 1..=steps {
                #[allow(clippy::cast_precision_loss)]
                let t = step as f64 / steps as f64;
                let eased = 1.0 - (1.0 - t).powi(3);
                shown.set(from + (value - from) * eased);
                crate::platform::sleep(FRAME).await;
            }
        });
    }));
    shown.into()
}

/// `true` for `duration` after `key` changes (not on mount): add `animate-flash` while it is.
#[allow(clippy::needless_pass_by_value)] // hooks take their inputs by value
pub fn use_flash_on_change<K: PartialEq + Clone + 'static>(
    key: K,
    duration: Duration,
) -> ReadSignal<bool> {
    let mut flashing = use_signal(|| false);
    let mut mounted = use_signal(|| false);
    let mut generation = use_signal(|| 0u64);
    use_effect(use_reactive!(|key| {
        let _ = key;
        if !*mounted.peek() {
            mounted.set(true);
            return;
        }
        flashing.set(true);
        let current = *generation.peek() + 1;
        generation.set(current);
        spawn(async move {
            crate::platform::sleep(duration).await;
            if *generation.peek() == current {
                flashing.set(false);
            }
        });
    }));
    flashing.into()
}
