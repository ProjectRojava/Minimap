//! The splash screen (docs/product-design/13-splash-screen.md): `index.html` paints it as plain
//! HTML and CSS the moment the window opens, while the app is still being downloaded and
//! started. Once the app has mounted this fades it out and removes it. It is in every window of
//! the app (the main window and the radar).

use std::time::Duration;

use leptos::{prelude::*, web_sys};
use wasm_bindgen::JsCast;

/// The id of the splash element in `index.html`.
pub const ID: &str = "splash";
/// The class that fades it out (`index.html` has the transition).
pub const OUT_CLASS: &str = "splash-out";
/// How long the fade takes, in milliseconds (the CSS transition is 220).
pub const FADE_MS: u64 = 260;

/// The main window's splash stays at least this long (milliseconds since the page started):
/// until the app is ready, or five seconds, whichever is later. A fast start shows the mark, the
/// version and who made it for the full five.
pub const MIN_SHOW_MS: f64 = 5000.0;

/// The radar is a window to glance at, so its splash only holds long enough not to flash.
pub const RADAR_MIN_SHOW_MS: f64 = 700.0;

/// How much longer to wait before fading, given how long the page has been open and the shortest
/// the splash may stay.
pub fn wait_before_fade(elapsed_ms: f64, min_show_ms: f64) -> u64 {
    (min_show_ms - elapsed_ms).max(0.0).round() as u64
}

/// Milliseconds since the page started (0 where there is no clock).
fn elapsed_ms() -> f64 {
    let global = js_sys::global();
    js_sys::Reflect::get(&global, &"performance".into())
        .ok()
        .and_then(|p| {
            let now: js_sys::Function = js_sys::Reflect::get(&p, &"now".into())
                .ok()?
                .dyn_into()
                .ok()?;
            now.call0(&p).ok()?.as_f64()
        })
        .unwrap_or(0.0)
}

/// Fades the splash out and removes it, once the app has mounted (this is called then) and the
/// splash has been up for [`MIN_SHOW_MS`] (the radar: [`RADAR_MIN_SHOW_MS`]).
pub fn dismiss() {
    let min = if crate::window::is_radar_window() {
        RADAR_MIN_SHOW_MS
    } else {
        MIN_SHOW_MS
    };
    set_timeout(
        fade_out,
        Duration::from_millis(wait_before_fade(elapsed_ms(), min)),
    );
}

/// Fades the splash out and removes it. Nothing happens when there is none (a test, a page
/// without it).
fn fade_out() {
    let Some(splash) = document().get_element_by_id(ID) else {
        return;
    };
    let _ = splash.class_list().add_1(OUT_CLASS);
    set_timeout(
        move || {
            let node: &web_sys::Element = &splash;
            node.remove();
        },
        Duration::from_millis(FADE_MS),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_has_the_splash_this_module_removes() {
        let html = include_str!("../index.html");
        assert!(html.contains(&format!("id=\"{ID}\"")));
        assert!(
            html.contains(&format!("#{ID}.{OUT_CLASS}")),
            "the fade-out rule"
        );
        // The fade lasts a little longer than the CSS transition, so nothing is cut short.
        assert!(html.contains("transition: opacity 220ms"));
        const { assert!(FADE_MS > 220) };
    }

    #[test]
    fn a_fast_start_still_shows_the_splash_for_five_seconds() {
        // The app was ready after 1.2 s: the splash waits for the rest of the five.
        assert_eq!(wait_before_fade(1200.0, MIN_SHOW_MS), 3800);
        assert_eq!(wait_before_fade(0.0, MIN_SHOW_MS), 5000);
        // A slow start (the app ready after 8 s, or a minute) leaves at once.
        assert_eq!(wait_before_fade(8000.0, MIN_SHOW_MS), 0);
        assert_eq!(wait_before_fade(60_000.0, MIN_SHOW_MS), 0);
        // The radar only avoids a flash.
        assert_eq!(wait_before_fade(250.4, RADAR_MIN_SHOW_MS), 450);
        assert_eq!(wait_before_fade(1000.0, RADAR_MIN_SHOW_MS), 0);
        // "Still starting" is for starts that are slower than the minimum.
        let html = include_str!("../index.html");
        assert!(html.contains("9s"), "the slow note appears after 9 s");
        const { assert!(MIN_SHOW_MS < 9000.0) };
    }

    #[test]
    fn the_splash_says_which_version_this_is_and_who_made_it() {
        // The page is plain HTML, so these are typed there; a release bump or a change of
        // address fails here until `ui/index.html` says the same as the About page.
        let html = include_str!("../index.html");
        assert!(html.contains(&format!("Version {}", crate::pages::about::VERSION)));
        assert!(html.contains(crate::pages::about::DEVELOPER));
        assert!(html.contains(crate::pages::about::EMAIL));
    }

    #[test]
    fn the_splash_needs_no_script_and_follows_the_security_policy() {
        let html = include_str!("../index.html");
        let start = html.find("id=\"splash\"").unwrap_or(0);
        let markup = &html[start..];
        // The content security policy allows no inline script, so the splash must not use any.
        assert!(!markup.contains("<script"));
        assert!(!markup.contains("onload=") && !markup.contains("onclick="));
        // Motion is switched off for people who ask for less of it.
        assert!(html.contains("prefers-reduced-motion: reduce"));
    }

    #[test]
    fn the_words_follow_the_voice() {
        let html = include_str!("../index.html");
        assert!(html.contains("Starting up"));
        assert!(html.contains("Still starting. This can take a moment after an update."));
    }
}
