//! A pine garland around a card (the Mikołajki card), drawn for the card's actual size: a dark
//! rope along the edge with thousands of short needles on it, lit from the top left. Generated
//! (with a fixed seed, so it never flickers) instead of a fixed drawing, which could only fit
//! one card size.

use std::f64::consts::{PI, TAU};
use std::fmt::Write as _;

use dioxus::prelude::*;

/// How far the garland may reach outside the card.
const MARGIN: f64 = 18.0;
/// Needles per pixel of rope (the design's drawing has about 6).
const DENSITY: f64 = 6.2;
/// Needle shades, darkest first, and the share of needles in each (the lit side is the rarest).
const SHADES: [(&str, f64, f64); 6] = [
    ("#0F3F1A", 1.2, 0.33),
    ("#16502A", 1.2, 0.34),
    ("#1F6B2D", 1.5, 0.25),
    ("#2A7F37", 1.2, 0.07),
    ("#3A9344", 1.2, 0.009),
    ("#55B25A", 1.2, 0.001),
];
const ROPE: &str = "#123D1B";

/// One SVG path: the rope, or all needles of one shade.
#[derive(Debug, Clone, PartialEq)]
pub struct Strand {
    pub color: &'static str,
    pub width: f64,
    pub d: String,
}

/// A small deterministic generator (xorshift64*), so the garland is the same on every render.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        #[allow(clippy::cast_precision_loss)]
        let value = (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64;
        value / 9_007_199_254_740_992.0 // 2^53
    }

    fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }
}

/// The rope's centre line: the card's edge (a rounded rectangle, in a frame `MARGIN` wider on
/// every side), as a function of the distance travelled along it.
struct Loop {
    width: f64,
    height: f64,
    radius: f64,
    phases: [f64; 2],
}

impl Loop {
    fn perimeter(&self) -> f64 {
        let r = self.radius;
        2.0 * (self.width - 2.0 * r) + 2.0 * (self.height - 2.0 * r) + TAU * r
    }

    /// The point at `s` along the loop and the outward normal there, before wobble.
    fn at(&self, s: f64) -> ((f64, f64), (f64, f64)) {
        let (w, h, r) = (self.width, self.height, self.radius);
        let (x0, y0) = (MARGIN, MARGIN);
        let top = w - 2.0 * r;
        let side = h - 2.0 * r;
        let arc = PI / 2.0 * r;
        let corner = |cx: f64, cy: f64, start: f64, t: f64| {
            let angle = start + t / r;
            let (sin, cos) = angle.sin_cos();
            ((cx + r * cos, cy + r * sin), (cos, sin))
        };
        let mut s = s.rem_euclid(self.perimeter());
        // Top edge, left to right.
        if s < top {
            return ((x0 + r + s, y0), (0.0, -1.0));
        }
        s -= top;
        if s < arc {
            return corner(x0 + w - r, y0 + r, -PI / 2.0, s);
        }
        s -= arc;
        if s < side {
            return ((x0 + w, y0 + r + s), (1.0, 0.0));
        }
        s -= side;
        if s < arc {
            return corner(x0 + w - r, y0 + h - r, 0.0, s);
        }
        s -= arc;
        if s < top {
            return ((x0 + w - r - s, y0 + h), (0.0, 1.0));
        }
        s -= top;
        if s < arc {
            return corner(x0 + r, y0 + h - r, PI / 2.0, s);
        }
        s -= arc;
        if s < side {
            return ((x0, y0 + h - r - s), (-1.0, 0.0));
        }
        s -= side;
        corner(x0 + r, y0 + r, PI, s.min(arc))
    }

    /// The rope wanders a little either side of the edge.
    fn point(&self, s: f64) -> (f64, f64) {
        let ((x, y), (nx, ny)) = self.at(s);
        let wobble = 1.6 * (s / 61.0 * TAU + self.phases[0]).sin()
            + 1.0 * (s / 23.0 * TAU + self.phases[1]).sin();
        (x + nx * wobble, y + ny * wobble)
    }
}

