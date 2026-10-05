//! This week: the landing screen. What needs attention now, Monday to Sunday: overdue, due this
//! week, blocked, my work in progress, waiting-ons that are stale or due, and 1:1s. Rows can be
//! completed, rescheduled (tomorrow, next week, or a typed date like `fri` or `+3d`) or resolved
//! in place; the screen refreshes itself after each action.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    Date, NodeRef, NodeType, NoteRow, TaskStatus, ThisWeek as Week, UpdateTask, Uuid, WaitingOnRow,
    WeekDay, WeekTask,
};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_SUCCESS, COMPACT_INPUT},
        node_row::NodeRow,
        page::{EmptyState, GroupLabel, Hints, PageHeader, Tone, CHIP_STRONG},
        sync_status::SyncBanner,
        waiting_panel::age_text,
    },
    labels::{task_status_label, task_status_tone},
    state::{finish, DataVersion, ListNav, Toasts},
    timeline::day_text,
};

// ------------------------------------------------------------------ pure text

/// "2027-03-01 → 2027-03-07".
pub fn range_text(start: Date, end: Date) -> String {
    format!("{start} → {end}")
}

/// "1 day overdue" / "30 days overdue".
pub fn overdue_text(days: u32) -> String {
    format!("{days} day{} overdue", if days == 1 { "" } else { "s" })
}

/// "today", "tomorrow", or "Fri 2027-03-05".
pub fn due_text(due: Date, today: Date) -> String {
    match (due - today).whole_days() {
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        _ => day_text(due),
    }
}

/// "Mon", "Tue"...
pub fn weekday_short(d: Date) -> &'static str {
    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [d.weekday().number_days_from_monday() as usize]
}

/// What the strip shows for a day besides its date: "2 due · 1:1".
pub fn day_summary(d: &WeekDay) -> String {
    let mut parts = Vec::new();
    if d.tasks_due > 0 {
        parts.push(format!("{} due", d.tasks_due));
    }
    if d.waiting_expected > 0 {
        parts.push(format!("{} waiting", d.waiting_expected));
    }
    if d.one_on_ones > 0 {
        parts.push(if d.one_on_ones == 1 {
            "1:1".to_owned()
        } else {
            format!("{} 1:1s", d.one_on_ones)
        });
    }
    parts.join(" · ")
}

/// Total rows across the sections (for the empty state).
pub fn total_items(w: &Week) -> usize {
    w.overdue.len()
        + w.due_this_week.len()
        + w.blocked.len()
        + w.in_progress.len()
        + w.waiting.len()
        + w.one_on_ones.len()
}

// ------------------------------------------------------------------ the screen

