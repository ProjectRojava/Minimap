use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_location};

use crate::{
    components::{page::Icon, search_box::SearchBox},
    nav::{grouped, pinned, NavItem},
    state::PaletteOpen,
    window::Platform,
};

/// One sidebar entry: icon, label, a quiet `g x` hint on hover, and a bar on the current one.
#[component]
fn NavLink(item: &'static NavItem, current: Memo<String>) -> impl IntoView {
    let path = item.path;
    let is_active = move || {
        let c = current.get();
        if path == "/" {
            c == "/"
        } else {
            c == path || c.starts_with(&format!("{path}/"))
        }
    };
    view! {
        <li>
            <A href=path
               attr:class=move || format!(
                   "group relative flex h-7 items-center gap-2.5 rounded-sm pl-3 pr-2 {}",
                   if is_active() {
                       "bg-active font-medium text-fg"
                   } else {
                       "text-muted hover:bg-hover hover:text-fg"
                   })
               attr:aria-current=move || is_active().then_some("page")>
                <span class=move || format!(
                          "absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-full {}",
                          if is_active() { "bg-accent" } else { "bg-transparent" })></span>
                <span class=move || if is_active() { "text-accent" } else { "" }>
                    <Icon name=item.icon />
                </span>
                <span class="flex-1 truncate">{item.label}</span>
                <kbd class="rounded-sm border border-line px-1 font-mono text-[10px] leading-4 text-faint \
                            opacity-0 group-hover:opacity-100">
                    {format!("g {}", item.chord)}
                </kbd>
            </A>
        </li>
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let location = use_location();
    let current = Memo::new(move |_| location.pathname.get());
    let palette = expect_context::<PaletteOpen>();
    let keys: [&str; 2] = if Platform::detect() == Platform::Mac {
        ["⌘", "K"]
    } else {
        ["Ctrl", "K"]
    };

    let sections = grouped()
        .into_iter()
        .map(|(heading, items)| {
            view! {
                <div>
                    {(!heading.is_empty()).then(|| view! {
                        <h2 class="px-3 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
                            {heading}
                        </h2>
                    })}
                    <ul class="space-y-px">
                        {items.into_iter().map(|item| view! { <NavLink item=item current=current /> }).collect_view()}
                    </ul>
                </div>
            }
        })
        .collect_view();
    let footer_links = pinned()
        .into_iter()
        .map(|item| view! { <NavLink item=item current=current /> })
        .collect_view();

    view! {
        <nav class="flex w-52 shrink-0 flex-col border-r border-line bg-panel" aria-label="Main">
            <div class="border-b border-line pb-2">
                <SearchBox />
            </div>
            <div class="flex-1 overflow-y-auto px-2 pb-2 pt-1">{sections}</div>
            <div class="space-y-px border-t border-line p-2">
                <ul class="space-y-px">{footer_links}</ul>
                <button class="flex h-7 w-full items-center gap-2.5 rounded-sm pl-3 pr-2 text-left \
                               text-muted hover:bg-hover hover:text-fg"
                        on:click=move |_| palette.toggle() title="Open the command palette">
                    <Icon name="command" />
                    <span class="flex-1 truncate">"Command palette"</span>
                    <span class="flex gap-0.5">
                        {keys.into_iter().map(|k| view! {
                            <kbd class="rounded-sm border border-line px-1 font-mono text-[10px] leading-4 text-faint">{k}</kbd>
                        }).collect_view()}
                    </span>
                </button>
            </div>
        </nav>
    }
}
