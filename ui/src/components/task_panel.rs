//! Detail-pane body for a task: editable fields, assignee, project, archive.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::fmt_ts, NodeType, Patch, PersonRow, Task, TaskDetail, TaskStatus, UpdateTask, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{date_patch, SelectField, TextField, BUTTON, BUTTON_DANGER},
        people_panel::error_line,
    },
    labels::{estimate_text, priority_option, task_status_label},
    state::{finish, DataVersion, Selection, Toasts},
};

#[component]
pub fn TaskPanel(id: Uuid) -> impl IntoView {
    // Loaded once per opening so typing is never overwritten by a reload.
    let detail = LocalResource::new(move || api::get_task_detail(id));
    let people = LocalResource::new(api::list_people);
    let projects = LocalResource::new(move || api::list_node_summaries(NodeType::Project));

    view! {
        <Section title="Fields">
            {move || match (detail.get(), people.get(), projects.get()) {
                (Some(Ok(d)), Some(Ok(ps)), Some(Ok(pr))) => view! { <TaskFields detail=d people=ps projects=pr /> }.into_any(),
                (Some(Err(e)), _, _) | (_, Some(Err(e)), _) | (_, _, Some(Err(e))) => error_line(e),
                _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
        <ArchiveTask id=id />
    }
}

#[component]
fn TaskFields(
    detail: TaskDetail,
    people: Vec<PersonRow>,
    projects: Vec<minimap_types::NodeSummary>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let task: Task = detail.task;
    let id = task.id;
    let save = move |patch: UpdateTask| {
        spawn_local(async move {
            finish(api::update_task(id, patch).await, toasts, version);
        });
    };
    let save_date = move |v: String, which: bool| match date_patch(&v) {
        Ok(p) => save(if which {
            UpdateTask {
                due_date: p,
                ..Default::default()
            }
        } else {
            UpdateTask {
                start_date: p,
                ..Default::default()
            }
        }),
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };
    let save_estimate = move |text: String| {
        spawn_local(async move {
            finish(api::set_task_estimate(id, text).await, toasts, version);
        });
    };
    let save_status = move |v: String| {
        if let Ok(s) = TaskStatus::from_str(&v) {
            save(UpdateTask {
                status: Some(s),
                ..Default::default()
            });
        }
    };
    let save_priority = move |v: String| {
        if let Ok(p) = v.parse::<u8>() {
            save(UpdateTask {
                priority: Some(p),
                ..Default::default()
            });
        }
    };
    let save_project = move |v: String| {
        let project_id = match Uuid::parse_str(&v) {
            Ok(p) => Patch::Set(p),
            Err(_) => Patch::Clear,
        };
        save(UpdateTask {
            project_id,
            ..Default::default()
        });
    };
    let save_assignee = move |v: String| {
        let person = Uuid::parse_str(&v).ok();
        spawn_local(async move {
            finish(api::set_assignee(id, person).await, toasts, version);
        });
    };

    let status_options: Vec<(String, String)> = TaskStatus::ALL
        .iter()
        .map(|s| (s.as_str().to_owned(), task_status_label(*s).to_owned()))
        .collect();
    let priority_options: Vec<(String, String)> = (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect();
    let project_options: Vec<(String, String)> =
        std::iter::once((String::new(), "No project (inbox)".to_owned()))
            .chain(
                projects
                    .iter()
                    .map(|p| (p.node.id.to_string(), p.label.clone())),
            )
            .collect();
    let assignee_options: Vec<(String, String)> =
        std::iter::once((String::new(), "Unassigned".to_owned()))
            .chain(people.iter().map(|p| {
                let name = if p.person.is_self {
                    format!("{} (you)", p.person.name)
                } else {
                    p.person.name.clone()
                };
                (p.person.id.to_string(), name)
            }))
            .collect();
    let completed = task
        .completed_at
        .map(|t| fmt_ts(t).chars().take(10).collect::<String>());

    view! {
        <TextField label="Title" value=task.title.clone()
            on_commit=move |v: String| save(UpdateTask { title: Some(v), ..Default::default() }) />
        <TextField label="Description" multiline=true value=task.description.clone()
            on_commit=move |v: String| save(UpdateTask { description: Some(v), ..Default::default() }) />
        <div class="grid grid-cols-2 gap-3">
            <SelectField label="Status" options=status_options
                current=task.status.as_str().to_owned() on_change=save_status />
            <SelectField label="Priority" options=priority_options
                current=task.priority.to_string() on_change=save_priority />
        </div>
        <div class="mt-2 grid grid-cols-2 gap-3">
            <SelectField label="Project" options=project_options
                current=task.project_id.map(|p| p.to_string()).unwrap_or_default() on_change=save_project />
            <SelectField label="Assignee" options=assignee_options
                current=detail.assignee.as_ref().map(|a| a.node.id.to_string()).unwrap_or_default()
                on_change=save_assignee />
        </div>
        <div class="mt-2 grid grid-cols-3 gap-3">
            <TextField label="Estimate (3d, 4h)" placeholder="3d" value=estimate_text(task.estimate_days)
                on_commit=save_estimate />
            <TextField label="Start date" kind="date"
                value=task.start_date.map(|d| d.to_string()).unwrap_or_default()
                on_commit=move |v: String| save_date(v, false) />
            <TextField label="Due date" kind="date"
                value=task.due_date.map(|d| d.to_string()).unwrap_or_default()
                on_commit=move |v: String| save_date(v, true) />
        </div>
        {completed.map(|c| view! { <p class="mt-1 text-[11px] text-muted">"Completed " {c}</p> })}
    }
}

#[component]
fn ArchiveTask(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let confirming = RwSignal::new(false);

    let confirm = move |_| {
        spawn_local(async move {
            if finish(api::archive_task(id).await, toasts, version).is_some() {
                confirming.set(false);
                selection.close();
            }
        });
    };

    view! {
        <Section title="Archive">
            {move || if confirming.get() {
                view! {
                    <div class="space-y-2">
                        <p>"Archive this task? Its links (assignee, blocks) are archived with it."</p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON on:click=move |_| confirming.set(true)>"Archive task…"</button> }.into_any()
            }}
        </Section>
    }
}
