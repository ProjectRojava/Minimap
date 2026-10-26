//! Detail-pane body for a task: editable fields, assignee, project, archive.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    NodeRef, NodeType, Patch, PersonRow, Task, TaskDetail, TaskFigure, TaskStatus, UpdateTask,
    Uuid, DECISION_TYPE, MEETING_TYPE,
};

use crate::{
    api,
    components::{
        backburner::BackburnerField,
        confirm_archive::{ArchiveChoice, ArchiveRequest, ConfirmArchive},
        detail_pane::{Section, FIELD_GROUP},
        files_links::FilesAndLinks,
        focus::FocusField,
        form::{date_patch, SelectField, TextField, BUTTON_DANGER},
        item_notes::ItemNotes,
        markdown_box::{saver, MarkdownField},
        meeting::{MakeMeeting, MeetingFields},
        page::Icon,
        pane_reveal::Reveal,
        people_panel::error_line,
        repeat_field::RepeatField,
        summary_chips::TaskSummary,
        task_add_row::TaskAddRow,
        task_decision::TaskDecision,
        task_links::TaskLinks,
        task_type::{use_task_types, TaskTiming},
        task_waiting::{archive_message, TaskWaiting},
    },
    labels::{estimate_text, priority_option, PRIORITY_TINT},
    state::{finish, finish_task, DataVersion, FinishDecision, Selection, Toasts},
};

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{Decision, DecisionStatus};

    fn record(decision: &str, rationale: &str) -> Decision {
        Decision {
            id: Uuid::nil(),
            title: "Pick the store".into(),
            context: String::new(),
            decision: decision.into(),
            rationale: rationale.into(),
            decided_on: None,
            status: DecisionStatus::Proposed,
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
            auto_status: false,
            auto_decided_on: false,
        }
    }

    #[test]
    fn leaving_decision_asks_and_says_whether_something_was_written() {
        let (title, message, label) =
            type_change_question(Some(&record("", "")), false, None).unwrap();
        assert_eq!(title, "Remove the decision record?");
        assert!(message.contains("\"Pick the store\" from the Decisions log"));
        assert!(!message.contains("what was decided"));
        assert!(message.contains("can't be undone"));
        assert_eq!(label, "Remove record and change type");

        let (_, written, _) =
            type_change_question(Some(&record("Postgres", "")), false, None).unwrap();
        assert!(written.contains("with what was decided and why"));
    }

    #[test]
    fn nothing_is_asked_when_there_is_no_record() {
        assert!(type_change_question(None, false, None).is_none());
    }

    #[test]
    fn leaving_meeting_names_the_time_that_goes() {
        let (title, message, _) = type_change_question(None, true, Some("10:30 – 11:30")).unwrap();
        assert_eq!(title, "Change this meeting into a task?");
        assert!(message.contains("(10:30 – 11:30)") && message.contains("due date"));
        let (_, plain, _) = type_change_question(None, true, None).unwrap();
        assert!(plain.starts_with("Its time is removed"));
    }
}

