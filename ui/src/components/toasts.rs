use leptos::prelude::*;

use crate::state::{Toast, Toasts};

#[component]
pub fn ToastHost() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    view! {
        <div class="fixed bottom-3 left-1/2 -translate-x-1/2 z-50 flex flex-col gap-2 w-[min(28rem,calc(100vw-1.5rem))]"
             role="status" aria-live="polite">
            <For
                each=move || toasts.items.get()
                key=|t: &Toast| t.id
                children=move |t: Toast| {
                    let id = t.id;
                    let tone = if t.is_error {
                        "border-red-300 bg-red-50 text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100"
                    } else {
                        "border-zinc-200 bg-white text-zinc-900 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-100"
                    };
                    view! {
                        <div class=format!("flex items-start gap-3 rounded-md border px-3 py-2 text-sm shadow-lg {tone}")>
                            <div class="flex-1 min-w-0">
                                <p class="break-words">{t.message}</p>
                                {t.code.map(|c| view! { <p class="mt-0.5 font-mono text-[11px] opacity-60">{c}</p> })}
                            </div>
                            <button class="opacity-60 hover:opacity-100" aria-label="Dismiss"
                                    on:click=move |_| toasts.dismiss(id)>"✕"</button>
                        </div>
                    }
                }
            />
        </div>
    }
}