/// The garland for a card of `width` × `height` px, in a frame `MARGIN` px wider on every side.
pub fn garland(width: f64, height: f64, seed: u64) -> Vec<Strand> {
    let mut rng = Rng(seed | 1);
    let edge = Loop {
        width,
        height,
        radius: 10.0_f64.min(width / 2.0).min(height / 2.0),
        phases: [rng.range(0.0, TAU), rng.range(0.0, TAU)],
    };
    let perimeter = edge.perimeter();

    let mut rope = String::new();
    let mut s = 0.0;
    while s <= perimeter {
        let (x, y) = edge.point(s);
        let op = if rope.is_empty() { 'M' } else { 'L' };
        let _ = write!(rope, "{op}{x:.1} {y:.1} ");
        s += 3.0;
    }
    rope.push('Z');

    // Needles: anywhere on the rope, any direction; the ones pointing up and left catch the
    // light. Shades are handed out by brightness rank, so each keeps its share.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (perimeter * DENSITY) as usize;
    let mut needles: Vec<(f64, String)> = (0..count)
        .map(|_| {
            let (x, y) = edge.point(rng.range(0.0, perimeter));
            let (x, y) = (x + rng.range(-1.5, 1.5), y + rng.range(-1.5, 1.5));
            let angle = rng.range(0.0, TAU);
            let length = 8.0 + 5.0 * rng.next().sqrt();
            let (dy, dx) = angle.sin_cos();
            let light = -dy * 0.6 - dx * 0.4 + rng.range(-0.35, 0.35);
            let needle = format!("M{x:.1} {y:.1}l{:.1} {:.1}", dx * length, dy * length);
            (light, needle)
        })
        .collect();
    needles.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut strands = vec![Strand {
        color: ROPE,
        width: 12.0,
        d: rope,
    }];
    let mut taken = 0;
    for (index, (color, width, share)) in SHADES.iter().enumerate() {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let end = if index == SHADES.len() - 1 {
            count
        } else {
            (taken + (count as f64 * share).round() as usize).min(count)
        };
        let d = needles[taken..end]
            .iter()
            .map(|(_, needle)| needle.as_str())
            .collect::<String>();
        strands.push(Strand {
            color,
            width: *width,
            d,
        });
        taken = end;
    }
    strands
}

/// The garland, sized to the element it is placed in (which must be positioned).
#[component]
pub fn Garland(#[props(default = 7)] seed: u64) -> Element {
    let mut size = use_signal(|| None::<(u32, u32)>);
    let strands = use_memo(move || size().map(|(w, h)| garland(f64::from(w), f64::from(h), seed)));
    rsx! {
        div {
            class: "garland-measure",
            "aria-hidden": "true",
            onresize: move |event| {
                if let Ok(box_size) = event.data().get_border_box_size() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let next = (box_size.width.round() as u32, box_size.height.round() as u32);
                    if size() != Some(next) && next.0 > 0 && next.1 > 0 {
                        size.set(Some(next));
                    }
                }
            },
        }
        if let (Some((w, h)), Some(strands)) = (size(), strands()) {
            svg {
                class: "garland",
                "aria-hidden": "true",
                width: "{f64::from(w) + 2.0 * MARGIN}",
                height: "{f64::from(h) + 2.0 * MARGIN}",
                view_box: "0 0 {f64::from(w) + 2.0 * MARGIN} {f64::from(h) + 2.0 * MARGIN}",
                for (i, strand) in strands.iter().enumerate() {
                    path {
                        key: "{i}",
                        d: "{strand.d}",
                        fill: "none",
                        stroke: strand.color,
                        stroke_width: "{strand.width}",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_card_it_is_drawn_for() {
        for (w, h) in [(358.0, 120.0), (300.0, 90.0), (448.0, 160.0)] {
            let strands = garland(w, h, 7);
            assert_eq!(strands.len(), 1 + SHADES.len());
            // The rope stays within a few pixels of the card's edge.
            let numbers: Vec<f64> = strands[0]
                .d
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter_map(|n| n.parse().ok())
                .collect();
            let (xs, ys): (Vec<f64>, Vec<f64>) = numbers.chunks(2).map(|p| (p[0], p[1])).unzip();
            let bounds = |v: &[f64]| {
                (
                    v.iter().copied().fold(f64::MAX, f64::min),
                    v.iter().copied().fold(f64::MIN, f64::max),
                )
            };
            let (x0, x1) = bounds(&xs);
            let (y0, y1) = bounds(&ys);
            assert!(
                (x0 - MARGIN).abs() < 3.0 && (x1 - (MARGIN + w)).abs() < 3.0,
                "{w}: {x0}..{x1}"
            );
            assert!(
                (y0 - MARGIN).abs() < 3.0 && (y1 - (MARGIN + h)).abs() < 3.0,
                "{h}: {y0}..{y1}"
            );
            // Needles scale with the rope's length, darkest shades most common.
            let needles: Vec<usize> = strands[1..]
                .iter()
                .map(|s| s.d.matches('M').count())
                .collect();
            let total: usize = needles.iter().sum();
            assert!(total > 1000, "{total}");
            assert!(needles[0] > needles[3] && needles[3] > needles[4]);
        }
    }

    #[test]
    fn is_the_same_every_time() {
        assert_eq!(garland(358.0, 120.0, 7), garland(358.0, 120.0, 7));
        assert_ne!(garland(358.0, 120.0, 7), garland(358.0, 120.0, 8));
    }
}
