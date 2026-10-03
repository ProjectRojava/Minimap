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
        <nav class="w-52 shrink-0 flex flex-col border-r border-line bg-panel"
             aria-label="Main">
            <ul class="flex-1 overflow-y-auto px-2 py-2 space-y-px">
                {NAV.iter().filter(|n| n.enabled).map(|n| {
                    let path = n.path;
                    view! {
                        <li>
                            <A href=path
                               attr:class=move || format!(
                                   "group flex items-center justify-between rounded-sm px-2 h-7 {}",
                                   if is_active(path) {
                                       "bg-active text-fg"
                                   } else {
                                       "text-muted hover:bg-hover hover:text-fg"
                                   })>
                                <span>{n.label}</span>
                                <kbd class="font-mono text-[10px] text-faint opacity-0 group-hover:opacity-100">
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
