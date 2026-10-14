//! The radar's geometry (docs/product-design/10-radar-window.md): turning the polar places core
//! decided (`radius 0..1`, `angle` clockwise from 12 o'clock) into drawing coordinates, and
//! pointing at things. Pure, so it is tested without a browser.
//!
//! The drawing is a square of [`VIEW`] units that the browser scales to the window, centred
//! (letterboxed) when the window is not square. Everything here is in those units unless it says
//! "px"; a pointer is converted into them first ([`to_view`]).

use minimap_types::BlipSize;

/// The side of the drawing's square.
pub const VIEW: f64 = 200.0;
/// The centre of the drawing.
pub const CENTRE: f64 = VIEW / 2.0;
/// The radius of the outer edge (radius 1), leaving room for the edge marks beyond it.
pub const RADIUS: f64 = 90.0;
/// How near a pointer must be to a blip to count as pointing at it, in screen pixels.
pub const HIT_TOLERANCE_PX: f64 = 6.0;

/// A place in the drawing for a polar place (`r` 0 = the centre, 1 = the outer edge; `theta`
/// radians clockwise from 12 o'clock).
pub fn point(r: f64, theta: f64) -> (f64, f64) {
    (
        CENTRE + RADIUS * r * theta.sin(),
        CENTRE - RADIUS * r * theta.cos(),
    )
}

/// A place beyond the edge, `outset` units out from radius 1 (for the edge marks).
pub fn rim_point(theta: f64, outset: f64) -> (f64, f64) {
    (
        CENTRE + (RADIUS + outset) * theta.sin(),
        CENTRE - (RADIUS + outset) * theta.cos(),
    )
}

/// The radius of a circle at `r` (0 to 1) in drawing units.
pub fn ring_radius(r: f64) -> f64 {
    RADIUS * r
}

/// How big a blip is drawn (its radius), by priority size.
pub fn blip_radius(size: BlipSize) -> f64 {
    match size {
        BlipSize::S => 3.0,
        BlipSize::M => 4.2,
        BlipSize::L => 5.4,
    }
}

