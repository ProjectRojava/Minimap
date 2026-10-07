//! Subtasks in a task's panel (spec 29): the parent it belongs to, its progress, its subtasks
//! (tick one done, open it, take it out), a box to make a new one and a picker to link a task
//! that already exists. A subtask is a full task; this only says which tasks belong together.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    EdgeType, NewEdge, NodeRef, NodeType, StatusHint, Subtask, TaskStatus, UpdateTask, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{SelectField, BUTTON_SOFT, BUTTON_SUCCESS, INPUT},
        page::Tone,
        people_panel::error_line,
        task_board::today,
    },
    labels::{date_tone, task_status_label, task_status_tone},
    state::{finish, DataVersion, Selection, Toasts},
};

/// "2 of 5 done" (cancelled subtasks are left out of both numbers), empty when there are none
/// to count.
fn progress_text(subtasks: &[Subtask]) -> String {
    let (done, total) = progress(subtasks);
    if total == 0 {
        String::new()
    } else {
        format!("{done} of {total} done")
    }
}

/// `(done, total)` over the subtasks that are not cancelled.
fn progress(subtasks: &[Subtask]) -> (usize, usize) {
    let live = subtasks
        .iter()
        .filter(|s| s.status != TaskStatus::Cancelled);
    (
        live.clone()
            .filter(|s| s.status == TaskStatus::Done)
            .count(),
        live.count(),
    )
}

/// The width of the progress bar, in percent.
fn percent(done: usize, total: usize) -> usize {
    (done * 100).checked_div(total).unwrap_or(0)
}

#[component]
pub fn Subtasks(task: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let detail = LocalResource::new(move || {
        version.track();
        api::get_task_detail(task)
    });
    let candidates = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Task)
    });

    view! {
        <Section title="Subtasks">
            {move || match (detail.get(), candidates.get()) {
                (Some(Ok(d)), Some(Ok(all))) => {
                    let linked: Vec<Uuid> = d.subtasks.iter().map(|s| s.node.node.id).collect();
                    let parent = d.parent.clone();
                    // Anything but itself, its parent and what is already below it; a loop is
                    // refused by the backend with the path.
                    let options: Vec<(String, String)> = std::iter::once((String::new(), "Link an existing task…".to_owned()))
                        .chain(all.iter()
                            .filter(|n| n.node.id != task && !linked.contains(&n.node.id))
                            .map(|n| (n.node.id.to_string(), n.label.clone())))
                        .collect();
                    let parent_options: Vec<(String, String)> = std::iter::once((String::new(), "Make this a subtask of…".to_owned()))
                        .chain(all.iter()
                            .filter(|n| n.node.id != task && !linked.contains(&n.node.id))
                            .map(|n| (n.node.id.to_string(), n.label.clone())))
                        .collect();
                    view! {
                        <ParentLine task=task parent=parent options=parent_options />
                        <SubtaskList task=task subtasks=d.subtasks.clone() hint=d.status_hint.clone() />
                        <AddSubtask task=task options=options />
                    }.into_any()
                }
                (Some(Err(e)), _) | (_, Some(Err(e))) => error_line(e),
                _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
    }
}

/// "Subtask of X" with a way out, or the picker to make this a subtask of something.
#[component]
fn ParentLine(
    task: Uuid,
    parent: Option<minimap_types::NodeSummary>,
    options: Vec<(String, String)>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let set = move |parent: Option<Uuid>| {
        spawn_local(async move {
            finish(api::set_parent(task, parent).await, toasts, version);
        });
    };
    match parent {
        Some(p) => {
            let node = p.node;
            view! {
                <p class="mb-2 flex items-center gap-2">
                    <span class="text-muted">"Subtask of"</span>
                    <button class="min-w-0 flex-1 truncate text-left font-medium hover:underline"
                            on:click=move |_| selection.open(node)>{p.label}</button>
                    <button class="px-1 text-faint hover:text-danger" aria-label="Make this a regular task"
                            title="Make this a regular task" on:click=move |_| set(None)>"✕"</button>
                </p>
            }
            .into_any()
        }
        None => view! {
            <div class="mb-2">
                <SelectField compact=true action=true options=options current=String::new()
                    on_change=move |v: String| if let Ok(p) = Uuid::parse_str(&v) { set(Some(p)) } />
            </div>
        }
        .into_any(),
    }
}

/// "All subtasks are done. [Mark done]": a suggestion for the group's own status, taken only
/// when asked.
#[component]
fn StatusHintLine(task: Uuid, hint: StatusHint) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let status = hint.status;
    let apply = move |_| {
        spawn_local(async move {
            let patch = UpdateTask {
                status: Some(status),
                ..Default::default()
            };
            finish(api::update_task(task, patch).await, toasts, version);
        });
    };
    let class = match status {
        TaskStatus::Done => BUTTON_SUCCESS,
        _ => BUTTON_SOFT,
    };
    view! {
        <p class="mb-2 flex flex-wrap items-center gap-2 rounded-sm border border-line bg-canvas px-2 py-1">
            <span class="text-muted">{hint.text}</span>
            <button class=class on:click=apply>{format!("Set {}", task_status_label(status).to_lowercase())}</button>
        </p>
    }
}