#[component]
pub fn TaskPanel(id: Uuid) -> impl IntoView {
    // What the person asked to see among the sections that stay hidden while empty.
    Reveal::provide();
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
        <TaskDecision task=id />
        <TaskWaiting task=id />
        // Below the fields: what a task can have, hidden while it has none of it, and the row
        // that starts the first of each (and "what if").
        <TaskAddRow id=id />
        <TaskLinks task=id />
        <FilesAndLinks task=id />
        <ItemNotes node=NodeRef::new(NodeType::Task, id) hide_when_empty=true />
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
    let gate = expect_context::<FinishDecision>();
    let task: Task = detail.task;
    let id = task.id;
    let save = move |patch: UpdateTask| {
        spawn_local(async move {
            finish_task(api::update_task(id, patch).await, id, toasts, version, gate);
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
    // The estimate can change under the open pane: moving either date re-counts it (in the store),
    // and the dates are saved from here and from the pinned header. The box is drawn again only
    // when the saved estimate is different from what it shows.
    let saved = LocalResource::new(move || {
        version.track();
        api::get_task(id)
    });
    let estimate_shown = RwSignal::new(estimate_text(task.estimate_days));
    let start_shown = RwSignal::new(task.start_date.map(|d| d.to_string()).unwrap_or_default());
    Effect::new(move |_| {
        if let Some(Ok(t)) = saved.get() {
            estimate_shown.set(estimate_text(t.estimate_days));
            start_shown.set(t.start_date.map(|d| d.to_string()).unwrap_or_default());
        }
    });
    let estimate_shown = Memo::new(move |_| estimate_shown.get());
    let start_shown = Memo::new(move |_| start_shown.get());
    let save_estimate = move |text: String| {
        spawn_local(async move {
            finish(api::set_task_estimate(id, text).await, toasts, version);
        });
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
    let confirm = expect_context::<ConfirmArchive>();
    // When the time of a meeting is what is about to go, the dialog says what it was.
    let meeting_time = StoredValue::new(
        task.start_minute
            .map(|m| minimap_types::fmt_range(m, task.meeting_minutes())),
    );
    let apply_type = move |v: String| {
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
    let save_type = move |v: String| {
        if v == MEETING_TYPE && !is_meeting {
            making.set(true);
            return;
        }
        // Leaving a type that owns something removes it: a Decision's record, a meeting's time.
        // That is asked first, and the type box goes back to what it was if the answer is no.
        let leaving_decision =
            type_now.get_untracked().as_deref() == Some(DECISION_TYPE) && v != DECISION_TYPE;
        let leaving_meeting = is_meeting && v != MEETING_TYPE;
        if !leaving_decision && !leaving_meeting {
            return apply_type(v);
        }
        // Made by the dialog's own owner: ones made here would go with the type box, which is
        // redrawn before the question is answered.
        let go = confirm.callback({
            let v = v.clone();
            move || apply_type(v.clone())
        });
        let back = confirm.callback(move || type_epoch.update(|n| *n += 1));
        let meeting_time = meeting_time.get_value();
        spawn_local(async move {
            let record = if leaving_decision {
                api::get_task_detail(id).await.ok().and_then(|d| d.decision)
            } else {
                None
            };
            let Some((title, message, label)) =
                type_change_question(record.as_ref(), leaving_meeting, meeting_time.as_deref())
            else {
                // Nothing would be lost (a Decision task whose record is already gone).
                return go.run(());
            };
            confirm.ask_with(ArchiveRequest {
                title,
                message,
                body: None,
                choices: vec![ArchiveChoice { label, run: go }],
                on_cancel: Some(back),
            });
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

    let priority_options: Vec<(String, String)> = (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect();
    // A project can be changed but not taken away; "No project" is only there for a meeting or a
    // task from before projects were required, which have none.
    let can_clear_project = is_meeting || task.project_id.is_none();
    let colours = crate::components::objective_colour::use_objective_colours();
    let project_options: Vec<(String, String)> = can_clear_project
        .then(|| (String::new(), "No project (inbox)".to_owned()))
        .into_iter()
        .chain(projects.iter().map(|p| colours.project_choice(p)))
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
        // Status, priority and the due date are in the pinned header (`task_pinned`), so they are
        // not here a second time. A meeting's header has no priority, so its priority stays.
        <div class=if is_meeting { "grid grid-cols-2 gap-3" } else { "grid grid-cols-3 gap-3" }>
            {is_meeting.then(|| view! {
                <SelectField label="Priority" options=priority_options
                    current=task.priority.to_string() on_change=save_priority
                    tint=PRIORITY_TINT />
            })}
            {move || {
                type_epoch.track();
                let current = type_now.get_untracked();
                view! {
                    <SelectField label="Type" options=types.options(current.as_deref())
                        current=current.unwrap_or_default() on_change=save_type />
                }
            }}
            <SelectField label="Project" options=project_options
                current=task.project_id.map(|p| p.to_string()).unwrap_or_default() on_change=save_project />
            <SelectField label="Assignee" options=assignee_options
                current=detail.assignee.as_ref().map(|a| a.node.id.to_string()).unwrap_or_default()
                on_change=save_assignee />
        </div>
        {move || making.get().then(|| view! {
            <MakeMeeting task=id due=task.due_date on_cancel=cancel_making />
        })}
        </div>
        <div class=FIELD_GROUP>
        {if is_meeting {
            // A meeting has a day, a time and a length instead of dates and an estimate.
            view! { <MeetingFields task=task.clone() /> }.into_any()
        } else {
            view! {
                <div class="grid grid-cols-2 gap-3">
                    <div class="relative">
                        <FigureButton id=id figure=TaskFigure::Estimate
                            title="Work out the estimate from the start date and the due date" />
                        {move || view! {
                            <TextField label="Estimate (3d, 4h)" placeholder="3d" value=estimate_shown.get()
                                on_commit=save_estimate />
                        }}
                    </div>
                    <div class="relative">
                        <FigureButton id=id figure=TaskFigure::Start
                            title="Work out the start date from the due date and the estimate" />
                        {move || view! {
                            <TextField label="Start date" kind="date" value=start_shown.get()
                                on_commit=move |v: String| save_date(v, false) />
                        }}
                    </div>
                </div>
            }.into_any()
        }}
        <RepeatField node=NodeRef::new(NodeType::Task, id) current=task.recurrence.clone() />
        {(!is_meeting).then(|| view! { <FocusField task=id focus=task.focus /> })}
        {(!is_meeting && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled))
            .then(|| view! { <BackburnerField task=id since=task.backburner_since /> })}
        <TaskTiming id=id />
        </div>
    }
}

/// A small calculator beside the start date, the due date or the estimate: works that one out
/// from the other two (working days) in one change, or says what is missing.
#[component]
pub fn FigureButton(id: Uuid, figure: TaskFigure, title: &'static str) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    view! {
        <button type="button" title=title aria-label=title
            class="absolute right-0 top-0 rounded-sm border border-accent/40 bg-accent/10 p-px text-accent hover:border-accent hover:bg-accent/25"
            on:click=move |_| spawn_local(async move {
                finish(api::compute_task_figure(id, figure).await, toasts, version);
            })>
            <Icon name="calc" size="h-3.5 w-3.5" />
        </button>
    }
}

/// What the dialog asks when a task's type changes away from one that owns something: the title,
/// what is lost, and the button. None when nothing would be lost. A meeting loses its time (the
/// day stays, as the due date); a Decision loses its record, with whatever was written in it.
pub fn type_change_question(
    record: Option<&minimap_types::Decision>,
    leaving_meeting: bool,
    meeting_time: Option<&str>,
) -> Option<(String, String, String)> {
    if leaving_meeting {
        let time = meeting_time.map(|t| format!(" ({t})")).unwrap_or_default();
        return Some((
            "Change this meeting into a task?".to_owned(),
            format!("Its time{time} is removed. The day stays as the due date."),
            "Change type".to_owned(),
        ));
    }
    let record = record?;
    let written = !record.decision.trim().is_empty() || !record.rationale.trim().is_empty();
    let what = if written {
        format!("\"{}\", with what was decided and why,", record.title)
    } else {
        format!("\"{}\"", record.title)
    };
    Some((
        "Remove the decision record?".to_owned(),
        format!(
            "Changing the type removes the decision record {what} from the Decisions log and from \
             everything it is linked to. This can't be undone."
        ),
        "Remove record and change type".to_owned(),
    ))
}

/// *Archive…* for a task, in the pinned header of the pane so it is there however far the pane
/// has been scrolled. Asks first; Ctrl/Cmd+Z brings the task back.
#[component]
pub fn ArchiveTaskButton(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let confirm = expect_context::<ConfirmArchive>();

    let archive = move || {
        spawn_local(async move {
            if finish(api::archive_task(id).await, toasts, version).is_some() {
                selection.close();
            }
        });
    };
    view! {
        <button class=format!("shrink-0 {BUTTON_DANGER}") title="Archive this task (Ctrl/Cmd+Z undoes)"
            on:click=move |_| spawn_local(async move {
                let message = archive_message(id).await;
                confirm.ask("Archive this task?", message, archive);
            })>
            "Archive…"
        </button>
    }
}
