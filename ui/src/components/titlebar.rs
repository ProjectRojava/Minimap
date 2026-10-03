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
    // The logo has a dark and a light tile; pick the one that sits well on the theme.
    let logo = move || {
        if theme.current().kind == Kind::Dark {
            "/minimap-logo-dark.svg"
        } else {
            "/minimap-logo-light.svg"
        }
    };
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

    let minimize = move |_| spawn_local(async move { report(window::minimize().await, toasts) });
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
