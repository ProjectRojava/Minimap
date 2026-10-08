//! The "link a task" dialog: make a new task joined to this one, or join an existing task to it,
//! saying how they relate. Opened from a Kanban card's menu or a task's panel; a new task opens
//! in the detail pane when it is made.
//!
//! The dialog is centred with `inset-x-0 mx-auto`, not a transform: a transformed element becomes
//! the reference for `position: fixed` inside it, which would put the dropdown lists of its
//! `SelectField`s (placed in viewport pixels) in the wrong place.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{EdgeType, LinkRelation, NewEdge, NodeRef, NodeType, Uuid};

use crate::{
    api,
    components::form::{SelectField, BUTTON, BUTTON_ON, BUTTON_PRIMARY, INPUT},
    state::{finish, DataVersion, LinkDialog, LinkRequest, Selection, Toasts},
};

/// The three ways two tasks can be joined, in the words the dialog uses: what it is called, and
/// what it means for the order of the work.
pub const RELATIONS: [(LinkRelation, &str, &str); 3] = [
    (
        LinkRelation::Blocks,
        "Comes first",
        "It has to be done before this task can start",
    ),
    (
        LinkRelation::BlockedBy,
        "Comes after",
        "It waits until this task is done",
    ),
    (
        LinkRelation::RelatesTo,
        "Related",
        "Connected, with no order between them",
    ),
];

/// The link to add between `source` and an `other` task for `relation`: its type and the ends,
/// in the order they are stored (the task that blocks is `from`).
pub fn relation_edge(relation: LinkRelation, source: Uuid, other: Uuid) -> NewEdge {
    let (edge_type, from, to) = match relation {
        LinkRelation::Blocks => (EdgeType::Blocks, other, source),
        LinkRelation::BlockedBy => (EdgeType::Blocks, source, other),
        LinkRelation::RelatesTo => (EdgeType::RelatesTo, source, other),
    };
    NewEdge {
        edge_type,
        from: NodeRef::new(NodeType::Task, from),
        to: NodeRef::new(NodeType::Task, to),
        attrs: serde_json::json!({}),
    }
}

/// Mount once, inside the router.
#[component]
pub fn LinkDialogHost() -> impl IntoView {
    let dialog = expect_context::<LinkDialog>();
    view! {
        {move || dialog.0.get().map(|request| view! { <Dialog request=request /> })}
    }
}

