//! Custom title bar (the native one is disabled in `tauri.conf.json`): app name, a drag
//! region, and minimize / maximize / close. On macOS the native traffic lights overlay it.

use leptos::{ev, prelude::*, task::spawn_local};
use minimap_types::AppError;

use crate::{
    state::Toasts,
    theme::ThemeCtx,
    themes::Kind,
    window::{self, Platform, ResizeDir},
};

const CAPTION: &str = "inline-flex h-full w-11 items-center justify-center text-muted \
    hover:bg-hover hover:text-fg focus-visible:outline-none";
const CAPTION_CLOSE: &str = "inline-flex h-full w-11 items-center justify-center text-muted \
    hover:bg-danger hover:text-canvas focus-visible:outline-none";

/// The small mark for a theme of this kind.
pub fn mark_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Dark => "/minimap-mark-small.svg",
        Kind::Light => "/minimap-mark-small-light.svg",
    }
}

fn report(result: Result<(), AppError>, toasts: Toasts) {
    if let Err(e) = result {
        toasts.error(&e);
    }
}

#[component]
pub fn TitleBar() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let theme = expect_context::<ThemeCtx>();
    let platform = Platform::detect();
    // The small mark (no grid, no gradient: it has to read at 16px) has a dark and a light tile;
    // pick the one that sits well on the theme.
    let logo = move || mark_for(theme.current().kind);
    let maximized = RwSignal::new(false);

    let refresh = move || {
        spawn_local(async move {
            if let Ok(m) = window::is_maximized().await {
                maximized.set(m);
            }
        });
    };
    refresh();
    // Maximizing, restoring and snapping all resize the window.
    let handle = window_event_listener(ev::resize, move |_| refresh());
    on_cleanup(move || handle.remove());

    // To the task bar, or to the tray when Settings says so (the shell decides).
    let minimize =
        move |_| spawn_local(async move { report(crate::api::minimize_main().await, toasts) });
    let toggle = move |_| {
        spawn_local(async move { report(window::toggle_maximize().await, toasts) });
    };
    let close = move |_| spawn_local(async move { report(window::close().await, toasts) });

    // Leave room for the macOS traffic lights.
    let inset = if platform.draws_window_controls() {
        "padding-left: 0.75rem"
    } else {
        "padding-left: 5rem"
    };

    view! {
        <header data-tauri-drag-region class="flex h-8 shrink-0 select-none items-stretch border-b border-line bg-panel">
            <div data-tauri-drag-region class="flex flex-1 items-center gap-2" style=inset>
                <img src=logo alt="" class="pointer-events-none h-4 w-4" />
                <span class="pointer-events-none text-[12px] text-muted">"Minimap"</span>
            </div>
            <crate::components::radar::RadarButton />
            <crate::components::orientation::ViewToggle />
            <Show when=move || platform.draws_window_controls()>
                <button class=CAPTION aria-label="Minimize" title="Minimize" on:click=minimize>
                    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
                        <path d="M1 5h8" />
                    </svg>
                </button>
                <button class=CAPTION on:click=toggle
                        aria-label=move || if maximized.get() { "Restore" } else { "Maximize" }
                        title=move || if maximized.get() { "Restore" } else { "Maximize" }>
                    {move || if maximized.get() {
                        view! {
                            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
                                <path d="M2.5 2.5V1h6.5v6.5H7.5M1 2.5h6.5V9H1z" />
                            </svg>
                        }.into_any()
                    } else {
                        view! {
                            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
                                <path d="M1.5 1.5h7v7h-7z" />
                            </svg>
                        }.into_any()
                    }}
                </button>
                <button class=CAPTION_CLOSE aria-label="Close" title="Close" on:click=close>
                    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
                        <path d="M1 1l8 8M9 1L1 9" />
                    </svg>
                </button>
            </Show>
        </header>
        <ResizeHandles maximized=maximized />
    }
}

/// Invisible grab areas around the window edge. Linux gives undecorated windows no resize
/// borders; Windows does natively; macOS keeps its native frame.
#[component]
fn ResizeHandles(maximized: RwSignal<bool>) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let enabled = Platform::detect().needs_resize_handles();
    view! {
        <Show when=move || enabled && !maximized.get()>
            {ResizeDir::ALL.iter().map(|&dir| view! {
                <div class=format!("fixed z-[60] {}", dir.class())
                     on:mousedown=move |ev| {
                         ev.prevent_default();
                         spawn_local(async move { report(window::start_resize(dir).await, toasts) });
                     }></div>
            }).collect_view()}
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_bar_wears_the_small_mark_that_suits_the_theme() {
        assert_eq!(mark_for(Kind::Dark), "/minimap-mark-small.svg");
        assert_eq!(mark_for(Kind::Light), "/minimap-mark-small-light.svg");
        // Trunk only serves what index.html copies.
        let html = include_str!("../../index.html");
        for path in [mark_for(Kind::Dark), mark_for(Kind::Light)] {
            let file = path.trim_start_matches('/');
            assert!(
                html.contains(&format!("branding/{file}\"")),
                "{file} is not copied"
            );
        }
    }
}
