//! Detail-pane body for a task: editable fields, assignee, project, archive.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    NodeRef, NodeType, Patch, PersonRow, Task, TaskDetail, TaskStatus, UpdateTask, Uuid,
    MEETING_TYPE,
};

use crate::{
    api,
    components::{
        detail_pane::{Section, FIELD_GROUP},
        focus::FocusField,
        form::{date_patch, SelectField, TextField, BUTTON, BUTTON_DANGER},
        item_notes::ItemNotes,
        markdown_box::{saver, MarkdownField},
        meeting::{MakeMeeting, MeetingFields},
        people_panel::error_line,
        reference_links::ReferenceLinks,
        repeat_field::RepeatField,
        summary_chips::TaskSummary,
        task_links::TaskLinks,
        task_type::{use_task_types, TaskTiming},
        what_if_button::WhatIfButton,
    },
    labels::{estimate_text, priority_option, task_status_label, PRIORITY_TINT, TASK_STATUS_TINT},
    state::{finish, DataVersion, Selection, Toasts},
};

#[component]
pub fn TaskPanel(id: Uuid) -> impl IntoView {
    // Loaded once per opening so typing is never overwritten by a reload; `reload` asks for it
    // again (a task made a meeting or turned back, a meeting the clock moved on).
    let reload = RwSignal::new(0u32);
    let detail = LocalResource::new(move || {
        reload.get();
        api::get_task_detail(id)
    });
    // A meeting moves itself on with the clock (spec 38): when its status changes under an open
    // pane, load the pane again so the Status box says so.
    let version = expect_context::<DataVersion>();
    let latest = LocalResource::new(move || {
        version.track();
        api::get_task(id)
    });
    Effect::new(move |_| {
        let Some(Ok(now)) = latest.get() else { return };
        if let Some(Ok(shown)) = detail.get_untracked() {
            if now.is_meeting() && shown.task.status != now.status {
                reload.update(|n| *n += 1);
            }
        }
    });
    let people = LocalResource::new(api::list_people);
    let projects = LocalResource::new(move || api::list_node_summaries(NodeType::Project));

    view! {
        <Section title="Fields" always_open=true>
            <TaskSummary id=id />
            {move || match (detail.get(), people.get(), projects.get()) {
                (Some(Ok(d)), Some(Ok(ps)), Some(Ok(pr))) => view! { <TaskFields detail=d people=ps projects=pr reload=reload /> }.into_any(),
                (Some(Err(e)), _, _) | (_, Some(Err(e)), _) | (_, _, Some(Err(e))) => error_line(e),
                _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
        <TaskLinks task=id />
        <ReferenceLinks task=id />
        <ItemNotes node=NodeRef::new(NodeType::Task, id) />
        <WhatIfButton node=NodeRef::new(NodeType::Task, id) />
        <ArchiveTask id=id />
    }
}

#[component]
fn TaskFields(
    detail: TaskDetail,
    people: Vec<PersonRow>,
    projects: Vec<minimap_types::NodeSummary>,
    reload: RwSignal<u32>,
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
    // The type dropdown follows the list (it is rebuilt if the list changes) and the value saved.
    let types = use_task_types();
    let type_now = RwSignal::new(task.task_type.clone());
    // Meetings (spec 38): Meeting needs a day and a time, so picking it on a task that has none
    // asks first (`making`), and the type box is rebuilt (`type_epoch`) if that is cancelled.
    let is_meeting = task.is_meeting();
    let making = RwSignal::new(false);
    let type_epoch = RwSignal::new(0u32);
    let save_type = move |v: String| {
        if v == MEETING_TYPE && !is_meeting {
            making.set(true);
            return;
        }
        making.set(false);
        let task_type = if v.is_empty() {
            Patch::Clear
        } else {
            Patch::Set(v.clone())
        };
        type_now.set((!v.is_empty()).then_some(v));
        // Turning a meeting back into a task drops its time, so the pane is loaded again.
        spawn_local(async move {
            let patch = UpdateTask {
                task_type,
                ..Default::default()
            };
            if finish(api::update_task(id, patch).await, toasts, version).is_some() && is_meeting {
                reload.update(|n| *n += 1);
            }
        });
    };
    let cancel_making = Callback::new(move |()| {
        making.set(false);
        type_epoch.update(|n| *n += 1);
        reload.update(|n| *n += 1);
    });
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
    view! {
        <TextField label="Title" value=task.title.clone()
            on_commit=move |v: String| save(UpdateTask { title: Some(v), ..Default::default() }) />
        <MarkdownField label="Description" value=task.description.clone() node=minimap_types::NodeRef::new(NodeType::Task, id)
            empty="No description yet. Click the pencil to write one." placeholder="Describe the task. Markdown works: lists, **bold**, @ to mention." rows=6
            save=saver(move |v: String| async move {
                finish(api::update_task(id, UpdateTask { description: Some(v), ..Default::default() }).await, toasts, version).is_some()
            }) />
        <div class=FIELD_GROUP>
        <div class="grid grid-cols-3 gap-3">
            <SelectField label="Status" options=status_options
                current=task.status.as_str().to_owned() on_change=save_status
                tint=TASK_STATUS_TINT />
            <SelectField label="Priority" options=priority_options
                current=task.priority.to_string() on_change=save_priority
                tint=PRIORITY_TINT />
            {move || {
                type_epoch.track();
                let current = type_now.get_untracked();
                view! {
                    <SelectField label="Type" options=types.options(current.as_deref())
                        current=current.unwrap_or_default() on_change=save_type />
                }
            }}
        </div>
        {move || making.get().then(|| view! {
            <MakeMeeting task=id due=task.due_date on_cancel=cancel_making />
        })}
        <div class="mt-2 grid grid-cols-2 gap-3">
            <SelectField label="Project" options=project_options
                current=task.project_id.map(|p| p.to_string()).unwrap_or_default() on_change=save_project />
            <SelectField label="Assignee" options=assignee_options
                current=detail.assignee.as_ref().map(|a| a.node.id.to_string()).unwrap_or_default()
                on_change=save_assignee />
        </div>
        </div>
        <div class=FIELD_GROUP>
        {if is_meeting {
            // A meeting has a day, a time and a length instead of dates and an estimate.
            view! { <MeetingFields task=task.clone() /> }.into_any()
        } else {
            view! {
                <div class="grid grid-cols-3 gap-3">
                    <TextField label="Estimate (3d, 4h)" placeholder="3d" value=estimate_text(task.estimate_days)
                        on_commit=save_estimate />
                    <TextField label="Start date" kind="date"
                        value=task.start_date.map(|d| d.to_string()).unwrap_or_default()
                        on_commit=move |v: String| save_date(v, false) />
                    <TextField label="Due date" kind="date"
                        value=task.due_date.map(|d| d.to_string()).unwrap_or_default()
                        on_commit=move |v: String| save_date(v, true) />
                </div>
            }.into_any()
        }}
        <RepeatField node=NodeRef::new(NodeType::Task, id) current=task.recurrence.clone() />
        {(!is_meeting).then(|| view! { <FocusField task=id focus=task.focus /> })}
        <TaskTiming id=id />
        </div>
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
        <Section title="Archive" collapsed=true tone=crate::components::page::Tone::Danger>
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
                view! { <button class=BUTTON_DANGER on:click=move |_| confirming.set(true)>"Archive task…"</button> }.into_any()
            }}
        </Section>
    }
}