#[component]
fn Dialog(request: LinkRequest) -> impl IntoView {
    let dialog = expect_context::<LinkDialog>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let source = request.task;
    let label = request.label.clone();
    let existing = RwSignal::new(request.existing);
    let relation = RwSignal::new(LinkRelation::Blocks);
    let title = RwSignal::new(String::new());
    let other = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let input = leptos::prelude::NodeRef::<html::Input>::new();

    // The tasks that can be picked (all but this one).
    let tasks = LocalResource::new(move || api::list_node_summaries(NodeType::Task));
    Effect::new(move |_| {
        if !existing.get() {
            if let Some(el) = input.get() {
                let _ = el.focus();
            }
        }
    });

    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let relation = relation.get_untracked();
        if existing.get_untracked() {
            let Ok(other) = Uuid::parse_str(&other.get_untracked()) else {
                return;
            };
            busy.set(true);
            spawn_local(async move {
                let result = api::add_edge(relation_edge(relation, source, other)).await;
                busy.set(false);
                if finish(result, toasts, version).is_some() {
                    dialog.close();
                    toasts.info("Linked");
                }
            });
        } else {
            let text = title.get_untracked();
            if text.trim().is_empty() {
                return;
            }
            busy.set(true);
            spawn_local(async move {
                let result = api::create_linked_task(source, relation, text).await;
                busy.set(false);
                if let Some(task) = finish(result, toasts, version) {
                    dialog.close();
                    selection.open(NodeRef::new(NodeType::Task, task.id));
                }
            });
        }
    };
    let on_keydown = move |e: ev::KeyboardEvent| match e.key().as_str() {
        "Escape" => {
            e.prevent_default();
            e.stop_propagation();
            dialog.close();
        }
        "Enter" if !existing.get_untracked() => {
            e.prevent_default();
            submit();
        }
        _ => {}
    };

    let relation_buttons = RELATIONS
        .iter()
        .map(|(value, name, hint)| {
            let value = *value;
            view! {
                <button type="button"
                    class=move || format!(
                        "flex flex-col items-start rounded-sm border border-line px-2 py-1 text-left \
                         hover:border-accent/50 {}",
                        if relation.get() == value { BUTTON_ON } else { "" })
                    aria-pressed=move || (relation.get() == value).to_string()
                    on:click=move |_| relation.set(value)>
                    <span class="text-[12px] font-medium">{*name}</span>
                    <span class="text-[11px] text-muted">{*hint}</span>
                </button>
            }
        })
        .collect_view();
    let tab = move |is_existing: bool, name: &'static str| {
        view! {
            <button type="button"
                class=move || format!("{BUTTON} {}", if existing.get() == is_existing { BUTTON_ON } else { "" })
                on:click=move |_| existing.set(is_existing)>{name}</button>
        }
    };

    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| dialog.close()></div>
        <div role="dialog" aria-label="Link a task"
             class="fixed inset-x-0 mx-auto top-[14vh] z-[81] w-[34rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-3 text-[14px] font-semibold">
                "Link a task to " <span class="text-accent">{label}</span>
            </h2>
            <div class="mb-3 flex gap-2">{tab(false, "New task")}{tab(true, "Existing task")}</div>
            <p class="mb-1 text-[11px] text-muted">"The other task…"</p>
            <div class="mb-3 grid grid-cols-3 gap-2">{relation_buttons}</div>
            {move || if existing.get() {
                let options: Vec<(String, String)> = std::iter::once((String::new(), "Choose a task…".to_owned()))
                    .chain(match tasks.get() {
                        Some(Ok(all)) => all
                            .into_iter()
                            .filter(|n| n.node.id != source)
                            .map(|n| (n.node.id.to_string(), n.label))
                            .collect::<Vec<_>>(),
                        _ => Vec::new(),
                    })
                    .collect();
                view! {
                    <SelectField label="Task" options=options current=other.get_untracked()
                        on_change=move |v: String| other.set(v) />
                }.into_any()
            } else {
                view! {
                    <label class="block">
                        <span class="mb-0.5 block text-[11px] text-muted">"Title of the new task"</span>
                        <input node_ref=input class=INPUT type="text" autocomplete="off"
                            placeholder="What needs doing?"
                            prop:value=move || title.get()
                            on:input=move |ev| title.set(event_target_value(&ev)) />
                    </label>
                }.into_any()
            }}
            <div class="mt-4 flex items-center justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| dialog.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY
                    disabled=move || busy.get() || if existing.get() { other.get().is_empty() } else { title.get().trim().is_empty() }
                    on:click=move |_| submit()>
                    {move || if existing.get() { "Link" } else { "Create and open" }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn the_task_that_comes_first_is_the_one_that_blocks() {
        let (source, other) = (id(1), id(2));
        // "Comes first": the other task blocks this one.
        let e = relation_edge(LinkRelation::Blocks, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::Blocks, other, source)
        );
        // "Comes after": this task blocks the other.
        let e = relation_edge(LinkRelation::BlockedBy, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::Blocks, source, other)
        );
        let e = relation_edge(LinkRelation::RelatesTo, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::RelatesTo, source, other)
        );
    }

    #[test]
    fn the_dialog_is_not_moved_by_a_transform_so_its_dropdowns_land_under_their_buttons() {
        // A transformed ancestor re-bases `position: fixed` (which `SelectField` lists use).
        let needle = ["-trans", "late-"].concat();
        let source = include_str!("link_dialog.rs");
        assert!(!source.contains(&needle));
    }

    #[test]
    fn every_relation_is_offered_once_with_words() {
        let mut seen: Vec<LinkRelation> = RELATIONS.iter().map(|r| r.0).collect();
        seen.dedup();
        assert_eq!(seen.len(), 3);
        assert!(RELATIONS
            .iter()
            .all(|(_, name, hint)| !name.is_empty() && !hint.is_empty()));
    }
}