#[component]
fn SubtaskList(task: Uuid, subtasks: Vec<Subtask>, hint: Option<StatusHint>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    if subtasks.is_empty() {
        return view! { <p class="mb-2 text-muted">"No subtasks yet."</p> }.into_any();
    }
    let (done, total) = progress(&subtasks);
    let bar = percent(done, total);
    let text = progress_text(&subtasks);
    let today = today();
    // Numbers only mean something once some steps are put in order.
    let ordered = subtasks.iter().any(|s| !s.after.is_empty());
    let siblings: Vec<(Uuid, String)> = subtasks
        .iter()
        .map(|s| (s.node.node.id, s.node.label.clone()))
        .collect();
    let many = subtasks.len() > 1;
    let rows = subtasks.into_iter().enumerate().map(|(i, s)| {
        let id = s.node.node.id;
        let node = s.node.node;
        let is_done = s.status == TaskStatus::Done;
        let open = !matches!(s.status, TaskStatus::Done | TaskStatus::Cancelled);
        let toggle = move |_| {
            let status = if is_done { TaskStatus::Todo } else { TaskStatus::Done };
            spawn_local(async move {
                let patch = UpdateTask { status: Some(status), ..Default::default() };
                finish(api::update_task(id, patch).await, toasts, version);
            });
        };
        let unlink = move |_| {
            spawn_local(async move {
                finish(api::set_parent(id, None).await, toasts, version);
            });
        };
        let due = s.due_date.map(|d| {
            let tone = date_tone(d, today, open);
            view! { <span class=format!("shrink-0 text-[11px] tabular-nums {}", tone.text())>{d.to_string()}</span> }
        });
        let title_class = if is_done || s.status == TaskStatus::Cancelled {
            "min-w-0 flex-1 truncate text-left text-muted line-through hover:underline"
        } else {
            "min-w-0 flex-1 truncate text-left hover:underline"
        };
        let marker = if s.next {
            Some(view! { <span class=Tone::Accent.chip() title="The next step: open and not waiting for anything">"next"</span> }.into_any())
        } else if s.waiting && open {
            Some(view! { <span class=Tone::Neutral.chip() title="Waiting for work that isn't finished yet">"waiting"</span> }.into_any())
        } else {
            None
        };
        let step = ordered.then(|| view! {
            <span class="w-4 shrink-0 text-right text-[11px] tabular-nums text-muted">{format!("{}.", i + 1)}</span>
        });
        let after = s.after.iter().map(|a| {
            let edge_id = a.edge_id;
            let label = a.node.label.clone();
            view! {
                <span class="inline-flex items-center gap-1 rounded-sm border border-line px-1 text-[11px] text-muted">
                    "after " {label}
                    <button class="text-faint hover:text-danger" aria-label="Remove this order"
                            on:click=move |_| {
                                spawn_local(async move {
                                    finish(api::remove_edge(edge_id).await, toasts, version);
                                });
                            }>"✕"</button>
                </span>
            }
        }).collect_view();
        // Put it after another subtask (a `blocks` link between the two).
        let already: Vec<Uuid> = s.after.iter().map(|a| a.node.node.id).collect();
        let options: Vec<(String, String)> = std::iter::once((String::new(), "Do after…".to_owned()))
            .chain(siblings.iter()
                .filter(|(other, _)| *other != id && !already.contains(other))
                .map(|(other, label)| (other.to_string(), label.clone())))
            .collect();
        let put_after = move |v: String| {
            if let Ok(before) = Uuid::parse_str(&v) {
                let new = NewEdge {
                    edge_type: EdgeType::Blocks,
                    from: NodeRef::new(NodeType::Task, before),
                    to: NodeRef::new(NodeType::Task, id),
                    attrs: serde_json::json!({}),
                };
                spawn_local(async move {
                    finish(api::add_edge(new).await, toasts, version);
                });
            }
        };
        let row_class = if s.next {
            "rounded-sm border-l-2 border-accent bg-accent/5 pl-1.5"
        } else {
            "pl-2"
        };
        view! {
            <li class=row_class>
                <div class="flex items-center gap-2">
                    {step}
                    <input type="checkbox" class="shrink-0" prop:checked=is_done
                           aria-label="Done" title="Mark done / reopen" on:change=toggle />
                    <button class=title_class on:click=move |_| selection.open(node)>{s.node.label}</button>
                    {marker}
                    {due}
                    <span class=task_status_tone(s.status).chip()>{task_status_label(s.status)}</span>
                    <button class="px-1 text-faint hover:text-danger" aria-label="Take out of this task"
                            title="Make it a regular task" on:click=unlink>"✕"</button>
                </div>
                {(many && open).then(|| view! {
                    <div class="mt-0.5 flex flex-wrap items-center gap-1 pl-6">
                        {after}
                        <SelectField compact=true action=true options=options current=String::new() on_change=put_after />
                    </div>
                })}
            </li>
        }
    }).collect_view();
    let hint = hint.map(|h| view! { <StatusHintLine task=task hint=h /> });
    view! {
        <div class="mb-2">
            {hint}
            <div class="mb-1 flex items-center gap-2 text-[11px] text-muted">
                <span>{text}</span>
                <span class="h-1 flex-1 overflow-hidden rounded-sm bg-line" aria-hidden="true">
                    <span class="block h-full bg-success" style=format!("width:{bar}%")></span>
                </span>
            </div>
            <ul class="space-y-1">{rows}</ul>
            <p class="mt-2 text-[11px] text-muted">
                "This task is a group: its dates come from its subtasks and its own estimate isn't used. A blocks link on it applies to every subtask. Use \"Do after…\" to put subtasks in order."
            </p>
        </div>
    }
    .into_any()
}

