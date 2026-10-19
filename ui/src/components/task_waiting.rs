//! The *Waiting on* section of a task's pane (spec 56): who the task is stuck on, with how long,
//! and a form to add one. A waiting-on is the ordinary record on the Waiting On screen with an
//! `about` link to the task; this is where it is made for a task and cleared when it arrives.
//!
//! The section stays hidden while the task has none (`pane_reveal`); the *Add* row asks for it.
//! Minimap only *suggests* a status (mark blocked, move on); it never changes one by itself.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::parse_date, AppError, CreateWaitingOn, Date, NodeRef, NodeType, TaskStatus, Uuid,
    WaitingOnRow,
};

use crate::{
    api,
    components::{
        avatar::PersonLabel,
        detail_pane::{Section, SECTION_ACTION},
        form::{DateField, SelectField, BUTTON, BUTTON_PRIMARY, BUTTON_SUCCESS, INPUT},
        page::Tone,
        pane_reveal::{shown, Reveal},
        people_panel::error_line,
        task_board::today,
        waiting_panel::{age_text, snooze_options},
    },
    state::{finish, DataVersion, Selection, Toasts},
};

/// What Minimap offers to do to the task's status, as a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    MarkBlocked,
    MoveToInProgress,
}

/// The status suggestion: *Mark as blocked* while something is open on a task that is not
/// blocked, finished or cancelled; *Move to In progress* when a blocked task has just had its
/// last waiting-on resolved (today), so an old resolved one never nags.
pub fn status_offer(
    status: TaskStatus,
    open: usize,
    last_resolved: Option<Date>,
    today: Option<Date>,
) -> Option<Offer> {
    match status {
        TaskStatus::Todo | TaskStatus::InProgress if open > 0 => Some(Offer::MarkBlocked),
        TaskStatus::Blocked if open == 0 && last_resolved.is_some() && last_resolved == today => {
            Some(Offer::MoveToInProgress)
        }
        _ => None,
    }
}

/// "2 open", "1 open, 3 resolved", "3 resolved": the quiet text after the heading.
pub fn meta_text(open: usize, resolved: usize) -> String {
    match (open, resolved) {
        (0, 0) => String::new(),
        (o, 0) => format!("{o} open"),
        (0, r) => format!("{r} resolved"),
        (o, r) => format!("{o} open, {r} resolved"),
    }
}

/// The sentence the archive dialog adds when the task still has open waiting-ons: they stay on
/// Waiting On, no longer tied to the task. Empty when there are none.
pub fn archive_note(open: usize) -> String {
    match open {
        0 => String::new(),
        1 => " 1 open waiting-on stays in Waiting On.".to_owned(),
        n => format!(" {n} open waiting-ons stay in Waiting On."),
    }
}

/// What the archive dialog says about a task: its links go with it, any open waiting-ons stay,
/// and how to get it back.
pub async fn archive_message(task: Uuid) -> String {
    let open = api::get_task_waiting(task)
        .await
        .map(|rows| {
            rows.iter()
                .filter(|r| r.waiting.resolved_on.is_none())
                .count()
        })
        .unwrap_or(0);
    format!(
        "Its links (assignee, blocks) are archived with it.{} {}",
        archive_note(open),
        crate::components::confirm_archive::UNDO_HINT
    )
}

/// What the "what" box holds before anything is typed.
pub fn default_what(task_title: &str) -> String {
    format!("Waiting for {task_title}")
}

/// Reads the optional "expected by" box.
fn parse_expected(text: &str) -> Result<Option<Date>, AppError> {
    match text.trim() {
        "" => Ok(None),
        d => parse_date(d).map(Some).map_err(|_| AppError {
            code: "invalid".into(),
            message: "Expected by must be a date like 2027-03-31".into(),
        }),
    }
}

