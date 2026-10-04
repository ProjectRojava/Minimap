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
                "group flex items-center gap-3 px-4 h-8 border-b border-line cursor-default select-none {}",
                if is_open() { "bg-active shadow-[inset_2px_0_0_var(--color-fg)]" }
                else if on_cursor() { "bg-hover shadow-[inset_2px_0_0_var(--color-muted)]" }
                else { "hover:bg-hover" })
            on:click=move |_| {
                list.cursor.set(Some(index));
                selection.open(node);
            }
        >
            {children()}
        </div>
    }
}
