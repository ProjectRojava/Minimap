//! What the shell asks of the window: "keep running in the tray?" when the close button is
//! pressed and the answer isn't remembered, the quick-add box from the tray menu, and the unlock
//! screen when the database was locked while the window was hidden. Mounted once by `App`,
//! outside the unlock gate, so it also works before the database is open.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::CloseChoice;

use crate::{
    api,
    components::form::{BUTTON, BUTTON_PRIMARY},
    state::{PaletteOpen, Toasts},
};

/// The events the shell sends (`src-tauri/src/tray.rs`).
const CLOSE_ASKED_EVENT: &str = "minimap://close-asked";
const QUICK_ADD_EVENT: &str = "minimap://quick-add";
const LOCKED_EVENT: &str = "minimap://locked";

/// Listens for the shell's events. Call once from `App`.
pub fn use_shell_events() -> RwSignal<bool> {
    let asked = RwSignal::new(false);
    let palette = expect_context::<PaletteOpen>();
    crate::window::listen::<(), _>(CLOSE_ASKED_EVENT, move |()| asked.set(true));
    crate::window::listen::<(), _>(QUICK_ADD_EVENT, move |()| palette.0.set(true));
    // The shell locked the database: load the page again, which asks the database whether it is
    // locked and shows the unlock screen.
    crate::window::listen::<(), _>(LOCKED_EVENT, move |()| {
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    });
    asked
}

/// The box that asks what the close button should do.
#[component]
pub fn ClosePrompt(asked: RwSignal<bool>) -> impl IntoView {
    view! { {move || asked.get().then(|| view! { <Dialog asked=asked /> })} }
}

#[component]
fn Dialog(asked: RwSignal<bool>) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let remember = RwSignal::new(false);
    let keep = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if let Some(el) = keep.get() {
            let _ = el.focus();
        }
    });
    let answer = move |choice: CloseChoice| {
        let remember = remember.get_untracked();
        asked.set(false);
        spawn_local(async move {
            if let Err(e) = api::answer_close(choice, remember).await {
                toasts.error(&e);
            }
        });
    };
    let on_keydown = move |e: ev::KeyboardEvent| {
        // Nothing typed here reaches the shortcuts underneath.
        e.stop_propagation();
        if e.key() == "Escape" {
            e.prevent_default();
            asked.set(false);
        }
    };
    view! {
        <div class="fixed inset-0 z-[90] bg-scrim" on:mousedown=move |_| asked.set(false)></div>
        <div role="alertdialog" aria-label="Keep Minimap running in the tray?"
             class="fixed inset-x-0 mx-auto top-[18vh] z-[91] w-[28rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-2 text-[14px] font-semibold">"Keep Minimap running in the tray?"</h2>
            <p class="mb-3 text-muted">
                "In the tray, Minimap keeps syncing, reminds you of meetings and shows what needs you on its icon. Click the icon to open the window again, or choose Quit from its menu to close Minimap for good."
            </p>
            <label class="mb-3 flex items-center gap-2">
                <input type="checkbox" prop:checked=move || remember.get()
                    on:change=move |ev| remember.set(event_target_checked(&ev)) />
                <span>"Remember my choice (you can change it in Settings)"</span>
            </label>
            <div class="flex flex-wrap justify-end gap-2">
                <button type="button" class=BUTTON_PRIMARY node_ref=keep
                    on:click=move |_| answer(CloseChoice::Tray)>"Keep running in the tray"</button>
                <button type="button" class=BUTTON
                    on:click=move |_| answer(CloseChoice::Quit)>"Quit Minimap"</button>
                <button type="button" class=BUTTON on:click=move |_| asked.set(false)>"Cancel"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_events_are_the_ones_the_shell_sends() {
        let shell = include_str!("../../../src-tauri/src/tray.rs");
        for (name, value) in [
            ("CLOSE_ASKED_EVENT", CLOSE_ASKED_EVENT),
            ("QUICK_ADD_EVENT", QUICK_ADD_EVENT),
            ("LOCKED_EVENT", LOCKED_EVENT),
        ] {
            let line = format!("pub const {name}: &str = \"{value}\";");
            assert!(shell.contains(&line), "{name} differs from the shell's");
        }
    }
}
