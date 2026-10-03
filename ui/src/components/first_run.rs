use leptos::{prelude::*, task::spawn_local};

use crate::{
    api,
    components::form::{BUTTON_PRIMARY, INPUT},
    state::{finish, DataVersion, Toasts},
};

/// Asks for the user's name the first time the app runs, then creates the "me" person.
#[component]
pub fn FirstRun() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let me = LocalResource::new(api::get_self_person);
    let created = RwSignal::new(false);
    let name = RwSignal::new("Me".to_owned());

    Effect::new(move |_| {
        if let Some(Err(e)) = me.get() {
            toasts.error(&e);
        }
    });

    let submit = move || {
        let n = name.get_untracked();
        spawn_local(async move {
            if finish(api::create_self_person(n).await, toasts, version).is_some() {
                created.set(true);
            }
        });
    };
    let needs_setup = move || matches!(me.get(), Some(Ok(None))) && !created.get();

    view! {
        <Show when=needs_setup>
            <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
                <form class="w-80 rounded-lg bg-white dark:bg-zinc-900 p-5 shadow-2xl space-y-3"
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <h2 class="text-base font-semibold">"Welcome to Minimap"</h2>
                    <p class="text-[13px] text-zinc-500">
                        "What should we call you? You can change this later."
                    </p>
                    <input class=INPUT autofocus prop:value=move || name.get()
                           on:input=move |ev| name.set(event_target_value(&ev)) />
                    <button class=BUTTON_PRIMARY type="submit">"Continue"</button>
                </form>
            </div>
        </Show>
    }
}
