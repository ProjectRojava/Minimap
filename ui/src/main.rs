mod api;

use leptos::prelude::*;

#[component]
fn App() -> impl IntoView {
    let ping = LocalResource::new(api::ping);

    view! {
        <main class="min-h-screen flex items-center justify-center">
            <div class="text-center space-y-2">
                <h1 class="text-2xl font-semibold tracking-tight">"Minimap"</h1>
                <Suspense fallback=|| view! { <p class="text-zinc-500">"Calling Rust…"</p> }>
                    {move || {
                        ping.get().map(|res| match res {
                            Ok(p) => view! {
                                <p class="font-mono text-emerald-600 dark:text-emerald-400">
                                    {p.message} " (schema v" {p.schema_version} ")"
                                </p>
                            }.into_any(),
                            Err(e) => view! {
                                <p class="font-mono text-red-600">{e}</p>
                            }.into_any(),
                        })
                    }}
                </Suspense>
            </div>
        </main>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
