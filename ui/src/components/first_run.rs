use leptos::{prelude::*, task::spawn_local};
use leptos_router::hooks::use_navigate;

use crate::{
    api,
    components::form::{BUTTON, BUTTON_PRIMARY, INPUT},
    settings_tab::Tab,
    state::{finish, DataVersion, Toasts},
};

/// Asks for the user's name the first time the app runs, then creates the "me" person.
///
/// A computer that joins data from another one must not make a "me" of its own: it would be a
/// second person, merged in later. So the way out is offered here: connect Google Drive first,
/// and this computer takes the "me" that is already there.
#[component]
pub fn FirstRun() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let navigate = use_navigate();
    let me = LocalResource::new(api::get_self_person);
    let created = RwSignal::new(false);
    // Said "I use Minimap on another computer": no "me" is made now; asked again at next start.
    let elsewhere = RwSignal::new(false);
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
    let connect_first = Callback::new(move |()| {
        elsewhere.set(true);
        navigate(&Tab::Data.path(), Default::default());
    });
    let needs_setup =
        move || matches!(me.get(), Some(Ok(None))) && !created.get() && !elsewhere.get();

    view! {
        <Show when=needs_setup>
            <div class="fixed inset-0 z-50 flex items-center justify-center bg-scrim">
                <form class="w-80 rounded-sm border border-line bg-panel p-5 space-y-3"
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <h2 class="text-base font-semibold">"Welcome to Minimap"</h2>
                    <p class="text-[13px] text-muted">
                        "What should we call you? You can change this later."
                    </p>
                    <p class="rounded-sm border border-warning/40 bg-warning/10 p-2 text-[12px]">
                        "Minimap keeps your data on this computer only. To back it up and use it on "
                        "other computers, connect Google Drive in Settings (you can do that any time)."
                    </p>
                    <input class=INPUT autofocus prop:value=move || name.get()
                           on:input=move |ev| name.set(event_target_value(&ev)) />
                    <button class=BUTTON_PRIMARY type="submit">"Continue"</button>
                    <div class="space-y-1 border-t border-line pt-3">
                        <p class="text-[12px] text-muted">
                            "Already use Minimap on another computer? Connect Google Drive first: "
                            "this computer then takes your data, and you, from there."
                        </p>
                        <button class=BUTTON type="button" on:click=move |_| connect_first.run(())>
                            "I use Minimap on another computer"
                        </button>
                    </div>
                </form>
            </div>
        </Show>
    }
}
