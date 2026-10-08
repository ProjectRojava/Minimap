//! The "⋯" menu on a Kanban card: open the task, add a sub-task to it, make a task linked to it,
//! link an existing task to it, or archive it.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{LinkRelation, NodeRef, NodeType, Uuid};

use crate::{
    api,
    components::overlay::{anchor_of, place, viewport, Anchor},
    state::{finish, CardMenuState, DataVersion, LinkDialog, MenuRequest, Selection, Toasts},
};

const WIDTH: f64 = 200.0;
const ROW_H: f64 = 28.0;

/// What the toast says after archiving: the name, and the way back.
pub fn archived_message(label: &str) -> String {
    format!("Archived \"{label}\". Press Ctrl/Cmd+Z to undo.")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Open,
    SubTask,
    LinkNew,
    LinkExisting,
    Archive,
}

const ITEMS: [(Item, &str); 5] = [
    (Item::Open, "Open"),
    (Item::SubTask, "Add a sub-task…"),
    (Item::LinkNew, "Link a new task…"),
    (Item::LinkExisting, "Link an existing task…"),
    (Item::Archive, "Archive"),
];

/// The ⋯ button on a card. It only asks for the menu to be shown (see [`CardMenuHost`]).
#[component]
pub fn CardMenu(task: Uuid, label: String) -> impl IntoView {
    let state = expect_context::<CardMenuState>();
    let button = leptos::prelude::NodeRef::<html::Button>::new();
    let label = StoredValue::new(label);
    let is_open = move || state.0.with(|m| m.as_ref().is_some_and(|m| m.task == task));

    let toggle = move |ev: ev::MouseEvent| {
        // The card behind opens its pane on click; the menu is not that.
        ev.stop_propagation();
        if is_open() {
            state.0.set(None);
            return;
        }
        let Some(btn) = button.get() else { return };
        let a = anchor_of(&btn);
        let h = ITEMS.len() as f64 * ROW_H + 8.0;
        // Aligned to the button's right edge.
        let aligned = Anchor {
            left: a.left + a.width - WIDTH,
            ..a
        };
        let (left, top) = place(&aligned, WIDTH, h, viewport());
        state.0.set(Some(MenuRequest {
            task,
            label: label.get_value(),
            left,
            top,
        }));
    };

    view! {
        <button type="button" node_ref=button draggable="false"
            class=move || format!(
                "absolute right-1 top-1 flex h-5 w-5 items-center justify-center rounded-sm \
                 text-[14px] leading-none text-muted hover:bg-hover hover:text-fg \
                 focus:opacity-100 group-hover:opacity-100 {}",
                if is_open() { "bg-hover text-fg opacity-100" } else { "opacity-0" })
            aria-label="Task actions" aria-haspopup="menu"
            aria-expanded=move || is_open().to_string()
            title="Task actions"
            on:mousedown=|ev| ev.stop_propagation()
            on:click=toggle>"⋯"</button>
    }
}

/// The open card menu, drawn once at the top level (not inside the card, whose own stacking
/// would let the cards below paint over it). Mount once, inside the router.
#[component]
pub fn CardMenuHost() -> impl IntoView {
    let state = expect_context::<CardMenuState>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let dialog = expect_context::<LinkDialog>();

    let choose = move |request: MenuRequest, item: Item| {
        state.0.set(None);
        let (task, name) = (request.task, request.label);
        match item {
            Item::Open => selection.open(NodeRef::new(NodeType::Task, task)),
            Item::SubTask => dialog.open_as(task, name, false, LinkRelation::Subtask),
            Item::LinkNew => dialog.open(task, name, false),
            Item::LinkExisting => dialog.open(task, name, true),
            Item::Archive => spawn_local(async move {
                if finish(api::archive_task(task).await, toasts, version).is_some() {
                    if selection.0.get_untracked() == Some(NodeRef::new(NodeType::Task, task)) {
                        selection.close();
                    }
                    toasts.info(archived_message(&name));
                }
            }),
        }
    };
    // Escape closes it.
    let handle = window_event_listener(ev::keydown, move |e: leptos::web_sys::KeyboardEvent| {
        if e.key() == "Escape" && state.0.get_untracked().is_some() {
            state.0.set(None);
        }
    });
    on_cleanup(move || handle.remove());

    view! {
        {move || state.0.get().map(|request| {
            let (left, top) = (request.left, request.top);
            let items = ITEMS.iter().map(|(item, text)| {
                let item = *item;
                let danger = item == Item::Archive;
                let request = request.clone();
                view! {
                    <li role="menuitem"
                        class=format!(
                            "flex h-7 cursor-default items-center px-3 hover:bg-hover {}",
                            if danger { "mt-1 border-t border-line text-danger" } else { "" })
                        on:mousedown=move |ev| { ev.prevent_default(); ev.stop_propagation(); choose(request.clone(), item); }>
                        {*text}
                    </li>
                }
            }).collect_view();
            view! {
                // Catches clicks outside (including on another card's ⋯, which then closes this).
                <div class="fixed inset-0 z-[70]"
                     on:mousedown=move |ev| { ev.stop_propagation(); state.0.set(None); }></div>
                <ul role="menu"
                    class="fixed z-[71] rounded-sm border border-line bg-panel py-1 text-[12px] text-fg"
                    style=format!("left:{left}px; top:{top}px; width:{WIDTH}px")>
                    {items}
                </ul>
            }
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archiving_says_how_to_get_it_back() {
        let m = archived_message("Fix login");
        assert!(m.contains("\"Fix login\"") && m.contains("Ctrl/Cmd+Z"));
    }

    #[test]
    fn the_menu_has_the_actions_in_order_with_archive_last() {
        let names: Vec<&str> = ITEMS.iter().map(|i| i.1).collect();
        assert_eq!(
            names,
            [
                "Open",
                "Add a sub-task…",
                "Link a new task…",
                "Link an existing task…",
                "Archive"
            ]
        );
    }
}
