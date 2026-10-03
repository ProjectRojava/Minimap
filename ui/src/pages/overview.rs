use leptos::prelude::*;

use crate::{api, state::Toasts};

/// Home screen. For now it only proves the UI <-> Rust bridge; feature 15 builds the real Overview.
#[component]
pub fn Overview() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let ping = LocalResource::new(api::ping);
    Effect::new(move |_| {
        if let Some(Err(e)) = ping.get() {
            toasts.error(&e);
        }
    });

    view! {
        <div class="p-6 space-y-3">
            <h1 class="text-[13px] font-semibold">"Overview"</h1>
            <p class="text-[13px] text-muted">
                "The portfolio overview arrives with " <code class="font-mono">"15-health-and-overview"</code> "."
            </p>
            <p class="font-mono text-[12px]">
                {move || match ping.get() {
                    None => "calling Rust…".to_owned(),
                    Some(Ok(p)) => format!("{} (schema v{})", p.message, p.schema_version),
                    Some(Err(_)) => "backend unavailable".to_owned(),
                }}
            </p>
            <p class="text-[12px] text-muted">
                "Shortcuts: " <kbd class="font-mono">"g"</kbd> " then a letter to jump (shown on hover in the sidebar), "
                <kbd class="font-mono">"j"</kbd> "/" <kbd class="font-mono">"k"</kbd> " move in lists, "
                <kbd class="font-mono">"Enter"</kbd> " opens, " <kbd class="font-mono">"Esc"</kbd> " closes."
            </p>
        </div>
    }
}
