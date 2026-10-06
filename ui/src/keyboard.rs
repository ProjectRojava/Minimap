//! Global keyboard shortcuts: `g` chords, `/` search, `?`/F1 help, Ctrl/Cmd+K palette, Ctrl/Cmd+Z undo, `j`/`k`/`Enter` on lists, `Esc` to close the pane.

use leptos::{ev, prelude::*, web_sys};
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;

use crate::{
    components::search_box::focus_search,
    nav::{
        chord_target, is_help_key, is_row_key, is_typing_target, undo_key, UndoKey, CHORD_WINDOW_MS,
    },
    state::{undo_or_redo, DataVersion, ListNav, PaletteOpen, Selection, Toasts},
};

/// Installs the window keydown handler. Must be called inside the `<Router>`.
pub fn use_global_shortcuts() {
    let navigate = use_navigate();
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let palette = expect_context::<PaletteOpen>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    // When `g` was pressed, in ms since the epoch (0 = no chord pending).
    let chord_started = StoredValue::new(0.0_f64);

    let handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        // Ctrl/Cmd+K opens the palette from anywhere, even from a text field.
        if (e.ctrl_key() || e.meta_key()) && !e.alt_key() && e.key().eq_ignore_ascii_case("k") {
            e.prevent_default();
            palette.toggle();
            return;
        }
        let (tag, editable) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            .map(|el| (el.tag_name(), el.is_content_editable()))
            .unwrap_or_default();
        let key = e.key();

        // F1 opens the help from anywhere, even a text box.
        if key == "F1" && is_help_key(&key) {
            e.prevent_default();
            navigate("/help", Default::default());
            return;
        }

        // Ctrl/Cmd+Z undoes the last change and Ctrl/Cmd+Shift+Z (or Ctrl+Y) redoes it, except
        // in a text field, where the field's own undo of what was typed stays.
        if let Some(which) = undo_key(&key, e.ctrl_key(), e.meta_key(), e.shift_key(), e.alt_key())
        {
            if !is_typing_target(&tag, editable) {
                e.prevent_default();
                undo_or_redo(which == UndoKey::Redo, toasts, version);
            }
            return;
        }
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return; // leave other Ctrl/Cmd combos (copy/paste) alone
        }

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
            "/" => {
                e.prevent_default();
                focus_search();
            }
            "?" if is_help_key(&key) => {
                e.prevent_default();
                navigate("/help", Default::default());
            }
            "Escape" if selection.0.get_untracked().is_some() => selection.close(),
            "n" => list.request_new(),
            k if is_row_key(k) && list.current().is_some() => list.send_row_key(k),
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
