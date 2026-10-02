// No list screen uses this yet; features 03-06 do.
#![allow(dead_code)]

use leptos::prelude::*;
use minimap_types::NodeRef;

use crate::state::{ListNav, Selection};

/// A clickable list row. `index` is its position in the list given to `ListNav::set_items`;
/// the row shows the keyboard cursor (`j`/`k`) and opens the detail pane on click or `Enter`.
#[component]
pub fn NodeRow(node: NodeRef, index: usize, children: Children) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let on_cursor = move || list.cursor.get() == Some(index);
    let is_open = move || selection.0.get() == Some(node);

    view! {
        <div
            role="row"
            class=move || format!(
                "flex items-center gap-3 px-3 py-1 text-[13px] cursor-default border-b border-zinc-100 dark:border-zinc-800/60 {}",
                if is_open() { "bg-zinc-100 dark:bg-zinc-800" }
                else if on_cursor() { "bg-zinc-50 dark:bg-zinc-900 outline outline-1 -outline-offset-1 outline-zinc-300 dark:outline-zinc-600" }
                else { "hover:bg-zinc-50 dark:hover:bg-zinc-900" })
            on:click=move |_| {
                list.cursor.set(Some(index));
                selection.open(node);
            }
        >
            {children()}
        </div>
    }
}
