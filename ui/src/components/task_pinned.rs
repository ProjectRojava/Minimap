//! What a task's pane keeps in view while the rest scrolls: under the title, its status, priority
//! and due date as controls that save when changed, a *Done* button, and one line that says what
//! is wrong with it (overdue, due today, blocked and by what). The same controls are in the
//! Fields below; these are the ones that don't scroll away.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{EdgeType, NodeSummary, Task, TaskStatus, UpdateTask, Uuid};

use crate::{
    api,
    components::{
        focus::FocusStar,
        form::{date_patch, DateField, SelectField, BUTTON, BUTTON_SUCCESS},
        page::Tone,
        task_board::today,
    },
    labels::{priority_option, task_status_label, PRIORITY_TINT, TASK_STATUS_TINT},
    state::{finish_task, DataVersion, FinishDecision, Selection, Toasts},
};

/// The warnings of a task, most pressing first: how late it is, or that it is due today, and what
/// blocks it. `blockers` are the open tasks that block it; `today` is none when the clock can't be
/// read.
pub fn warnings(
    task: &Task,
    today: Option<minimap_types::Date>,
    blockers: &[NodeSummary],
) -> Vec<(Tone, String)> {
    let open = !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled);
    let mut out = Vec::new();
    if !open {
        return out;
    }
    if let (Some(due), Some(today)) = (task.due_date, today) {
        let days = (today - due).whole_days();
        if days > 0 {
            let plural = if days == 1 { "" } else { "s" };
            out.push((Tone::Danger, format!("Overdue by {days} day{plural}")));
        } else if days == 0 {
            out.push((Tone::Warning, "Due today".to_owned()));
        }
    }
    match blockers {
        [] if task.status == TaskStatus::Blocked => {
            out.push((Tone::Warning, "Marked blocked".to_owned()));
        }
        [] => {}
        [one] => out.push((Tone::Warning, format!("Blocked by {}", one.label))),
        [first, rest @ ..] => out.push((
            Tone::Warning,
            format!("Blocked by {} and {} more", first.label, rest.len()),
        )),
    }
    out
}