#[component]
pub fn ThisWeek() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let week_start = RwSignal::new(Option::<Date>::None);
    let week = LocalResource::new(move || {
        version.track();
        api::get_this_week(week_start.get())
    });
    view! {
        <div class="flex h-full flex-col">
            {move || match week.get() {
                None => view! { <p class="p-6 text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
                Some(Ok(w)) => view! { <Body week=w week_start=week_start /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn Body(week: Week, week_start: RwSignal<Option<Date>>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();

    // Rows in display order, so j/k/Enter walk the whole screen.
    let mut nodes: Vec<NodeRef> = Vec::new();
    let task_nodes = |v: &[WeekTask]| -> Vec<NodeRef> {
        v.iter()
            .map(|t| NodeRef::new(NodeType::Task, t.id()))
            .collect()
    };
    nodes.extend(task_nodes(&week.overdue));
    nodes.extend(task_nodes(&week.due_this_week));
    nodes.extend(task_nodes(&week.blocked));
    nodes.extend(task_nodes(&week.in_progress));
    nodes.extend(
        week.waiting
            .iter()
            .map(|w| NodeRef::new(NodeType::WaitingOn, w.waiting.id)),
    );
    nodes.extend(
        week.one_on_ones
            .iter()
            .map(|n| NodeRef::new(NodeType::Note, n.id)),
    );
    list.set_items(nodes);

    // `x` completes the task (or resolves the waiting-on) under the cursor.
    list.on_row_key(move |key, node| {
        if key != "x" {
            return;
        }
        match node.node_type {
            NodeType::Task => complete(node.id, toasts, version),
            NodeType::WaitingOn => resolve(node.id, toasts, version),
            _ => {}
        }
    });

    let today = week.today;
    let (prev_start, next_start) = (week.prev_week_start, week.next_week_start);
    let current = week.is_current_week;
    let prev = move |_| week_start.set(Some(prev_start));
    let next = move |_| week_start.set(Some(next_start));
    let reset = move |_| week_start.set(None);

    let mut index = 0usize;
    let mut section = |title: &'static str, rows: &[WeekTask]| {
        if rows.is_empty() {
            return None;
        }
        let first = index;
        index += rows.len();
        let items = rows
            .iter()
            .enumerate()
            .map(|(i, t)| view! { <TaskRowView row=t.clone() index=first + i today=today /> })
            .collect_view();
        Some(view! { <GroupLabel label=title count=rows.len() /> {items} })
    };
    let overdue = section("Overdue", &week.overdue);
    let due = section(
        if current {
            "Due this week"
        } else {
            "Due in this week"
        },
        &week.due_this_week,
    );
    let blocked = section("Blocked", &week.blocked);
    let progress = section(
        if week.has_self {
            "My tasks in progress"
        } else {
            "Tasks in progress"
        },
        &week.in_progress,
    );
    let waiting = (!week.waiting.is_empty()).then(|| {
        let first = index;
        index += week.waiting.len();
        let items = week
            .waiting
            .iter()
            .enumerate()
            .map(|(i, w)| view! { <WaitingRowView row=w.clone() index=first + i /> })
            .collect_view();
        view! { <GroupLabel label="Waiting on: stale or due" count=week.waiting.len() /> {items} }
    });
    let ones = (!week.one_on_ones.is_empty()).then(|| {
        let first = index;
        let items = week
            .one_on_ones
            .iter()
            .enumerate()
            .map(|(i, n)| view! { <OneOnOneRow row=n.clone() index=first + i /> })
            .collect_view();
        view! { <GroupLabel label="1:1s this week" count=week.one_on_ones.len() /> {items} }
    });
    let empty = total_items(&week) == 0;
    let strip = week
        .days
        .iter()
        .map(|d| view! { <DayTile day=d.clone() /> })
        .collect_view();

    view! {
        <PageHeader icon="week" title="This week"
                    subtitle="What needs attention now, Monday to Sunday">
            <button class=BUTTON aria-label="Previous week" on:click=prev>"‹"</button>
            <span class="tabular-nums text-muted">{range_text(week.week_start, week.week_end)}</span>
            <button class=BUTTON aria-label="Next week" on:click=next>"›"</button>
            <button class=BUTTON disabled=current on:click=reset>"This week"</button>
            <Hints keys=&[("j/k", "move"), ("Enter", "open"), ("x", "done")] />
        </PageHeader>
        <div class="min-h-0 flex-1 overflow-y-auto">
            <SyncBanner />
            <div class="grid grid-cols-7 gap-1.5 border-b border-line px-4 py-3">{strip}</div>
            {overdue}{due}{blocked}{progress}{waiting}{ones}
            {empty.then(|| view! {
                <EmptyState icon="week" title="Nothing needs attention"
                    hint="No overdue work, nothing due, nothing blocked or stale. A clear week." />
            })}
            {(!week.has_self).then(|| view! {
                <p class="px-4 py-3 text-[11px] text-muted">
                    "Add yourself under People (first-run setup) to see just your tasks in progress."
                </p>
            })}
        </div>
    }
}

fn complete(id: Uuid, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        let patch = UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        };
        finish(api::update_task(id, patch).await, toasts, version);
    });
}

fn resolve(id: Uuid, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        finish(api::resolve_waiting_on(id).await, toasts, version);
    });
}

