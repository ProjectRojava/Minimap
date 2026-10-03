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
                        "border-l-2 border-l-danger"
                    } else {
                        ""
                    };
                    view! {
                        <div class=format!("flex items-start gap-3 rounded-sm border border-line bg-panel px-3 py-2 text-[13px] text-fg {tone}")>
                            <div class="flex-1 min-w-0">
                                <p class="break-words">{t.message}</p>
                                {t.code.map(|c| view! { <p class="mt-0.5 font-mono text-[11px] text-faint">{c}</p> })}
                            </div>
                            <button class="text-faint hover:text-fg" aria-label="Dismiss"
                                    on:click=move |_| toasts.dismiss(id)>"✕"</button>
                        </div>
                    }
                }
            />
        </div>
    }
}
