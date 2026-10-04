//! The screen shown instead of the app while an encrypted database waits for its key (spec 21).

use leptos::{prelude::*, task::spawn_local, web_sys};
use minimap_types::Secret;

use crate::{
    api,
    components::{
        form::{BUTTON_PRIMARY, INPUT},
        page::Icon,
        titlebar::TitleBar,
    },
};

/// Why Enter does nothing yet, or `None` when the field can be submitted.
pub fn submit_problem(secret: &str) -> Option<&'static str> {
    secret
        .trim()
        .is_empty()
        .then_some("Type your passphrase or recovery key")
}

#[component]
pub fn UnlockScreen() -> impl IntoView {
    let secret = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);

    let submit = move || {
        let typed = secret.get_untracked();
        if let Some(problem) = submit_problem(&typed) {
            error.set(Some(problem.to_owned()));
            return;
        }
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::unlock_database(Secret(typed)).await {
                Ok(_) => {
                    // Every screen, the theme and the first-run check read the opened data.
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().reload();
                    }
                }
                Err(e) => {
                    secret.set(String::new());
                    busy.set(false);
                    error.set(Some(e.message));
                }
            }
        });
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && !busy.get_untracked() {
            submit();
        }
    };

    view! {
        <div class="flex h-screen flex-col overflow-hidden text-[13px]">
            <TitleBar />
            <div class="flex min-h-0 flex-1 items-center justify-center p-6">
                <div class="w-full max-w-sm space-y-4 rounded-sm border border-line bg-panel p-6">
                    <div class="flex items-center gap-3">
                        <span class="flex h-9 w-9 items-center justify-center rounded-sm border border-accent/30 bg-accent/10 text-accent">
                            <Icon name="lock" />
                        </span>
                        <div>
                            <h1 class="text-[15px] font-semibold">"Minimap is locked"</h1>
                            <p class="text-[12px] text-muted">"Your data is encrypted."</p>
                        </div>
                    </div>
                    <label class="block space-y-1">
                        <span class="text-[11px] text-muted">"Passphrase or recovery key"</span>
                        <input type="password" autofocus=true autocomplete="off" spellcheck="false"
                               class=INPUT prop:value=move || secret.get()
                               on:input=move |ev| secret.set(event_target_value(&ev))
                               on:keydown=on_key />
                    </label>
                    {move || error.get().map(|m| view! { <p class="text-danger" role="alert">{m}</p> })}
                    <button class=format!("{BUTTON_PRIMARY} w-full py-1.5") disabled=move || busy.get()
                            on:click=move |_| submit()>
                        {move || if busy.get() { "Unlocking…" } else { "Unlock" }}
                    </button>
                    <p class="text-[11px] text-muted">
                        "Use the passphrase you chose. If your key is kept in the system keychain and it "
                        "can't be reached, use the recovery key you saved when you turned encryption on."
                    </p>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_field_is_not_submitted() {
        assert!(submit_problem("").is_some());
        assert!(submit_problem("   ").is_some());
        assert!(submit_problem("a passphrase").is_none());
    }
}
