//! The link sections of a task's panel. Two ideas that are never mixed:
//!
//! - **Part of** (spec 33): the task's parent and its sub-tasks, with the progress of the
//!   sub-tasks. Organisation only; no date moves and nothing waits.
//! - **Links**: the order of the work (what blocks it, what it blocks), related tasks, then its
//!   other links (the generic list), with the ways to add one side by side: a new linked task, an
//!   existing task, or something else.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    EdgeLink, EdgeType, LinkRelation, NodeRef, NodeType, TaskFilter, TaskRow, TaskStatus, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::{Section, SECTION_ACTION},
        links_editor::{shown_links, LinksEditor},
        people_panel::error_line,
    },
    labels::{task_status_label, task_status_tone},
    state::{finish, DataVersion, LinkDialog, Selection, Toasts},
};

/// The task links of a task sorted for display.
#[derive(Default)]
pub struct TaskGroups<'a> {
    /// Tasks that must finish before this one can start (they block it).
    pub blocked_by: Vec<&'a EdgeLink>,
    /// Tasks that wait for this one (it blocks them).
    pub blocks: Vec<&'a EdgeLink>,
    /// Related tasks, no order.
    pub related: Vec<&'a EdgeLink>,
    /// The task this one is part of (normally at most one).
    pub parent: Vec<&'a EdgeLink>,
    /// The tasks that are part of this one.
    pub subtasks: Vec<&'a EdgeLink>,
}

pub fn group_links(links: &[EdgeLink]) -> TaskGroups<'_> {
    let mut groups = TaskGroups::default();
    for l in links {
        match (l.edge.edge_type, l.outgoing) {
            (EdgeType::Blocks, false) => groups.blocked_by.push(l),
            (EdgeType::Blocks, true) => groups.blocks.push(l),
            (EdgeType::RelatesTo, _) if l.other.node.node_type == NodeType::Task => {
                groups.related.push(l)
            }
            // This task is the child of the other.
            (EdgeType::SubtaskOf, true) => groups.parent.push(l),
            (EdgeType::SubtaskOf, false) => groups.subtasks.push(l),
            _ => {}
        }
    }
    groups
}

/// "2 of 5 done" for the sub-tasks heading; cancelled ones are not counted (the row's own count).
pub fn progress_text(done: u32, total: u32) -> String {
    format!("{done} of {total} done")
}

