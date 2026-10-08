//! The "link a task" dialog: make a new task joined to this one, or join an existing task to it,
//! saying how they relate. Opened from a Kanban card's menu or a task's panel; a new task opens
//! in the detail pane when it is made.
//!
//! The dialog is centred with `inset-x-0 mx-auto`, not a transform: a transformed element becomes
//! the reference for `position: fixed` inside it, which would put the dropdown lists of its
//! `SelectField`s (placed in viewport pixels) in the wrong place.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{EdgeType, LinkRelation, NewEdge, NodeRef, NodeSummary, NodeType, Uuid};

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_ON, BUTTON_PRIMARY, INPUT},
        task_type::use_task_types,
    },
    state::{finish, DataVersion, LinkDialog, LinkRequest, Selection, Toasts},
};

/// The three ways two tasks can be joined, in the words the dialog uses: what the *other* task
/// is called, and what that means for the order of the work. A child is part of its parent, and a
/// parent waits until its children are done, so under the words it is the same `blocks` link.
pub const RELATIONS: [(LinkRelation, &str, &str); 3] = [
    (
        LinkRelation::BlockedBy,
        "Parent",
        "This task is part of it. It waits until this task is done",
    ),
    (
        LinkRelation::Blocks,
        "Child",
        "It is part of this task. This task waits until it is done",
    ),
    (
        LinkRelation::RelatesTo,
        "Related",
        "Connected, with no order between them",
    ),
];

/// One sentence that says what the choice does, naming this task: shown under the choices so it
/// is never unclear which task waits for which.
pub fn relation_summary(relation: LinkRelation, task: &str) -> String {
    let task = if task.trim().is_empty() {
        "this task"
    } else {
        task.trim()
    };
    match relation {
        LinkRelation::BlockedBy => format!("The other task waits until “{task}” is done."),
        LinkRelation::Blocks => format!("“{task}” waits until the other task is done."),
        LinkRelation::RelatesTo => "Neither task waits for the other.".to_owned(),
    }
}

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

/// The tasks the "Existing task" tab offers: every task but this one and the ones already linked
/// to it, whose title has every word typed in the search box. Title order.
pub fn pick_list(
    all: &[NodeSummary],
    source: Uuid,
    linked: &[Uuid],
    query: &str,
) -> Vec<(Uuid, String)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut list: Vec<(Uuid, String)> = all
        .iter()
        .filter(|n| n.node.id != source && !linked.contains(&n.node.id))
        .filter(|n| {
            let title = n.label.to_lowercase();
            words.iter().all(|w| title.contains(w))
        })
        .map(|n| (n.node.id, n.label.clone()))
        .collect();
    list.sort_by_key(|(_, label)| label.to_lowercase());
    list
}

/// `picked` with `id` added, or removed when it is already there.
pub fn toggled(picked: &[Uuid], id: Uuid) -> Vec<Uuid> {
    if picked.contains(&id) {
        picked.iter().copied().filter(|p| *p != id).collect()
    } else {
        let mut next = picked.to_vec();
        next.push(id);
        next
    }
}