#[component]
pub fn TaskWaiting(task: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let rows = LocalResource::new(move || {
        version.track();
        api::get_task_waiting(task)
    });
    let detail = LocalResource::new(move || {
        version.track();
        api::get_task_detail(task)
    });
    let adding = RwSignal::new(false);
    // Asking for the section (the Add row's button) opens the form in it.
    if let Some(reveal) = use_context::<Reveal>() {
        Effect::new(move |_| {
            if reveal.wanted("waiting") {
                adding.set(true);
            }
        });
    }

    move || {
        let rows = rows.get()?;
        let detail = detail.get().and_then(Result::ok)?;
        let rows = match rows {
            Ok(r) => r,
            Err(e) => return Some(
                view! { <Section title="Waiting on" always_open=true>{error_line(e)}</Section> }
                    .into_any(),
            ),
        };
        if !shown("waiting", !rows.is_empty()) {
            return None;
        }
        let (open, resolved): (Vec<WaitingOnRow>, Vec<WaitingOnRow>) = rows
            .into_iter()
            .partition(|r| r.waiting.resolved_on.is_none());
        let last_resolved = resolved.iter().filter_map(|r| r.waiting.resolved_on).max();
        let offer = status_offer(detail.task.status, open.len(), last_resolved, today());
        let meta = meta_text(open.len(), resolved.len());
        let no_rows = open.is_empty() && resolved.is_empty();
        let title = detail.task.title.clone();
        Some(
            view! {
                <Section title="Waiting on" tone=Tone::Warning always_open=true
                    meta=move || meta.clone()
                    actions=move || view! {
                        <button class=SECTION_ACTION on:click=move |_| adding.update(|a| *a = !*a)>
                            {move || if adding.get() { "Cancel" } else { "+ Add" }}
                        </button>
                    }>
                    {offer.map(|o| view! { <OfferLine task=task offer=o /> })}
                    {no_rows.then(|| view! {
                        <Show when=move || !adding.get()>
                            <p class="text-muted">"Nothing is being waited for on this task."</p>
                        </Show>
                    })}
                    <Show when=move || adding.get()>
                        <AddForm task=task title=title.clone() done=move || adding.set(false) />
                    </Show>
                    {open.into_iter().map(|r| view! { <OpenRow row=r /> }).collect_view()}
                    {(!resolved.is_empty()).then(|| view! {
                        <details class="mt-2">
                            <summary class="cursor-pointer text-[11px] text-muted">
                                {format!("Resolved ({})", resolved.len())}
                            </summary>
                            {resolved.into_iter().map(|r| view! { <ResolvedRow row=r /> }).collect_view()}
                        </details>
                    })}
                </Section>
            }
            .into_any(),
        )
    }
}

/// The status suggestion and its button.
#[component]
fn OfferLine(task: Uuid, offer: Offer) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let (text, label, status) = match offer {
        Offer::MarkBlocked => (
            "This task is waiting on someone.",
            "Mark as blocked",
            TaskStatus::Blocked,
        ),
        Offer::MoveToInProgress => (
            "Nothing is left to wait for.",
            "Move to In progress",
            TaskStatus::InProgress,
        ),
    };
    let apply = move |_| {
        spawn_local(async move {
            finish(
                api::update_task(
                    task,
                    minimap_types::UpdateTask {
                        status: Some(status),
                        ..Default::default()
                    },
                )
                .await,
                toasts,
                version,
            );
        });
    };
    view! {
        <div class="mb-2 flex items-center gap-2">
            <span class="text-muted">{text}</span>
            <button class=BUTTON on:click=apply>{label}</button>
        </div>
    }
}

/// Person, what for, and when: the same facts as the Waiting On screen.
fn facts(row: &WaitingOnRow) -> String {
    let mut parts = vec![format!("asked {} ago", age_text(row.age_days))];
    if row.age_days == 0 {
        parts[0] = "asked today".to_owned();
    }
    if let Some(d) = row.waiting.expected_by {
        parts.push(format!("expected {d}"));
    }
    if let Some(d) = row.waiting.follow_up_on.filter(|_| row.snoozed) {
        parts.push(format!("snoozed until {d}"));
    }
    parts.join(" · ")
}

