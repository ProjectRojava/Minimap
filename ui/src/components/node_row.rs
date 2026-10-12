// No list screen uses this yet; features 03-06 do.
#![allow(dead_code)]

use leptos::prelude::*;
use minimap_types::NodeRef;

use crate::{
    components::fog::{fog_class, mark_for, FogChip},
    labels::heat_strength,
    state::{ListNav, Selection},
};

/// A clickable list row. `index` is its position in the list given to `ListNav::set_items`;
/// the row shows the keyboard cursor (`j`/`k`) and opens the detail pane on click or `Enter`.
/// A task, project or objective in play that has gone quiet (spec 43) is faded and ends in a
/// `≋ 19d` chip.
#[component]
pub fn NodeRow(
    node: NodeRef,
    index: usize,
    /// The hue of the objective the row serves: a coloured edge down its left side.
    #[prop(optional, into)]
    hue: Option<Signal<Option<u16>>>,
    /// How red the row's fill is, in percent (a task near its deadline).
    #[prop(default = None)]
    heat: Option<u8>,
    /// Something shown under the row's own line (This week's latest notes), part of the row.
    #[prop(optional, into)]
    below: ViewFn,
    children: Children,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let on_cursor = move || list.cursor.get() == Some(index);
    let is_open = move || selection.0.get() == Some(node);
    let fog = move || mark_for(node.id);

    view! {
        <div
            role="row"
            class=move || format!(
                "group relative border-b border-line cursor-default select-none {} {}",
                fog_class(fog().map(|m| m.level)),
                if is_open() { "bg-active shadow-[inset_2px_0_0_var(--color-accent)]" }
                else if on_cursor() { "bg-hover shadow-[inset_2px_0_0_var(--color-muted)]" }
                else if heat.is_some() { "heat hover:bg-hover" }
                else { "hover:bg-hover" })
            style=heat.map(|h| format!("--heat: {}", heat_strength(h)))
            on:click=move |_| {
                list.cursor.set(Some(index));
                selection.open(node);
            }
        >
            {hue.map(|hue| move || hue.get().map(|h| view! {
                <span class="obj-edge" style=format!("--obj-h: {h}") />
            }))}
            <div class="fog-content flex h-8 items-center gap-3 px-4">
                {children()}
                {move || fog().map(|mark| view! { <FogChip mark=mark /> })}
            </div>
            {below.run()}
        </div>
    }
}
