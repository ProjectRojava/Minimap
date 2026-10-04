//! Search box at the top of the sidebar: type to find any node, arrows + Enter (or a click)
//! to open it in the detail pane. `/` focuses it from anywhere.

use leptos::{ev, prelude::*, task::spawn_local, web_sys};
use minimap_types::{SearchFilter, SearchHit};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::form::COMPACT_INPUT,
    nav::{move_cursor, type_label},
    state::{Selection, Toasts},
};

pub const SEARCH_INPUT_ID: &str = "global-search";

/// Focuses and selects the search box (the `/` shortcut).
pub fn focus_search() {
    if let Some(el) = document()
        .get_element_by_id(SEARCH_INPUT_ID)
        .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
    {
        let _ = el.focus();
        el.select();
    }
}

#[component]
pub fn SearchBox() -> impl IntoView {
    let selection = expect_context::<Selection>();
    let toasts = expect_context::<Toasts>();

    let query = RwSignal::new(String::new());
    let hits = RwSignal::new(Vec::<SearchHit>::new());
    let cursor = RwSignal::new(Option::<usize>::None);
    let open = RwSignal::new(false);
    let archived = RwSignal::new(false);
    // Answers can arrive out of order; only the newest request may update the list.
    let latest = StoredValue::new(0_u64);

    let run = move || {
        let text = query.get_untracked();
        let ticket = latest.get_value() + 1;
        latest.set_value(ticket);
        if text.trim().is_empty() {
            hits.set(Vec::new());
            cursor.set(None);
            return;
        }
        let filter = SearchFilter {
            include_archived: archived.get_untracked(),
            ..Default::default()
        };
        spawn_local(async move {
            match api::search(text, filter).await {
                Ok(found) if latest.get_value() == ticket => {
                    cursor.set(move_cursor(None, found.len(), 0));
                    hits.set(found);
                }
                Ok(_) => {}
                Err(e) => toasts.error(&e),
            }
        });
    };
    let choose = move |hit: &SearchHit| {
        selection.open(hit.node);
        open.set(false);
        query.set(String::new());
        hits.set(Vec::new());
        cursor.set(None);
    };

    let on_input = move |ev: ev::Event| {
        query.set(event_target_value(&ev));
        open.set(true);
        run();
    };
    let on_keydown = move |ev: ev::KeyboardEvent| match ev.key().as_str() {
        "ArrowDown" | "ArrowUp" => {
            ev.prevent_default();
            let delta = if ev.key() == "ArrowDown" { 1 } else { -1 };
            open.set(true);
            cursor.update(|c| *c = move_cursor(*c, hits.with_untracked(Vec::len), delta));
        }
        "Enter" => {
            let picked = cursor
                .get_untracked()
                .and_then(|i| hits.with_untracked(|h| h.get(i).cloned()));
            if let Some(hit) = picked {
                ev.prevent_default();
                choose(&hit);
            }
        }
        "Escape" => {
            open.set(false);
            query.set(String::new());
            hits.set(Vec::new());
        }
        _ => {}
    };
    let toggle_archived = move |_| {
        archived.update(|a| *a = !*a);
        run();
    };

    let panel = move || {
        if !open.get() || query.with(|q| q.trim().is_empty()) {
            return None;
        }
        let rows = hits.get();
        let body = if rows.is_empty() {
            view! { <p class="px-3 py-2 text-muted">"No matches."</p> }.into_any()
        } else {
            rows.into_iter()
                .enumerate()
                .map(|(i, hit)| {
                    let picked = hit.clone();
                    let on_cursor = move || cursor.get() == Some(i);
                    let muted = if hit.archived { "text-muted" } else { "" };
                    view! {
                        <li role="option"
                            class=move || format!(
                                "px-3 py-1 cursor-default {}",
                                if on_cursor() { "bg-active" } else { "hover:bg-hover" })
                            on:mousedown=move |ev| { ev.prevent_default(); choose(&picked); }
                            on:mousemove=move |_| cursor.set(Some(i))>
                            <div class=format!("flex items-baseline gap-2 {muted}")>
                                <span class="w-16 shrink-0 text-[10px] uppercase tracking-wide text-muted">
                                    {type_label(hit.node.node_type)}
                                </span>
                                <span class="truncate font-medium">{hit.label.clone()}</span>
                                {hit.archived.then(|| view! { <span class="text-[11px] text-faint">"archived"</span> })}
                            </div>
                            {(!hit.snippet.is_empty() && hit.snippet != hit.label).then(|| view! {
                                <div class="ml-[4.5rem] truncate text-[11px] text-muted">{hit.snippet.clone()}</div>
                            })}
                        </li>
                    }
                })
                .collect_view()
                .into_any()
        };
        Some(view! {
            // Clicks inside must not blur the input (that would close the panel first).
            <div class="absolute left-2 top-full z-[60] mt-px w-[26rem] max-w-[80vw] \
                        rounded-sm border border-line bg-panel"
                 on:mousedown=move |ev| ev.prevent_default()>
                <ul role="listbox" class="max-h-[60vh] overflow-y-auto py-1">{body}</ul>
                <div class="flex items-center justify-between border-t border-line px-3 py-1 text-[11px] text-muted">
                    <span>"↑↓ move · Enter open · Esc close"</span>
                    <button class="hover:text-fg" on:click=toggle_archived>
                        {move || if archived.get() { "Archived: shown" } else { "Archived: hidden" }}
                    </button>
                </div>
            </div>
        })
    };

    view! {
        <div class="relative px-2 pt-2">
            <input id=SEARCH_INPUT_ID class=format!("{COMPACT_INPUT} w-full") type="search"
                   placeholder="Search  (/)" autocomplete="off" spellcheck="false"
                   aria-label="Search"
                   prop:value=move || query.get()
                   on:input=on_input on:keydown=on_keydown
                   on:focus=move |_| open.set(true)
                   on:blur=move |_| open.set(false) />
            {panel}
        </div>
    }
}