fn reschedule(id: Uuid, when: String, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        finish(api::reschedule_task(id, when).await, toasts, version);
    });
}

#[component]
fn DayTile(day: WeekDay) -> impl IntoView {
    let class = if day.is_today {
        "rounded-sm border border-accent bg-accent/10 px-2 py-1.5"
    } else {
        "rounded-sm border border-line bg-panel px-2 py-1.5"
    };
    let summary = day_summary(&day);
    let head = if day.is_today {
        "text-accent"
    } else {
        "text-muted"
    };
    view! {
        <div class=class>
            <div class=format!("flex items-baseline justify-between text-[10px] font-semibold uppercase tracking-wider {head}")>
                <span>{weekday_short(day.date)}</span>
                {day.is_today.then(|| view! { <span class="font-normal normal-case">"today"</span> })}
            </div>
            <div class="text-[15px] font-semibold tabular-nums leading-5">{day.date.day()}</div>
            <div class="h-4 truncate text-[11px] text-muted">{summary}</div>
        </div>
    }
}

/// Complete / reschedule controls, shown when the row is hovered or has focus inside it.
#[component]
pub(crate) fn TaskActions(
    id: Uuid,
    /// Always visible instead of on hover (the weekly review's quick fixes).
    #[prop(optional)]
    always: bool,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let typed = RwSignal::new(String::new());
    let go = move |when: &'static str| move |_| reschedule(id, when.to_owned(), toasts, version);
    let tomorrow = go("tomorrow");
    let next_week = go("next-week");
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" {
            let when = typed.get_untracked();
            if !when.trim().is_empty() {
                typed.set(String::new());
                reschedule(id, when, toasts, version);
            }
        }
    };
    view! {
        <span class=if always {
                  "ml-auto flex shrink-0 items-center gap-1"
              } else {
                  "ml-auto flex shrink-0 items-center gap-1 opacity-0 focus-within:opacity-100 group-hover:opacity-100"
              }
              on:click=|ev| ev.stop_propagation()>
            <button class=BUTTON title="Move the due date to tomorrow" on:click=tomorrow>"Tomorrow"</button>
            <button class=BUTTON title="Move the due date to next Monday" on:click=next_week>"Next week"</button>
            <input class=format!("{COMPACT_INPUT} w-24") placeholder="fri, +3d…"
                   title="Type a date: fri, next-wed, +3d, 2027-03-31, then Enter"
                   prop:value=move || typed.get()
                   on:input=move |ev| typed.set(event_target_value(&ev))
                   on:keydown=on_key />
        </span>
    }
}

#[component]
fn TaskRowView(row: WeekTask, index: usize, today: Date) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let t = row.row.task.clone();
    let id = t.id;
    let node = NodeRef::new(NodeType::Task, id);
    let project = row
        .row
        .project
        .as_ref()
        .map(|p| p.label.clone())
        .unwrap_or_default();
    let due = match (row.overdue_days, t.due_date) {
        (Some(n), _) => Some((overdue_text(n), true)),
        (None, Some(d)) => Some((due_text(d, today), false)),
        _ => None,
    };
    let blockers = if row.blocked_by.is_empty() {
        None
    } else {
        let names: Vec<&str> = row.blocked_by.iter().map(|b| b.label.as_str()).collect();
        Some(format!("blocked by {}", names.join(", ")))
    };
    let status = match t.status {
        TaskStatus::Blocked | TaskStatus::InProgress => {
            Some((task_status_tone(t.status), task_status_label(t.status)))
        }
        _ => None,
    };
    view! {
        <NodeRow node=node index=index>
            <button class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-line-strong \
                           text-[10px] text-transparent hover:border-success hover:text-success"
                    title="Mark done" aria-label="Mark done"
                    on:click=move |ev| { ev.stop_propagation(); complete(id, toasts, version); }>"✓"</button>
            <span class="min-w-0 truncate">
                <span class="font-medium">{t.title.clone()}</span>
                <span class="ml-2 text-muted">{project}</span>
            </span>
            {(t.priority <= 2).then(|| view! { <span class=CHIP_STRONG>{format!("P{}", t.priority)}</span> })}
            {status.map(|(tone, s)| view! { <span class=tone.chip()>{s}</span> })}
            {due.map(|(text, late)| view! {
                <span class=if late { "shrink-0 text-danger" } else { "shrink-0 tabular-nums text-muted" }>{text}</span>
            })}
            {blockers.map(|b| view! { <span class="min-w-0 truncate text-[11px] text-muted">{b}</span> })}
            <TaskActions id=id />
        </NodeRow>
    }
}