/// The button of the "Existing task" tab.
pub fn link_button_text(chosen: usize) -> String {
    match chosen {
        0 | 1 => "Link".to_owned(),
        n => format!("Link {n} tasks"),
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
    let summary_label = request.label.clone();
    let existing = RwSignal::new(request.existing);
    let relation = RwSignal::new(LinkRelation::Blocks);
    let title = RwSignal::new(String::new());
    let picked = RwSignal::new(Vec::<Uuid>::new());
    let query = RwSignal::new(String::new());
    let task_type = RwSignal::new(String::new());
    let types = use_task_types();
    let busy = RwSignal::new(false);
    let input = leptos::prelude::NodeRef::<html::Input>::new();

    // The tasks that can be picked (all but this one).
    let tasks = LocalResource::new(move || api::list_node_summaries(NodeType::Task));
    // Tasks already linked to this one are not offered again.
    let current = LocalResource::new(move || api::list_edges_for(source));
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
            let chosen = picked.get_untracked();
            if chosen.is_empty() {
                return;
            }
            let edges: Vec<NewEdge> = chosen
                .iter()
                .map(|other| relation_edge(relation, source, *other))
                .collect();
            let count = edges.len();
            busy.set(true);
            spawn_local(async move {
                // All of them or none, as one step for undo.
                let result = api::add_edges(edges).await;
                busy.set(false);
                if finish(result, toasts, version).is_some() {
                    dialog.close();
                    toasts.info(if count == 1 {
                        "Linked".to_owned()
                    } else {
                        format!("Linked {count} tasks")
                    });
                }
            });
        } else {
            let text = title.get_untracked();
            if text.trim().is_empty() {
                return;
            }
            busy.set(true);
            spawn_local(async move {
                let kind = Some(task_type.get_untracked()).filter(|t| !t.is_empty());
                let result = api::create_linked_task(source, relation, text, kind).await;
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
            <p class="mb-1 text-[11px] text-muted">"The other task is a…"</p>
            <div class="mb-1 grid grid-cols-3 gap-2">{relation_buttons}</div>
            <p class="mb-3 text-[11px] text-muted">{move || relation_summary(relation.get(), &summary_label)}</p>
            {move || if existing.get() {
                view! { <TaskPicker source=source tasks=tasks current=current picked=picked query=query /> }.into_any()
            } else {
                view! {
                    <label class="block">
                        <span class="mb-0.5 block text-[11px] text-muted">"Title of the new task"</span>
                        <input node_ref=input class=INPUT type="text" autocomplete="off"
                            placeholder="What needs doing?"
                            prop:value=move || title.get()
                            on:input=move |ev| title.set(event_target_value(&ev)) />
                    </label>
                    <div class="mt-2">
                        {move || view! {
                            <SelectField label="Type" options=types.all_active_options()
                                current=task_type.get_untracked()
                                on_change=move |v: String| task_type.set(v) />
                        }}
                    </div>
                }.into_any()
            }}
            <div class="mt-4 flex items-center justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| dialog.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY
                    disabled=move || busy.get() || if existing.get() { picked.get().is_empty() } else { title.get().trim().is_empty() }
                    on:click=move |_| submit()>
                    {move || if existing.get() { link_button_text(picked.get().len()) } else { "Create and open".to_owned() }}
                </button>
            </div>
        </div>
    }
}

/// The "Existing task" tab: a search box over a list of tasks with a tick box each, so several
/// can be chosen and linked at once.
#[component]
fn TaskPicker(
    source: Uuid,
    tasks: LocalResource<Result<Vec<NodeSummary>, minimap_types::AppError>>,
    current: LocalResource<Result<Vec<minimap_types::EdgeLink>, minimap_types::AppError>>,
    picked: RwSignal<Vec<Uuid>>,
    query: RwSignal<String>,
) -> impl IntoView {
    let rows = move || {
        let (Some(Ok(all)), Some(Ok(links))) = (tasks.get(), current.get()) else {
            return None;
        };
        let linked: Vec<Uuid> = links
            .iter()
            .filter(|l| l.other.node.node_type == NodeType::Task)
            .map(|l| l.other.node.id)
            .collect();
        Some(pick_list(&all, source, &linked, &query.get()))
    };
    view! {
        <p class="mb-0.5 text-[11px] text-muted">"Tasks to link"</p>
        <input class=INPUT type="search" autocomplete="off" placeholder="Search tasks"
            prop:value=move || query.get()
            on:input=move |ev| query.set(event_target_value(&ev)) />
        <ul class="mt-1 max-h-56 overflow-y-auto rounded-sm border border-line" role="listbox"
            aria-multiselectable="true" aria-label="Tasks to link">
            {move || match rows() {
                None => view! { <li class="px-2 py-1.5 text-muted">"Loading…"</li> }.into_any(),
                Some(list) if list.is_empty() => view! {
                    <li class="px-2 py-1.5 text-muted">"No tasks to link."</li>
                }.into_any(),
                Some(list) => list.into_iter().map(|(id, label)| view! {
                    <li role="option" aria-selected=move || picked.get().contains(&id).to_string()>
                        <label class="flex cursor-pointer items-center gap-2 px-2 py-1 hover:bg-hover">
                            <input type="checkbox"
                                prop:checked=move || picked.get().contains(&id)
                                on:change=move |_| picked.update(|p| *p = toggled(p, id)) />
                            <span class="min-w-0 truncate">{label}</span>
                        </label>
                    </li>
                }).collect_view().into_any(),
            }}
        </ul>
        <p class="mt-1 text-[11px] text-muted">
            {move || match picked.get().len() {
                0 => "Tick one or more tasks.".to_owned(),
                1 => "1 task chosen.".to_owned(),
                n => format!("{n} tasks chosen."),
            }}
        </p>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Task, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    #[test]
    fn the_list_leaves_out_this_task_and_the_ones_already_linked_and_follows_the_search() {
        let all = vec![
            summary(1, "Ship it"),
            summary(2, "write docs"),
            summary(3, "Fix the build"),
            summary(4, "Docs review"),
        ];
        let ids = |q: &str| -> Vec<u128> {
            pick_list(&all, Uuid::from_u128(1), &[Uuid::from_u128(3)], q)
                .iter()
                .map(|(id, _)| id.as_u128())
                .collect()
        };
        // Title order, ignoring case; no 1 (this task) and no 3 (already linked).
        assert_eq!(ids(""), [4, 2]);
        assert_eq!(ids("DOCS"), [4, 2]);
        assert_eq!(ids("docs write"), [2]);
        assert!(ids("zzz").is_empty());
    }

    #[test]
    fn ticking_twice_unticks_and_the_order_is_kept() {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let picked = toggled(&toggled(&toggled(&[], a), b), c);
        assert_eq!(picked, [a, b, c]);
        assert_eq!(toggled(&picked, b), [a, c]);
    }

    #[test]
    fn the_button_counts_what_is_chosen() {
        assert_eq!(link_button_text(0), "Link");
        assert_eq!(link_button_text(1), "Link");
        assert_eq!(link_button_text(3), "Link 3 tasks");
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn the_task_that_comes_first_is_the_one_that_blocks() {
        let (source, other) = (id(1), id(2));
        // The other task is a child: it blocks this one (this waits for it).
        let e = relation_edge(LinkRelation::Blocks, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::Blocks, other, source)
        );
        // The other task is a parent: this task blocks it.
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
    fn the_summary_names_the_task_that_waits() {
        assert_eq!(
            relation_summary(LinkRelation::BlockedBy, "Ship it"),
            "The other task waits until “Ship it” is done."
        );
        assert_eq!(
            relation_summary(LinkRelation::Blocks, "Ship it"),
            "“Ship it” waits until the other task is done."
        );
        assert_eq!(
            relation_summary(LinkRelation::RelatesTo, "Ship it"),
            "Neither task waits for the other."
        );
        assert!(relation_summary(LinkRelation::Blocks, " ").starts_with("“this task”"));
    }

    #[test]
    fn parent_and_child_are_told_apart_by_the_order_of_the_work() {
        let name = |r: LinkRelation| RELATIONS.iter().find(|x| x.0 == r).unwrap().1;
        assert_eq!(name(LinkRelation::BlockedBy), "Parent");
        assert_eq!(name(LinkRelation::Blocks), "Child");
        assert_eq!(name(LinkRelation::RelatesTo), "Related");
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