#[component]
pub fn TaskPinned(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let gate = expect_context::<FinishDecision>();
    let selection = expect_context::<Selection>();
    let task = LocalResource::new(move || {
        version.track();
        api::get_task(id)
    });
    // The open tasks that block this one: the incoming "blocks" links, minus the finished ones.
    let blockers = LocalResource::new(move || {
        version.track();
        async move {
            let links = api::list_edges_for(id).await.unwrap_or_default();
            let mut open = Vec::new();
            for l in links
                .into_iter()
                .filter(|l| l.edge.edge_type == EdgeType::Blocks && !l.outgoing && !l.other.archived)
            {
                if let Ok(t) = api::get_task(l.other.node.id).await {
                    if !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled) {
                        open.push(l.other);
                    }
                }
            }
            open
        }
    });
    let save = move |patch: UpdateTask| {
        spawn_local(async move {
            finish_task(api::update_task(id, patch).await, id, toasts, version, gate);
        });
    };
    let on_status = move |v: String| {
        if let Ok(s) = TaskStatus::from_str(&v) {
            save(UpdateTask {
                status: Some(s),
                ..Default::default()
            });
        }
    };
    let on_priority = move |v: String| {
        if let Ok(p) = v.parse::<u8>() {
            save(UpdateTask {
                priority: Some(p),
                ..Default::default()
            });
        }
    };
    let on_due = move |v: String| match date_patch(&v) {
        Ok(due_date) => save(UpdateTask {
            due_date,
            ..Default::default()
        }),
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };
    let status_options: Vec<(String, String)> = TaskStatus::ALL
        .iter()
        .map(|s| (s.as_str().to_owned(), task_status_label(*s).to_owned()))
        .collect();
    let priority_options: Vec<(String, String)> = (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect();

    move || {
        let t = task.get().and_then(|r| r.ok())?;
        let open = !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled);
        let is_meeting = t.is_meeting();
        let due = t.due_date.map(|d| d.to_string()).unwrap_or_default();
        let blocking = blockers.get().unwrap_or_default();
        let lines = warnings(&t, today(), &blocking);
        let first_blocker = blocking.first().map(|b| b.node);
        let done_button = if open {
            view! {
                <button class=BUTTON_SUCCESS title="Mark done"
                        on:click=move |_| save(UpdateTask { status: Some(TaskStatus::Done), ..Default::default() })>
                    "✓ Done"
                </button>
            }
            .into_any()
        } else {
            view! {
                <button class=BUTTON title="Open it again"
                        on:click=move |_| save(UpdateTask { status: Some(TaskStatus::Todo), ..Default::default() })>
                    "↺ Reopen"
                </button>
            }
            .into_any()
        };
        Some(view! {
            <div class="mt-2 flex flex-wrap items-center gap-2">
                <SelectField compact=true options=status_options.clone()
                             current=t.status.as_str().to_owned() on_change=on_status
                             tint=TASK_STATUS_TINT />
                {(!is_meeting).then(|| view! {
                    <SelectField compact=true options=priority_options.clone()
                                 current=t.priority.to_string() on_change=on_priority
                                 tint=PRIORITY_TINT />
                    <DateField compact=true current=due.clone() on_commit=on_due placeholder="Due" />
                    <FocusStar task=id focus=t.focus />
                })}
                <span class="ml-auto">{done_button}</span>
            </div>
            {(!lines.is_empty()).then(|| view! {
                <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                    {lines.into_iter().map(|(tone, text)| view! { <span class=tone.chip()>{text}</span> }).collect_view()}
                    {first_blocker.map(|node| view! {
                        <button class="text-[11px] text-accent hover:underline"
                                on:click=move |_| selection.open(node)>"Open blocker"</button>
                    })}
                </div>
            })}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{Date, NodeRef, NodeType};
    use time::macros::date;

    fn task(status: TaskStatus, due: Option<Date>) -> Task {
        let mut t: Task = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "title": "t", "description": "", "project_id": null, "status": "todo",
            "estimate_days": null, "start_date": null, "due_date": null, "completed_at": null,
            "priority": 3, "created_at": "2027-03-01T00:00:00Z", "updated_at": "2027-03-01T00:00:00Z",
            "archived_at": null
        }))
        .unwrap();
        t.status = status;
        t.due_date = due;
        t
    }

    fn blocker(label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Task, Uuid::nil()),
            label: label.into(),
            archived: false,
        }
    }

    const TODAY: Date = date!(2027 - 03 - 10);

    #[test]
    fn a_late_task_says_how_late_and_one_due_today_says_so() {
        let late = warnings(&task(TaskStatus::Todo, Some(date!(2027 - 03 - 07))), Some(TODAY), &[]);
        assert_eq!(late, vec![(Tone::Danger, "Overdue by 3 days".to_owned())]);
        let one = warnings(&task(TaskStatus::Todo, Some(date!(2027 - 03 - 09))), Some(TODAY), &[]);
        assert_eq!(one[0].1, "Overdue by 1 day");
        let today = warnings(&task(TaskStatus::Todo, Some(TODAY)), Some(TODAY), &[]);
        assert_eq!(today, vec![(Tone::Warning, "Due today".to_owned())]);
        assert!(warnings(&task(TaskStatus::Todo, Some(date!(2027 - 03 - 11))), Some(TODAY), &[]).is_empty());
    }

    #[test]
    fn blockers_are_named_and_counted() {
        let t = task(TaskStatus::Todo, None);
        assert_eq!(
            warnings(&t, Some(TODAY), &[blocker("API")])[0].1,
            "Blocked by API"
        );
        assert_eq!(
            warnings(&t, Some(TODAY), &[blocker("API"), blocker("DB"), blocker("UI")])[0].1,
            "Blocked by API and 2 more"
        );
        // Marked blocked with nothing linked still says so.
        assert_eq!(
            warnings(&task(TaskStatus::Blocked, None), Some(TODAY), &[])[0].1,
            "Marked blocked"
        );
    }

    #[test]
    fn a_finished_task_has_nothing_to_warn_about() {
        let done = task(TaskStatus::Done, Some(date!(2027 - 03 - 01)));
        assert!(warnings(&done, Some(TODAY), &[blocker("API")]).is_empty());
        assert!(warnings(&task(TaskStatus::Cancelled, None), None, &[]).is_empty());
    }

    #[test]
    fn without_a_clock_only_the_blockers_are_known() {
        let t = task(TaskStatus::Todo, Some(date!(2027 - 03 - 01)));
        assert!(warnings(&t, None, &[]).is_empty());
    }
}
