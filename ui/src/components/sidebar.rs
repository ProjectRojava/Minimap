use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_location};

use crate::nav::NAV;

#[component]
pub fn Sidebar() -> impl IntoView {
    let location = use_location();
    let is_active = move |path: &'static str| {
        let current = location.pathname.get();
        if path == "/" {
            current == "/"
        } else {
            current == path || current.starts_with(&format!("{path}/"))
        }
    };

    view! {
        <nav class="w-52 shrink-0 flex flex-col border-r border-zinc-200 dark:border-zinc-800 bg-zinc-50 dark:bg-zinc-900/40"
             aria-label="Main">
            <div class="flex items-center gap-2 px-3 h-12">
                <img src="/minimap-logo-auto.svg" alt="" class="h-6 w-6" />
                <span class="font-semibold tracking-tight">"Minimap"</span>
            </div>
            <ul class="flex-1 overflow-y-auto px-2 pb-2 space-y-px">
                {NAV.iter().filter(|n| n.enabled).map(|n| {
                    let path = n.path;
                    view! {
                        <li>
                            <A href=path
                               attr:class=move || format!(
                                   "group flex items-center justify-between rounded px-2 py-1 text-[13px] {}",
                                   if is_active(path) {
                                       "bg-zinc-200/70 dark:bg-zinc-800 font-medium"
                                   } else {
                                       "text-zinc-600 dark:text-zinc-400 hover:bg-zinc-200/50 dark:hover:bg-zinc-800/60"
                                   })>
                                <span>{n.label}</span>
                                <kbd class="font-mono text-[10px] text-zinc-400 opacity-0 group-hover:opacity-100">
                                    {format!("g {}", n.chord)}
                                </kbd>
                            </A>
                        </li>
                    }
                }).collect_view()}
            </ul>
        </nav>
    }
}