#[component]
fn OpenRow(row: WaitingOnRow) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = row.waiting.id;
    let detail = facts(&row);
    let snoozed = row.snoozed;
    let overdue = row.overdue;
    let resolve = move |_| {
        spawn_local(async move {
            finish(api::resolve_waiting_on(id).await, toasts, version);
        });
    };
    let wake = move |_| {
        spawn_local(async move {
            finish(api::snooze_waiting_on(id, None).await, toasts, version);
        });
    };
    let snooze = move |v: String| {
        if let Ok(days) = v.parse::<u32>() {
            spawn_local(async move {
                finish(
                    api::snooze_waiting_on(id, Some(days)).await,
                    toasts,
                    version,
                );
            });
        }
    };
    view! {
        <div class="mb-2 rounded-sm border border-line px-2 py-1.5">
            <div class="flex items-center gap-1.5">
                <span class="font-medium"><PersonLabel id=row.person.node.id name=row.person.label.clone() /></span>
                {row.stale.then(|| view! { <span class=Tone::Warning.chip()>"stale"</span> })}
                {snoozed.then(|| view! { <span class=Tone::Neutral.chip()>"snoozed"</span> })}
            </div>
            <p class="mt-0.5">{row.waiting.description.clone()}</p>
            <p class=if overdue { "mt-0.5 text-[11px] text-warning" } else { "mt-0.5 text-[11px] text-muted" }>
                {detail}
            </p>
            <div class="mt-1.5 flex flex-wrap items-center gap-2">
                <button class=BUTTON_SUCCESS on:click=resolve>"Resolve"</button>
                {if snoozed {
                    view! { <button class=BUTTON on:click=wake>"End snooze"</button> }.into_any()
                } else {
                    view! { <SelectField compact=true options=snooze_options() current=String::new() on_change=snooze /> }
                        .into_any()
                }}
                <button class=BUTTON
                    on:click=move |_| selection.open(NodeRef::new(NodeType::WaitingOn, id))>
                    "Open"
                </button>
            </div>
        </div>
    }
}

#[component]
fn ResolvedRow(row: WaitingOnRow) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = row.waiting.id;
    let when = row
        .waiting
        .resolved_on
        .map(|d| format!("resolved {d}"))
        .unwrap_or_default();
    let reopen = move |_| {
        spawn_local(async move {
            finish(api::reopen_waiting_on(id).await, toasts, version);
        });
    };
    view! {
        <div class="mt-1.5 flex items-center gap-2 text-muted">
            <span class="min-w-0 flex-1 truncate">
                {format!("{}: {} ({when})", row.person.label, row.waiting.description)}
            </span>
            <button class=BUTTON on:click=reopen>"Reopen"</button>
            <button class=BUTTON
                on:click=move |_| selection.open(NodeRef::new(NodeType::WaitingOn, id))>"Open"</button>
        </div>
    }
}

