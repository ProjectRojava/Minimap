//! Positioning for pop-ups that are drawn above everything else (dropdown lists, calendars).

use leptos::web_sys;

/// Where the control that opened the pop-up is, in viewport pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub left: f64,
    pub top: f64,
    pub bottom: f64,
    pub width: f64,
}

pub fn anchor_of(el: &web_sys::Element) -> Anchor {
    let r = el.get_bounding_client_rect();
    Anchor {
        left: r.left(),
        top: r.top(),
        bottom: r.bottom(),
        width: r.width(),
    }
}

/// Viewport width and height in pixels.
pub fn viewport() -> (f64, f64) {
    let size = |v: Result<leptos::wasm_bindgen::JsValue, _>| v.ok().and_then(|v| v.as_f64());
    web_sys::window()
        .map(|w| {
            (
                size(w.inner_width()).unwrap_or(1024.0),
                size(w.inner_height()).unwrap_or(768.0),
            )
        })
        .unwrap_or((1024.0, 768.0))
}

const GAP: f64 = 2.0;
const MARGIN: f64 = 4.0;

/// Top-left corner for a `w` x `h` pop-up under the anchor: below if it fits (or if there is
/// more room below than above), otherwise above; shifted left so it stays inside the viewport.
pub fn place(a: &Anchor, w: f64, h: f64, viewport: (f64, f64)) -> (f64, f64) {
    let (vw, vh) = viewport;
    let room_below = vh - a.bottom;
    let room_above = a.top;
    let top = if room_below >= h + MARGIN || room_below >= room_above {
        a.bottom + GAP
    } else {
        (a.top - h - GAP).max(MARGIN)
    };
    let left = if a.left + w > vw - MARGIN {
        (vw - w - MARGIN).max(MARGIN)
    } else {
        a.left
    };
    (left, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(left: f64, top: f64) -> Anchor {
        Anchor {
            left,
            top,
            bottom: top + 24.0,
            width: 100.0,
        }
    }

    #[test]
    fn opens_below_when_it_fits() {
        assert_eq!(
            place(&anchor(10.0, 100.0), 140.0, 200.0, (800.0, 600.0)),
            (10.0, 126.0)
        );
    }

    #[test]
    fn flips_above_near_the_bottom() {
        let (left, top) = place(&anchor(10.0, 500.0), 140.0, 200.0, (800.0, 600.0));
        assert_eq!(left, 10.0);
        assert_eq!(top, 298.0); // 500 - 200 - 2
                                // Never above the top of the window.
        assert_eq!(
            place(&anchor(10.0, 20.0), 140.0, 400.0, (800.0, 300.0)).1,
            46.0
        );
    }

    #[test]
    fn stays_inside_the_right_edge() {
        assert_eq!(
            place(&anchor(700.0, 100.0), 232.0, 100.0, (800.0, 600.0)).0,
            564.0
        );
        assert_eq!(
            place(&anchor(5.0, 100.0), 900.0, 100.0, (800.0, 600.0)).0,
            4.0
        );
    }
}
