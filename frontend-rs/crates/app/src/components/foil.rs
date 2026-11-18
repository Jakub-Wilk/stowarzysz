//! Gold foil pressed into leather (the profile bookmark, the profile panel): a vertical gold
//! gradient with a debossed edge. Text uses CSS (`FoilText`); strokes (the close cross) need
//! the SVG gradient, defined once by `FoilDefs` (rendered in `App`).

use dioxus::prelude::*;

/// `url(#foil)` (gradient) and `url(#foil-deboss)` (filter) for SVG and CSS. Render once.
#[component]
pub fn FoilDefs() -> Element {
    rsx! {
        svg {
            width: "0",
            height: "0",
            class: "absolute",
            "aria-hidden": "true",
            "focusable": "false",
            defs {
                linearGradient {
                    id: "foil",
                    "gradientUnits": "objectBoundingBox",
                    x1: "0",
                    y1: "0",
                    x2: "0",
                    y2: "1",
                    stop { offset: "0", style: "stop-color: var(--foil-1)" }
                    stop { offset: "0.3", style: "stop-color: var(--foil-2)" }
                    stop { offset: "0.52", style: "stop-color: var(--foil-3)" }
                    stop { offset: "0.53", style: "stop-color: var(--foil-4)" }
                    stop { offset: "1", style: "stop-color: var(--foil-5)" }
                }
                filter {
                    id: "foil-deboss",
                    x: "-10%",
                    y: "-20%",
                    width: "125%",
                    height: "160%",
                    "color-interpolation-filters": "sRGB",
                    feOffset { "in": "SourceAlpha", dx: "0", dy: "0.6", result: "o1" }
                    feFlood { style: "flood-color: var(--foil-lip)", "flood-opacity": "0.4" }
                    feComposite { in2: "o1", operator: "in", result: "lc" }
                    feComposite { "in": "lc", in2: "SourceAlpha", operator: "out", result: "lip" }
                    feOffset { "in": "SourceAlpha", dx: "0", dy: "0.8", result: "o2" }
                    feGaussianBlur { "in": "o2", "stdDeviation": "0.35", result: "o2b" }
                    feComposite { "in": "SourceAlpha", in2: "o2b", operator: "out", result: "edge" }
                    feFlood { style: "flood-color: var(--foil-shade)", "flood-opacity": "0.7" }
                    feComposite { in2: "edge", operator: "in", result: "inner" }
                    feMerge {
                        feMergeNode { "in": "lip" }
                        feMergeNode { "in": "SourceGraphic" }
                        feMergeNode { "in": "inner" }
                    }
                }
            }
        }
    }
}

/// Text in gold foil.
#[component]
pub fn FoilText(#[props(into, default)] class: String, children: Element) -> Element {
    rsx! {
        span { class: "foil-text {class}", {children} }
    }
}

/// The close cross, stamped in foil.
#[component]
pub fn FoilCross(#[props(default = 44)] size: u32) -> Element {
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 44 44",
            "aria-hidden": "true",
            path {
                d: "M15 15L29 29M29 15L15 29",
                stroke: "url(#foil)",
                stroke_width: "3.4",
                stroke_linecap: "round",
                fill: "none",
                filter: "url(#foil-deboss)",
            }
        }
    }
}