/// Who, what for and by when. Adds the waiting-on and its link to the task in one step.
#[component]
fn AddForm(task: Uuid, title: String, #[prop(into)] done: Callback<()>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let people = LocalResource::new(api::list_people);
    let who = RwSignal::new(String::new());
    let what = RwSignal::new(default_what(&title));
    let expected = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let description = what.get_untracked();
        let Ok(person_id) = Uuid::parse_str(&who.get_untracked()) else {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Choose who you are waiting on".into(),
            });
            return;
        };
        if description.trim().is_empty() {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Say what you are waiting for".into(),
            });
            return;
        }
        let expected_by = match parse_expected(&expected.get_untracked()) {
            Ok(d) => d,
            Err(e) => {
                toasts.error(&e);
                return;
            }
        };
        busy.set(true);
        spawn_local(async move {
            let input = CreateWaitingOn {
                description,
                person_id,
                asked_on: None,
                expected_by,
                follow_up_on: None,
            };
            let result = api::add_task_waiting(task, input).await;
            busy.set(false);
            if finish(result, toasts, version).is_some() {
                who.set(String::new());
                expected.set(String::new());
                done.run(());
            }
        });
    };
    view! {
        <form class="mb-2 rounded-sm border border-accent/30 bg-accent/5 p-2"
              on:submit=move |ev| { ev.prevent_default(); submit(); }>
            {move || {
                let options: Vec<(String, String)> =
                    std::iter::once((String::new(), "Who from…".to_owned()))
                        .chain(match people.get() {
                            Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                            _ => Vec::new(),
                        })
                        .collect();
                view! { <SelectField label="Waiting on" options=options current=who.get_untracked()
                                     on_change=move |v: String| who.set(v) /> }
            }}
            <label class="mb-2 block">
                <span class="mb-0.5 block text-[11px] text-muted">"What are you waiting for?"</span>
                <input class=INPUT prop:value=move || what.get()
                    on:input=move |ev| what.set(event_target_value(&ev)) />
            </label>
            <DateField label="Expected by (optional)" current=expected.get_untracked()
                on_commit=move |v: String| expected.set(v) />
            <div class="flex justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| done.run(())>"Cancel"</button>
                <button type="submit" class=BUTTON_PRIMARY disabled=move || busy.get()>"Add"</button>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    const TODAY: Option<Date> = Some(date!(2027 - 03 - 15));

    #[test]
    fn mark_blocked_is_offered_only_while_something_is_open_on_a_workable_task() {
        for s in [TaskStatus::Todo, TaskStatus::InProgress] {
            assert_eq!(status_offer(s, 1, None, TODAY), Some(Offer::MarkBlocked));
            assert_eq!(status_offer(s, 0, None, TODAY), None);
        }
        for s in [TaskStatus::Blocked, TaskStatus::Done, TaskStatus::Cancelled] {
            assert_ne!(status_offer(s, 2, None, TODAY), Some(Offer::MarkBlocked));
        }
    }

    #[test]
    fn moving_on_is_offered_only_to_a_blocked_task_whose_last_one_was_resolved_today() {
        let b = TaskStatus::Blocked;
        assert_eq!(
            status_offer(b, 0, TODAY, TODAY),
            Some(Offer::MoveToInProgress)
        );
        // Still something open, resolved earlier, never any, or the clock unreadable: no offer.
        assert_eq!(status_offer(b, 1, TODAY, TODAY), None);
        assert_eq!(status_offer(b, 0, Some(date!(2027 - 03 - 10)), TODAY), None);
        assert_eq!(status_offer(b, 0, None, TODAY), None);
        assert_eq!(status_offer(b, 0, TODAY, None), None);
        // Other statuses are never moved.
        assert_eq!(status_offer(TaskStatus::Todo, 0, TODAY, TODAY), None);
    }

    #[test]
    fn the_heading_note_counts_open_and_resolved() {
        assert_eq!(meta_text(0, 0), "");
        assert_eq!(meta_text(2, 0), "2 open");
        assert_eq!(meta_text(0, 3), "3 resolved");
        assert_eq!(meta_text(1, 3), "1 open, 3 resolved");
    }

    #[test]
    fn the_archive_note_counts_what_stays() {
        assert_eq!(archive_note(0), "");
        assert_eq!(archive_note(1), " 1 open waiting-on stays in Waiting On.");
        assert_eq!(archive_note(3), " 3 open waiting-ons stay in Waiting On.");
    }

    #[test]
    fn the_what_box_starts_with_the_task_title() {
        assert_eq!(default_what("Ship it"), "Waiting for Ship it");
    }

    #[test]
    fn expected_by_is_optional_but_must_be_a_date() {
        assert_eq!(parse_expected("  "), Ok(None));
        assert_eq!(
            parse_expected("2027-03-31"),
            Ok(Some(date!(2027 - 03 - 31)))
        );
        assert!(parse_expected("soon").is_err());
    }
}
