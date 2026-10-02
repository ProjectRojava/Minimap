use leptos::prelude::*;

/// Stand-in for screens that later feature specs build.
#[component]
pub fn Placeholder(title: &'static str, spec: &'static str) -> impl IntoView {
    view! {
        <div class="p-6">
            <h1 class="text-lg font-semibold tracking-tight">{title}</h1>
            <p class="mt-1 text-[13px] text-zinc-500">"Not built yet — see " <code class="font-mono">{spec}</code> "."</p>
        </div>
    }
}
