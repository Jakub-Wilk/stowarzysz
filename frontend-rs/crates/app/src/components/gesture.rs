//! Swipes: a pointer dragged sideways far enough (the profile bookmark is pulled left to open,
//! the profile panel pushed right to close). Elements that take them set `touch-action: pan-y`,
//! so vertical drags still scroll.
//!
//! The drag is followed by plain browser listeners, and only the finished swipe reaches Rust:
//! routing every `pointermove` through the app made a swipe queue up dozens of events, and taps
//! right after it waited behind them.
//!
//! A sideways touch drag is also kept from the browser (`preventDefault` on its `touchmove`).
//! Otherwise its small vertical part starts a scroll, the release starts a fling (momentum
//! scrolling, even with nothing to scroll), and while a fling runs Chrome spends the next tap
//! on stopping it: the first tap after a swipe did nothing for a second or two.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;

/// How far (px) a pointer must travel sideways to count as a swipe.
const DISTANCE: u32 = 32;
/// And how much more sideways than up or down.
const RATIO: f64 = 1.5;
/// How long after a mouse swipe its closing click is ignored (ms).
const CLICK_GRACE: u32 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swipe {
    Left,
    Right,
}

/// Call `on_swipe` whenever the element with `id` is swiped. The element must exist for as long
/// as the calling component does (it is looked up once, after the first render).
/// `on_swipe` is kept from the first render (it should only capture signals and handlers).
pub fn use_swipes(id: &'static str, on_swipe: impl FnMut(Swipe) + 'static) {
    let on_swipe = use_hook(move || Rc::new(RefCell::new(on_swipe)));
    // After the first render, once the element is in the document (reads no signals: runs once).
    use_effect(move || {
        let on_swipe = on_swipe.clone();
        spawn(async move {
            let script = format!(
                r"
                const element = document.getElementById({id:?});
                if (!element) return;
                let start = null;
                element.addEventListener('pointerdown', (event) => {{
                    start = {{ id: event.pointerId, x: event.clientX, y: event.clientY }};
                }}, {{ passive: true }});
                element.addEventListener('pointermove', (event) => {{
                    if (!start || event.pointerId !== start.id) return;
                    const dx = event.clientX - start.x, dy = event.clientY - start.y;
                    if (Math.abs(dx) < {DISTANCE} || Math.abs(dx) <= {RATIO} * Math.abs(dy)) return;
                    start = null;
                    // A mouse drag ends in a click; it must not count as a tap. Touch swipes
                    // produce no click, so nothing is swallowed for them.
                    if (event.pointerType === 'mouse') {{
                        const swallow = (click) => {{ click.stopPropagation(); click.preventDefault(); }};
                        window.addEventListener('click', swallow, {{ capture: true, once: true }});
                        setTimeout(() => window.removeEventListener('click', swallow, {{ capture: true }}), {CLICK_GRACE});
                    }}
                    dioxus.send(dx < 0 ? 'left' : 'right');
                }}, {{ passive: true }});
                let touch = null;
                element.addEventListener('touchstart', (event) => {{
                    const point = event.touches[0];
                    touch = {{ x: point.clientX, y: point.clientY }};
                }}, {{ passive: true }});
                element.addEventListener('touchmove', (event) => {{
                    if (!touch || !event.cancelable) return;
                    const point = event.touches[0];
                    const dx = Math.abs(point.clientX - touch.x), dy = Math.abs(point.clientY - touch.y);
                    // Sideways: ours. Up and down stays the browser's (the panel scrolls).
                    if (dx > dy) event.preventDefault();
                }}, {{ passive: false }});
                element.addEventListener('touchend', () => {{ touch = null; }}, {{ passive: true }});
                const end = () => {{ start = null; }};
                element.addEventListener('pointerup', end, {{ passive: true }});
                element.addEventListener('pointercancel', end, {{ passive: true }});
                await new Promise(() => {{}});
                "
            );
            let mut eval = document::eval(&script);
            while let Ok(direction) = eval.recv::<String>().await {
                let swipe = if direction == "left" {
                    Swipe::Left
                } else {
                    Swipe::Right
                };
                (on_swipe.borrow_mut())(swipe);
            }
        });
    });
}
