use leptos::prelude::*;

/// Stand-in for screens that later feature specs build.
#[component]
pub fn Placeholder(title: &'static str, spec: &'static str) -> impl IntoView {
    view! {
        <div class="p-6">
            <h1 class="text-[13px] font-semibold">{title}</h1>
            <p class="mt-1 text-[13px] text-muted">"Not built yet — see " <code class="font-mono">{spec}</code> "."</p>
        </div>
    }
}