fn fixed(n: f64) -> String {
    // Two decimals are more than a pixel can show and keep the path short and stable.
    let text = format!("{n:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// An arc along a circle of radius `r` (0 to 1, plus `outset` units) from `start` for `sweep`
/// radians, as an SVG path. A full turn is drawn as two half arcs (one arc cannot end where it
/// started).
pub fn arc_path(r: f64, outset: f64, start: f64, sweep: f64) -> String {
    let radius = RADIUS * r + outset;
    let (x0, y0) = (CENTRE + radius * start.sin(), CENTRE - radius * start.cos());
    if sweep >= std::f64::consts::TAU - 1e-6 {
        let (x1, y1) = (
            CENTRE + radius * (start + std::f64::consts::PI).sin(),
            CENTRE - radius * (start + std::f64::consts::PI).cos(),
        );
        return format!(
            "M{} {}A{} {} 0 1 1 {} {}A{} {} 0 1 1 {} {}",
            fixed(x0),
            fixed(y0),
            fixed(radius),
            fixed(radius),
            fixed(x1),
            fixed(y1),
            fixed(radius),
            fixed(radius),
            fixed(x0),
            fixed(y0)
        );
    }
    let end = start + sweep;
    let (x1, y1) = (CENTRE + radius * end.sin(), CENTRE - radius * end.cos());
    let large = u8::from(sweep > std::f64::consts::PI);
    format!(
        "M{} {}A{} {} 0 {large} 1 {} {}",
        fixed(x0),
        fixed(y0),
        fixed(radius),
        fixed(radius),
        fixed(x1),
        fixed(y1)
    )
}

/// A straight line from the centre to the rim at `theta`, as an SVG path (a sector divider).
pub fn spoke_path(theta: f64) -> String {
    let (x, y) = rim_point(theta, 0.0);
    format!(
        "M{} {}L{} {}",
        fixed(CENTRE),
        fixed(CENTRE),
        fixed(x),
        fixed(y)
    )
}

/// A small chevron pointing out from the rim at `theta`.
pub fn chevron_path(theta: f64) -> String {
    let tip = rim_point(theta, 7.0);
    let back = 3.2;
    let base = rim_point(theta, 3.2);
    // The two feet sit either side of the base, along the tangent.
    let (tx, ty) = (theta.cos(), theta.sin());
    let left = (base.0 - back * tx, base.1 - back * ty);
    let right = (base.0 + back * tx, base.1 + back * ty);
    format!(
        "M{} {}L{} {}L{} {}",
        fixed(left.0),
        fixed(left.1),
        fixed(tip.0),
        fixed(tip.1),
        fixed(right.0),
        fixed(right.1)
    )
}

/// Where a pointer at `(x, y)` pixels is in the drawing, for an element `width` x `height`
/// pixels wide that draws the square centred at the largest size that fits.
pub fn to_view(x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
    let scale = scale_of(width, height);
    if scale <= 0.0 {
        return (CENTRE, CENTRE);
    }
    let left = (width - VIEW * scale) / 2.0;
    let top = (height - VIEW * scale) / 2.0;
    ((x - left) / scale, (y - top) / scale)
}

/// Where a place in the drawing is in pixels inside an element `width` x `height` pixels (the
/// inverse of [`to_view`]).
pub fn from_view(x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
    let scale = scale_of(width, height);
    (
        (width - VIEW * scale) / 2.0 + x * scale,
        (height - VIEW * scale) / 2.0 + y * scale,
    )
}

/// Pixels per drawing unit for an element `width` x `height` pixels.
pub fn scale_of(width: f64, height: f64) -> f64 {
    width.min(height) / VIEW
}

/// What a pointer can point at: where it is and how big (all in drawing units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// The nearest target the pointer at `(x, y)` is on, allowing `tolerance_px` pixels of slack at
/// the given `scale` (pixels per unit). Ties go to the first.
pub fn hit_test(
    targets: &[Target],
    x: f64,
    y: f64,
    tolerance_px: f64,
    scale: f64,
) -> Option<usize> {
    if scale <= 0.0 {
        return None;
    }
    let slack = tolerance_px / scale;
    targets
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let d = ((t.x - x).powi(2) + (t.y - y).powi(2)).sqrt();
            (d <= t.radius + slack).then_some((i, d - t.radius))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Where the top-left of a tooltip goes: below and to the right of the pointer, flipped to the
/// other side when it would leave the window, and never off the left or top edge. All in pixels.
pub fn tooltip_origin(pointer: (f64, f64), tip: (f64, f64), bounds: (f64, f64)) -> (f64, f64) {
    const GAP: f64 = 12.0;
    let place = |p: f64, size: f64, limit: f64| {
        let after = p + GAP;
        let pos = if after + size <= limit {
            after
        } else {
            p - GAP - size
        };
        pos.clamp(2.0, (limit - size - 2.0).max(2.0))
    };
    (
        place(pointer.0, tip.0, bounds.0),
        place(pointer.1, tip.1, bounds.1),
    )
}

/// A rough size for a tooltip holding `text`, so it can be placed before it is drawn: at most
/// `max_width` wide, as many 11px lines as the text needs.
pub fn tooltip_size(text: &str, max_width: f64) -> (f64, f64) {
    const CHAR: f64 = 6.0;
    const LINE: f64 = 15.0;
    const PAD: f64 = 12.0;
    let natural = text.chars().count() as f64 * CHAR + PAD;
    let width = natural.min(max_width).max(40.0);
    let per_line = ((width - PAD) / CHAR).floor().max(1.0);
    let lines = (text.chars().count() as f64 / per_line).ceil().max(1.0);
    (width, lines * LINE + 8.0)
}

/// The next blip for `Tab` (`forward`) or `Shift+Tab` in angle order, wrapping round; the first
/// (or last) when nothing is focused.
pub fn step_focus(current: Option<usize>, len: usize, forward: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (current, forward) {
        (None, true) => 0,
        (None, false) => len - 1,
        (Some(i), true) => (i + 1) % len,
        (Some(i), false) => (i + len - 1) % len,
    })
}

/// A short word for a ring: "week", "2 wks", "qtr" for the defaults, else the working days.
pub fn ring_label(days: u32) -> String {
    match days {
        5 => "week".to_owned(),
        10 => "2 wks".to_owned(),
        60 => "qtr".to_owned(),
        n => format!("{n}d"),
    }
}

/// Where a ring sits (a radius from 0 to 1): the ends of the stretches core lays out, which
/// `minimap_core::radar::radius` uses.
pub const RING_RADII: [f64; 3] = [0.4, 0.65, 0.95];

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn twelve_oclock_is_straight_up_and_the_centre_is_you() {
        let (x, y) = point(1.0, 0.0);
        assert!(near(x, CENTRE) && near(y, CENTRE - RADIUS));
        let (x, y) = point(0.0, 1.234);
        assert!(near(x, CENTRE) && near(y, CENTRE));
        // Clockwise: a quarter turn is three o'clock, half a turn is six.
        let (x, y) = point(0.5, FRAC_PI_2);
        assert!(near(x, CENTRE + RADIUS / 2.0) && near(y, CENTRE));
        let (x, y) = point(1.0, PI);
        assert!(near(x, CENTRE) && near(y, CENTRE + RADIUS));
    }

    #[test]
    fn everything_inside_the_circle_stays_inside_the_drawing() {
        for i in 0..64 {
            let theta = TAU * f64::from(i) / 64.0;
            let (x, y) = point(1.0, theta);
            assert!((0.0..=VIEW).contains(&x) && (0.0..=VIEW).contains(&y));
            // Edge marks sit just beyond it and still fit.
            let (x, y) = rim_point(theta, 8.0);
            assert!((0.0..=VIEW).contains(&x) && (0.0..=VIEW).contains(&y));
        }
    }

    #[test]
    fn a_sector_arc_starts_and_ends_where_it_should() {
        // A quarter at the rim from 12 to 3 o'clock.
        assert_eq!(
            arc_path(1.0, 0.0, 0.0, FRAC_PI_2),
            "M100 10A90 90 0 0 1 190 100"
        );
        // More than half a turn takes the large arc.
        assert!(arc_path(1.0, 0.0, 0.0, PI * 1.5).contains("0 1 1"));
        // A full turn is two half arcs back to the start.
        let full = arc_path(1.0, 0.0, 0.0, TAU);
        assert_eq!(full.matches('A').count(), 2);
        assert!(full.starts_with("M100 10") && full.ends_with("100 10"));
    }

    #[test]
    fn a_divider_runs_from_the_centre_to_the_rim() {
        assert_eq!(spoke_path(0.0), "M100 100L100 10");
    }

    #[test]
    fn a_chevron_points_away_from_the_centre() {
        let path = chevron_path(0.0);
        assert!(path.starts_with('M') && path.matches('L').count() == 2);
        // At 12 o'clock the tip is the highest point.
        let tip = rim_point(0.0, 7.0);
        assert!(tip.1 < CENTRE - RADIUS);
    }

    #[test]
    fn a_pointer_is_converted_through_the_letterbox() {
        // Square element: the drawing fills it.
        assert_eq!(to_view(100.0, 100.0, 200.0, 200.0), (100.0, 100.0));
        assert_eq!(to_view(50.0, 25.0, 100.0, 100.0), (100.0, 50.0));
        // Wide element: the drawing is centred, so its middle is the element's middle.
        let (x, y) = to_view(300.0, 100.0, 600.0, 200.0);
        assert!(near(x, CENTRE) && near(y, CENTRE));
        // Left of the drawing is outside it.
        assert!(to_view(10.0, 100.0, 600.0, 200.0).0 < 0.0);
    }

    #[test]
    fn a_place_in_the_drawing_and_its_pixel_go_back_and_forth() {
        for (w, h) in [(200.0, 200.0), (600.0, 200.0), (260.0, 420.0)] {
            let (px, py) = from_view(37.0, 151.0, w, h);
            let (x, y) = to_view(px, py, w, h);
            assert!(near(x, 37.0) && near(y, 151.0), "{w}x{h}");
        }
    }

    #[test]
    fn pointing_takes_the_nearest_blip_within_six_pixels() {
        let targets = [
            Target {
                x: 50.0,
                y: 50.0,
                radius: 3.0,
            },
            Target {
                x: 56.0,
                y: 50.0,
                radius: 3.0,
            },
        ];
        // At a scale of 1 pixel per unit.
        assert_eq!(
            hit_test(&targets, 50.0, 50.0, HIT_TOLERANCE_PX, 1.0),
            Some(0)
        );
        assert_eq!(
            hit_test(&targets, 57.0, 50.0, HIT_TOLERANCE_PX, 1.0),
            Some(1)
        );
        // Between two: the closer edge wins.
        assert_eq!(
            hit_test(&targets, 54.0, 50.0, HIT_TOLERANCE_PX, 1.0),
            Some(1)
        );
        // Just outside the slack: nothing.
        assert_eq!(hit_test(&targets, 50.0, 60.0, HIT_TOLERANCE_PX, 1.0), None);
        assert_eq!(
            hit_test(&targets, 50.0, 58.9, HIT_TOLERANCE_PX, 1.0),
            Some(0)
        );
        // Zoomed in twice, the same 6 pixels are 3 units: the blip (3) and the slack (3) reach 6.
        assert_eq!(hit_test(&targets, 50.0, 56.5, HIT_TOLERANCE_PX, 2.0), None);
        assert_eq!(
            hit_test(&targets, 50.0, 55.5, HIT_TOLERANCE_PX, 2.0),
            Some(0)
        );
        assert_eq!(hit_test(&[], 0.0, 0.0, HIT_TOLERANCE_PX, 1.0), None);
        assert_eq!(hit_test(&targets, 50.0, 50.0, HIT_TOLERANCE_PX, 0.0), None);
    }

    #[test]
    fn a_tooltip_flips_before_it_leaves_the_window() {
        let bounds = (260.0, 260.0);
        let tip = (120.0, 30.0);
        // Room on the right and below: placed there.
        assert_eq!(tooltip_origin((20.0, 20.0), tip, bounds), (32.0, 32.0));
        // No room on the right: flipped to the left of the pointer.
        let (x, _) = tooltip_origin((200.0, 20.0), tip, bounds);
        assert!(x + tip.0 <= 200.0);
        // No room below: flipped above.
        let (_, y) = tooltip_origin((20.0, 250.0), tip, bounds);
        assert!(y + tip.1 <= 250.0);
        // Never outside, even when it does not fit at all.
        let (x, y) = tooltip_origin((5.0, 5.0), (400.0, 400.0), bounds);
        assert!(x >= 2.0 && y >= 2.0);
    }

    #[test]
    fn a_long_label_gets_more_lines() {
        let (w1, h1) = tooltip_size("Short", 220.0);
        let (w2, h2) = tooltip_size(&"word ".repeat(40), 220.0);
        assert!(w1 < w2 && w2 <= 220.0);
        assert!(h2 > h1);
    }

    #[test]
    fn tab_walks_the_blips_in_order_and_wraps() {
        assert_eq!(step_focus(None, 0, true), None);
        assert_eq!(step_focus(None, 3, true), Some(0));
        assert_eq!(step_focus(None, 3, false), Some(2));
        assert_eq!(step_focus(Some(2), 3, true), Some(0));
        assert_eq!(step_focus(Some(0), 3, false), Some(2));
        assert_eq!(step_focus(Some(1), 3, true), Some(2));
    }

    #[test]
    fn rings_are_labelled_in_words_for_the_defaults() {
        assert_eq!(ring_label(5), "week");
        assert_eq!(ring_label(10), "2 wks");
        assert_eq!(ring_label(60), "qtr");
        assert_eq!(ring_label(30), "30d");
    }

    #[test]
    fn blips_get_bigger_with_priority() {
        assert!(blip_radius(BlipSize::S) < blip_radius(BlipSize::M));
        assert!(blip_radius(BlipSize::M) < blip_radius(BlipSize::L));
    }
}