/// Which "Part of" buttons make sense: a task is part of one task and the tree is one level deep,
/// so a sub-task cannot have sub-tasks and a parent cannot be a sub-task.
/// `(can have sub-tasks added, can be made part of another task)`.
pub fn part_buttons(has_parent: bool, has_subtasks: bool) -> (bool, bool) {
    (!has_parent, !has_parent && !has_subtasks)
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
        {move || match (links.get(), tasks.get()) {
            (Some(Ok(links)), Some(Ok(rows))) => {
                let this: Option<&TaskRow> = rows.iter().find(|r| r.task.id == task);
                let label = this.map(|r| r.task.title.clone()).unwrap_or_default();
                let (done, total) = this.map(|r| (r.subtasks_done, r.subtask_count)).unwrap_or((0, 0));
                let status_of = |id: Uuid| rows.iter().find(|r| r.task.id == id).map(|r| r.task.status);
                let groups = group_links(&links);
                let (can_add_subtasks, can_join_parent) =
                    part_buttons(!groups.parent.is_empty(), !groups.subtasks.is_empty());
                let node = NodeRef::new(NodeType::Task, task);
                let part_empty = groups.parent.is_empty() && groups.subtasks.is_empty();
                let link_total = groups.blocked_by.len()
                    + groups.blocks.len()
                    + groups.related.len()
                    + shown_links(node, &links).len();
                let empty = groups.blocked_by.is_empty()
                    && groups.blocks.is_empty()
                    && groups.related.is_empty()
                    && shown_links(node, &links).is_empty();
                let group = |title: String, hint: &'static str, list: &[&EdgeLink]| {
                    (!list.is_empty()).then(|| {
                        let rows = list
                            .iter()
                            .map(|l| link_row(l, status_of(l.other.node.id)))
                            .collect_view();
                        view! {
                            <p class="mt-2 mb-1 text-[11px] text-muted" title=hint>{title}</p>
                            <ul class="space-y-1">{rows}</ul>
                        }
                    })
                };

                let open_part = move |name: String, existing: bool, relation: LinkRelation| {
                    dialog.open_as(task, name, existing, relation)
                };
                let part_label = label.clone();
                let part_buttons_view = move || {
                    let (a, b, c) = (part_label.clone(), part_label.clone(), part_label.clone());
                    view! {
                        {can_add_subtasks.then(|| view! {
                            <button class=SECTION_ACTION
                                    on:click=move |_| open_part(a.clone(), false, LinkRelation::Subtask)>
                                "New sub-task…"
                            </button>
                            <button class=SECTION_ACTION
                                    on:click=move |_| open_part(b.clone(), true, LinkRelation::Subtask)>
                                "Add existing…"
                            </button>
                        })}
                        {can_join_parent.then(|| view! {
                            <button class=SECTION_ACTION
                                    on:click=move |_| open_part(c.clone(), true, LinkRelation::Parent)>
                                "Make it part of…"
                            </button>
                        })}
                    }
                };

                // The two ways to link another task sit next to the "something else" button
                // of the generic list, so a task has one place for all its links.
                let label_new = label.clone();
                let actions = move || {
                    let (new_label, existing_label) = (label_new.clone(), label.clone());
                    view! {
                        <button class=SECTION_ACTION on:click=move |_| dialog.open(task, new_label.clone(), false)>
                            "New linked task…"
                        </button>
                        <button class=SECTION_ACTION on:click=move |_| dialog.open(task, existing_label.clone(), true)>
                            "Link an existing task…"
                        </button>
                    }
                };
                let subtasks_title = if total > 0 {
                    format!("Sub-tasks · {}", progress_text(done, total))
                } else {
                    "Sub-tasks".to_owned()
                };
                let parent_rows = group("Part of".to_owned(), "This task is a sub-task of it", &groups.parent);
                let subtask_rows = group(subtasks_title, "These are part of this task. They do not change its dates", &groups.subtasks);
                let blocked_by_rows = group("Blocked by".to_owned(), "These must be done before this task can start", &groups.blocked_by);
                let blocks_rows = group("Blocks".to_owned(), "These wait until this task is done", &groups.blocks);
                let related_rows = group("Related tasks".to_owned(), "Connected, with no order", &groups.related);
                let editor_links = links.clone();
                let part_meta = if total > 0 {
                    format!("· {}", progress_text(done, total))
                } else {
                    String::new()
                };
                let links_meta = if link_total > 0 {
                    format!("· {link_total}")
                } else {
                    String::new()
                };
                view! {
                    <Section title="Part of" meta=move || part_meta.clone() actions=part_buttons_view>
                        {part_empty.then(|| view! {
                            <p class="text-muted" title="Part of is for organising: it moves no date and holds nothing up.">"None yet."</p>
                        })}
                        {parent_rows}
                        {subtask_rows}
                    </Section>
                    <Section title="Links" meta=move || links_meta.clone() actions=actions>
                        {empty.then(|| view! { <p class="mb-1 text-muted">"No links yet."</p> })}
                        {blocked_by_rows}
                        {blocks_rows}
                        {related_rows}
                        <div class="mt-3">
                            <LinksEditor node=node links=editor_links compact=true />
                        </div>
                    </Section>
                }.into_any()
            }
            (Some(Err(e)), _) | (_, Some(Err(e))) => view! { <Section title="Links">{error_line(e)}</Section> }.into_any(),
            _ => view! { <Section title="Links"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
        }}
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
    fn links_are_sorted_by_what_they_mean_and_other_links_stay_in_the_generic_list() {
        let links = vec![
            link(EdgeType::Blocks, false, NodeType::Task, 1),
            link(EdgeType::Blocks, true, NodeType::Task, 2),
            link(EdgeType::RelatesTo, true, NodeType::Task, 3),
            link(EdgeType::RelatesTo, false, NodeType::Task, 4),
            // This task is the sub-task (outgoing) / the parent (incoming).
            link(EdgeType::SubtaskOf, true, NodeType::Task, 7),
            link(EdgeType::SubtaskOf, false, NodeType::Task, 8),
            link(EdgeType::SubtaskOf, false, NodeType::Task, 9),
            // Relations to other kinds of item, and other relation types, stay in the generic list.
            link(EdgeType::RelatesTo, true, NodeType::Decision, 5),
            link(EdgeType::AssignedTo, true, NodeType::Person, 6),
        ];
        let g = group_links(&links);
        let ids = |v: &[&EdgeLink]| v.iter().map(|l| l.edge.id.as_u128()).collect::<Vec<_>>();
        assert_eq!(ids(&g.blocked_by), [1]);
        assert_eq!(ids(&g.blocks), [2]);
        assert_eq!(ids(&g.related), [3, 4]);
        assert_eq!(ids(&g.parent), [7]);
        assert_eq!(ids(&g.subtasks), [8, 9]);
    }

    #[test]
    fn a_sub_task_cannot_get_sub_tasks_and_a_parent_cannot_be_made_part_of_another() {
        assert_eq!(part_buttons(false, false), (true, true));
        assert_eq!(part_buttons(true, false), (false, false));
        assert_eq!(part_buttons(false, true), (true, false));
    }

    #[test]
    fn progress_reads_done_of_all() {
        assert_eq!(progress_text(2, 5), "2 of 5 done");
    }
}
