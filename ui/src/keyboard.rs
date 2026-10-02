//! Global keyboard shortcuts: `g` chords, `j`/`k`/`Enter` on lists, `Esc` to close the pane.

use leptos::{ev, prelude::*, web_sys};
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;

use crate::{
    nav::{chord_target, is_typing_target, CHORD_WINDOW_MS},
    state::{ListNav, Selection},
};

/// Installs the window keydown handler. Must be called inside the `<Router>`.
pub fn use_global_shortcuts() {
    let navigate = use_navigate();
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    // When `g` was pressed, in ms since the epoch (0 = no chord pending).
    let chord_started = StoredValue::new(0.0_f64);

    let handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return; // leave Ctrl/Cmd combos (palette, copy/paste) alone
        }
        let (tag, editable) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            .map(|el| (el.tag_name(), el.is_content_editable()))
            .unwrap_or_default();
        let key = e.key();

        if is_typing_target(&tag, editable) {
            if key == "Escape" {
                if let Some(el) = e
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = el.blur();
                }
            }
            return;
        }

        let now = js_sys::Date::now();
        let chord_pending = now - chord_started.get_value() < CHORD_WINDOW_MS;
        chord_started.set_value(0.0);

        if chord_pending {
            if let Some(path) = chord_target(&key) {
                e.prevent_default();
                navigate(path, Default::default());
                return;
            }
        }

        match key.as_str() {
            "g" => chord_started.set_value(now),
            "Escape" if selection.0.get_untracked().is_some() => selection.close(),
            "j" => list.step(1),
            "k" => list.step(-1),
            // Let Enter activate a focused link or button instead.
            "Enter" if !matches!(tag.as_str(), "A" | "BUTTON") => {
                if let Some(node) = list.current() {
                    selection.open(node);
                }
            }
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());
}
