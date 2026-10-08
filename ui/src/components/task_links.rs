//! The "Links" section of a task's panel: its children (what it waits for), its parents (what
//! waits for it) and which tasks are related, then its other links (the generic list), with the
//! ways to add one side by side: a new
//! linked task, an existing task, or something else. A task has no second Links list further down.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{EdgeLink, EdgeType, NodeRef, NodeType, TaskFilter, TaskStatus, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::BUTTON_SOFT,
        links_editor::{shown_links, LinksEditor},
        people_panel::error_line,
    },
    labels::{task_status_label, task_status_tone},
    state::{finish, DataVersion, LinkDialog, Selection, Toasts},
};

/// The links of a task sorted for display: `(children, parents, related tasks)`. A child is a task
/// this one waits for (it blocks this one); a parent is a task that waits for this one.
pub fn group_links(links: &[EdgeLink]) -> (Vec<&EdgeLink>, Vec<&EdgeLink>, Vec<&EdgeLink>) {
    let mut children = Vec::new();
    let mut parents = Vec::new();
    let mut related = Vec::new();
    for l in links {
        match (l.edge.edge_type, l.outgoing) {
            // Another task blocks this one: it is a child.
            (EdgeType::Blocks, false) => children.push(l),
            (EdgeType::Blocks, true) => parents.push(l),
            (EdgeType::RelatesTo, _) if l.other.node.node_type == NodeType::Task => related.push(l),
            _ => {}
        }
    }
    (children, parents, related)
}

#[component]
pub fn TaskLinks(task: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let dialog = expect_context::<LinkDialog>();
    let links = LocalResource::new(move || {
        version.track();
        api::list_edges_for(task)
    });
    // Status and title of every task, for the pills and the dialog's heading.
    let tasks = LocalResource::new(move || {
        version.track();
        api::list_tasks(TaskFilter {
            include_closed: true,
            ..Default::default()
        })
    });

    view! {
        <Section title="Links">
            {move || match (links.get(), tasks.get()) {
                (Some(Ok(links)), Some(Ok(rows))) => {
                    let label = rows
                        .iter()
                        .find(|r| r.task.id == task)
                        .map(|r| r.task.title.clone())
                        .unwrap_or_default();
                    let status_of = |id: Uuid| rows.iter().find(|r| r.task.id == id).map(|r| r.task.status);
                    let (children, parents, related) = group_links(&links);
                    let node = NodeRef::new(NodeType::Task, task);
                    let empty = children.is_empty()
                        && parents.is_empty()
                        && related.is_empty()
                        && shown_links(node, &links).is_empty();
                    let group = |title: &'static str, hint: &'static str, list: Vec<&EdgeLink>| {
                        (!list.is_empty()).then(|| {
                            let rows = list
                                .into_iter()
                                .map(|l| link_row(l, status_of(l.other.node.id)))
                                .collect_view();
                            view! {
                                <p class="mt-2 mb-1 text-[11px] text-muted" title=hint>{title}</p>
                                <ul class="space-y-1">{rows}</ul>
                            }
                        })
                    };
                    // The two ways to link another task sit next to the "something else" button
                    // of the generic list, so a task has one place for all its links.
                    let label_new = label.clone();
                    let actions = move || {
                        let (new_label, existing_label) = (label_new.clone(), label.clone());
                        view! {
                            <button class=BUTTON_SOFT on:click=move |_| dialog.open(task, new_label.clone(), false)>
                                "New linked task…"
                            </button>
                            <button class=BUTTON_SOFT on:click=move |_| dialog.open(task, existing_label.clone(), true)>
                                "Link an existing task…"
                            </button>
                        }
                    };
                    view! {
                        {empty.then(|| view! { <p class="mb-1 text-muted">"No links yet."</p> })}
                        {group("Parents", "This task is part of these. They wait until it is done", parents)}
                        {group("Children", "These are part of this task. It waits until they are done", children)}
                        {group("Related tasks", "Connected, with no order", related)}
                        <div class="mt-3">
                            <LinksEditor node=node links=links.clone() compact=true actions=actions />
                        </div>
                    }.into_any()
                }
                (Some(Err(e)), _) | (_, Some(Err(e))) => error_line(e),
                _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
    }
}

fn link_row(link: &EdgeLink, status: Option<TaskStatus>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let node = link.other.node;
    let edge_id = link.edge.id;
    let remove = move |_| {
        spawn_local(async move {
            finish(api::remove_edge(edge_id).await, toasts, version);
        });
    };
    let chip = status
        .map(|s| view! { <span class=task_status_tone(s).chip()>{task_status_label(s)}</span> });
    view! {
        <li class="flex items-center gap-2">
            <button class="min-w-0 flex-1 truncate text-left hover:underline"
                    on:click=move |_| selection.open(NodeRef::new(node.node_type, node.id))>
                {link.other.label.clone()}
            </button>
            {chip}
            <button class="px-1 text-faint hover:text-danger" aria-label="Remove link"
                    title="Remove this link" on:click=remove>"✕"</button>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{Edge, NodeSummary};
    use time::OffsetDateTime;

    fn link(kind: EdgeType, outgoing: bool, other_type: NodeType, n: u128) -> EdgeLink {
        EdgeLink {
            edge: Edge {
                id: Uuid::from_u128(n),
                edge_type: kind,
                from_type: NodeType::Task,
                from_id: Uuid::from_u128(1),
                to_type: other_type,
                to_id: Uuid::from_u128(n + 100),
                attrs: serde_json::json!({}),
                created_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            outgoing,
            other: NodeSummary {
                node: NodeRef::new(other_type, Uuid::from_u128(n + 100)),
                label: format!("T{n}"),
                archived: false,
            },
        }
    }

    #[test]
    fn links_are_sorted_into_children_parents_and_related_tasks_only() {
        let links = vec![
            link(EdgeType::Blocks, false, NodeType::Task, 1),
            link(EdgeType::Blocks, true, NodeType::Task, 2),
            link(EdgeType::RelatesTo, true, NodeType::Task, 3),
            link(EdgeType::RelatesTo, false, NodeType::Task, 4),
            // Relations to other kinds of item, and other relation types, stay in the generic list.
            link(EdgeType::RelatesTo, true, NodeType::Decision, 5),
            link(EdgeType::AssignedTo, true, NodeType::Person, 6),
        ];
        let (children, parents, related) = group_links(&links);
        let ids = |v: &[&EdgeLink]| v.iter().map(|l| l.edge.id.as_u128()).collect::<Vec<_>>();
        assert_eq!(ids(&children), [1]);
        assert_eq!(ids(&parents), [2]);
        assert_eq!(ids(&related), [3, 4]);
    }
}
