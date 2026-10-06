//! Applies the chosen colour theme at runtime and keeps it in sync with the OS ("System").
//!
//! The theme id is stored in the database (settings); a copy is cached in `localStorage` so
//! the next launch can apply it before the settings have loaded. Everything here is best
//! effort: if storage or `matchMedia` is unavailable the default (dark) theme is used.

use leptos::{prelude::*, web_sys};
use wasm_bindgen::{prelude::*, JsCast};

use crate::themes::{self, Kind, Theme};

const CACHE_KEY: &str = "minimap.theme";
const DARK_QUERY: &str = "(prefers-color-scheme: dark)";

fn cached_id() -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(CACHE_KEY)
        .ok()?
}

fn cache_id(id: &str) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(CACHE_KEY, id);
    }
}

fn os_prefers_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media(DARK_QUERY).ok().flatten())
        .is_none_or(|m| m.matches())
}

/// Writes the theme's colours onto `<html>` as custom properties.
fn apply(theme: &Theme) {
    let Some(root) = document().document_element() else {
        return;
    };
    let Ok(root) = root.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };
    let style = root.style();
    for (name, value) in theme.tokens.css_vars() {
        let _ = style.set_property(name, value);
    }
    let _ = style.set_property(
        "color-scheme",
        if theme.kind == Kind::Dark {
            "dark"
        } else {
            "light"
        },
    );
    let _ = root.set_attribute("data-theme", theme.id);
    // Objective colours pick their lightness from this (input.css).
    let _ = root.set_attribute(
        "data-kind",
        if theme.kind == Kind::Dark {
            "dark"
        } else {
            "light"
        },
    );
}

/// The selected theme id (or `system`), reactive. Provided as a context from `App`.
#[derive(Clone, Copy)]
pub struct ThemeCtx {
    id: RwSignal<String>,
    os_dark: RwSignal<bool>,
}

impl ThemeCtx {
    /// Starts from the cached choice (falling back to dark), applies it immediately, and
    /// follows OS light/dark changes for the `system` choice.
    pub fn new() -> Self {
        let ctx = Self {
            id: RwSignal::new(
                cached_id().unwrap_or_else(|| minimap_types::DEFAULT_THEME.to_owned()),
            ),
            os_dark: RwSignal::new(os_prefers_dark()),
        };
        apply(ctx.current());

        if let Some(query) =
            web_sys::window().and_then(|w| w.match_media(DARK_QUERY).ok().flatten())
        {
            let os_dark = ctx.os_dark;
            let on_change = Closure::<dyn FnMut(web_sys::MediaQueryListEvent)>::new(
                move |e: web_sys::MediaQueryListEvent| os_dark.set(e.matches()),
            );
            let _ = query
                .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
            on_change.forget(); // lives as long as the app
        }

        Effect::new(move |_| apply(ctx.current()));
        ctx
    }

    /// The theme being drawn (resolves `system`, falls back for unknown ids). Tracks signals.
    pub fn current(&self) -> &'static Theme {
        themes::resolve(&self.id.get(), self.os_dark.get())
    }

    /// The stored id: a theme id or `system`.
    pub fn selected(&self) -> String {
        self.id.get()
    }

    /// Switches theme now and remembers it for the next launch.
    pub fn select(&self, id: &str) {
        cache_id(id);
        self.id.set(id.to_owned());
    }
}