/// A box that makes a new subtask on Enter, and a picker that links an existing task.
#[component]
fn AddSubtask(task: Uuid, options: Vec<(String, String)>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let title = RwSignal::new(String::new());
    let add = move || {
        let text = title.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            if finish(api::create_subtask(task, text).await, toasts, version).is_some() {
                title.set(String::new());
            }
        });
    };
    let link = move |v: String| {
        if let Ok(child) = Uuid::parse_str(&v) {
            spawn_local(async move {
                finish(api::set_parent(child, Some(task)).await, toasts, version);
            });
        }
    };
    view! {
        <div class="space-y-2">
            <input class=INPUT type="text" placeholder="Add a subtask… (Enter)" aria-label="New subtask"
                prop:value=move || title.get()
                on:input=move |ev| title.set(event_target_value(&ev))
                on:keydown=move |ev| if ev.key() == "Enter" { add() } />
            <SelectField compact=true action=true options=options current=String::new() on_change=link />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::NodeSummary;

    fn sub(status: TaskStatus) -> Subtask {
        Subtask {
            node: NodeSummary {
                node: NodeRef::new(NodeType::Task, Uuid::nil()),
                label: "x".into(),
                archived: false,
            },
            status,
            due_date: None,
            assignee: None,
            after: Vec::new(),
            waiting: false,
            next: false,
        }
    }

    #[test]
    fn progress_counts_done_over_the_ones_still_in_play() {
        use TaskStatus::*;
        let list = [
            sub(Done),
            sub(Done),
            sub(Todo),
            sub(InProgress),
            sub(Cancelled),
        ];
        assert_eq!(progress(&list), (2, 4));
        assert_eq!(progress_text(&list), "2 of 4 done");
        assert_eq!(progress_text(&[sub(Cancelled)]), "");
        assert_eq!(progress_text(&[]), "");
    }

    #[test]
    fn the_bar_is_a_whole_percent() {
        assert_eq!(percent(2, 4), 50);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(0, 0), 0);
    }
}
