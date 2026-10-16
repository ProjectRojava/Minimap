//! The radar from the main window's side (spec 53, ADR-0021): the title-bar button, the commands
//! that open and close the window, and the listener for "open this item" that the radar sends
//! when you click a blip. The radar itself is `pages/radar.rs`.

use leptos::{prelude::*, task::spawn_local};
use leptos_router::hooks::use_navigate;
use minimap_types::NodeRef;

use crate::{
    api,
    components::page::Icon,
    nav::list_path,
    state::{Selection, Toasts},
    theme::ThemeCtx,
    themes::Kind,
};

/// The event the backend sends the main window to open an item (`commands::radar::OPEN_EVENT`).
const OPEN_EVENT: &str = "minimap://open";

/// The hover text of the title-bar button.
pub const BUTTON_TITLE: &str = "Open or close the radar (Ctrl/Cmd+Shift+M)";

fn is_dark(theme: ThemeCtx) -> bool {
    theme.current().kind == Kind::Dark
}

/// Opens the radar window, or brings it forward.
pub fn open(theme: ThemeCtx, toasts: Toasts) {
    let dark = is_dark(theme);
    spawn_local(async move {
        if let Err(e) = api::open_radar(dark).await {
            toasts.error(&e);
        }
    });
}

/// Closes the radar window.
pub fn close(toasts: Toasts) {
    spawn_local(async move {
        if let Err(e) = api::close_radar().await {
            toasts.error(&e);
        }
    });
}

/// Opens the radar when it is closed and closes it when it is open.
pub fn toggle(theme: ThemeCtx, toasts: Toasts) {
    let dark = is_dark(theme);
    spawn_local(async move {
        if let Err(e) = api::toggle_radar(dark).await {
            toasts.error(&e);
        }
    });
}

/// The title bar's radar button.
#[component]
pub fn RadarButton() -> impl IntoView {
    let theme = expect_context::<ThemeCtx>();
    let toasts = expect_context::<Toasts>();
    view! {
        <button class="mr-1 inline-flex h-6 items-center gap-1.5 self-center rounded-sm px-2 text-[12px] text-muted \
                       hover:bg-hover hover:text-fg"
                aria-label="Open or close the radar" title=BUTTON_TITLE
                on:click=move |_| toggle(theme, toasts)>
            <Icon name="radar" size="h-3.5 w-3.5" />
            <span>"Radar"</span>
        </button>
    }
}

/// Listens for the radar asking to open an item: opens it in the detail pane over its screen.
/// Call once, inside the router.
pub fn use_open_requests() {
    let navigate = use_navigate();
    let selection = expect_context::<Selection>();
    crate::window::listen::<NodeRef, _>(OPEN_EVENT, move |node| {
        selection.open(node);
        navigate(list_path(node.node_type), Default::default());
    });
}