#[component]
fn WaitingRowView(row: WaitingOnRow, index: usize) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = row.waiting.id;
    let node = NodeRef::new(NodeType::WaitingOn, id);
    let expected = row
        .waiting
        .expected_by
        .map(|d| format!("expected {d}"))
        .unwrap_or_default();
    let (state, state_tone) = if row.stale {
        ("stale", Tone::Warning)
    } else {
        ("due", Tone::Accent)
    };
    view! {
        <NodeRow node=node index=index>
            <span class=state_tone.chip()>{state}</span>
            <span class="w-24 shrink-0 truncate text-muted">{row.person.label.clone()}</span>
            <span class="min-w-0 truncate font-medium">{row.waiting.description.clone()}</span>
            <span class="shrink-0 text-[11px] text-muted">{format!("waiting {}", age_text(row.age_days))}</span>
            <span class="shrink-0 text-[11px] tabular-nums text-muted">{expected}</span>
            <span class="ml-auto flex shrink-0 items-center gap-1 opacity-0 focus-within:opacity-100 group-hover:opacity-100"
                  on:click=|ev| ev.stop_propagation()>
                <button class=BUTTON_SUCCESS on:click=move |_| resolve(id, toasts, version)>"Resolve"</button>
                <button class=BUTTON title="Hide it for 3 days"
                        on:click=move |_| {
                            spawn_local(async move {
                                finish(api::snooze_waiting_on(id, Some(3)).await, toasts, version);
                            });
                        }>"Snooze 3 days"</button>
            </span>
        </NodeRow>
    }
}

#[component]
fn OneOnOneRow(row: NoteRow, index: usize) -> impl IntoView {
    let node = NodeRef::new(NodeType::Note, row.id);
    let people = row
        .mentions
        .iter()
        .map(|m| m.label.clone())
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <NodeRow node=node index=index>
            <span class="w-24 shrink-0 tabular-nums text-muted">{day_text(row.note_date)}</span>
            <span class="min-w-0 truncate font-medium">{row.title}</span>
            <span class="min-w-0 truncate text-muted">{people}</span>
        </NodeRow>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn dates_read_naturally() {
        let today = date!(2027 - 03 - 03);
        assert_eq!(due_text(today, today), "today");
        assert_eq!(due_text(date!(2027 - 03 - 04), today), "tomorrow");
        assert_eq!(due_text(date!(2027 - 03 - 05), today), "Fri 2027-03-05");
        assert_eq!(overdue_text(1), "1 day overdue");
        assert_eq!(overdue_text(30), "30 days overdue");
        assert_eq!(
            range_text(date!(2027 - 03 - 01), date!(2027 - 03 - 07)),
            "2027-03-01 → 2027-03-07"
        );
        assert_eq!(weekday_short(date!(2027 - 03 - 01)), "Mon");
        assert_eq!(weekday_short(date!(2027 - 03 - 07)), "Sun");
    }

    #[test]
    fn the_day_strip_summarises_what_is_on() {
        let d = |t, w, o| WeekDay {
            date: date!(2027 - 03 - 03),
            is_today: false,
            tasks_due: t,
            waiting_expected: w,
            one_on_ones: o,
        };
        assert_eq!(day_summary(&d(0, 0, 0)), "");
        assert_eq!(day_summary(&d(2, 0, 1)), "2 due · 1:1");
        assert_eq!(day_summary(&d(1, 3, 2)), "1 due · 3 waiting · 2 1:1s");
    }
}
