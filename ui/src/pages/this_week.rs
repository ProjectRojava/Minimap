//! This week: the landing screen. What needs attention now, Monday to Sunday: overdue, due this
//! week, blocked, my work in progress, waiting-ons that are stale or due, and 1:1s. Rows can be
//! completed, rescheduled (tomorrow, next week, or a typed date like `fri` or `+3d`) or resolved
//! in place; the screen refreshes itself after each action.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    Date, Flag, FlaggedTask, NodeRef, NodeType, NoteRow, TaskStatus, ThisWeek as Week, UpdateTask,
    Uuid, WaitingOnRow, WeekDay, WeekTask,
};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_SUCCESS, COMPACT_INPUT},
        node_row::NodeRow,
        page::{EmptyState, Hints, PageHeader, Tone, CHIP_STRONG},
        review_row::ReviewRow,
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
        + w.reviews.len()
}

/// How many red flags of each kind, and the rest of the week, for the tiles at the top.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub overdue: usize,
    pub due_today: usize,
    pub blocked: usize,
    pub planned: usize,
    pub waiting: usize,
}

pub fn counts(w: &Week) -> Counts {
    let with = |flag: Flag| {
        w.attention
            .iter()
            .filter(|f| f.flags.contains(&flag))
            .count()
    };
    Counts {
        overdue: with(Flag::Overdue),
        due_today: with(Flag::DueToday),
        blocked: with(Flag::Blocked),
        planned: w.priorities.len(),
        waiting: w.waiting.len(),
    }
}

/// The one line at the top that says how the week stands.
pub fn headline(w: &Week) -> String {
    match w.attention.len() {
        0 if w.priorities.is_empty() => "Nothing needs you this week".to_owned(),
        0 => "Nothing is late or stuck. Here is the plan".to_owned(),
        1 => "1 task needs your attention".to_owned(),
        n => format!("{n} tasks need your attention"),
    }
}

/// `d` moved by `n` days (unchanged if that leaves the calendar).
pub fn shift_days(d: Date, n: i32) -> Date {
    Date::from_julian_day(d.to_julian_day() + n).unwrap_or(d)
}

/// What the banner says while a past day is shown.
pub fn as_of_note(day: Date) -> String {
    format!(
        "Viewing the week as it stood on {}. Tasks finished since then show as open, and tasks made since are left out; due dates are as they are now.",
        day_text(day)
    )
}

/// "Thursday 8 October" for today's week, "As of Tuesday 2 March" for a past day, "Week of
/// 2026-10-12" for another week.
pub fn date_line(w: &Week) -> String {
    if !w.is_current_week {
        return format!("Week of {}", w.week_start);
    }
    const DAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let t = w.today;
    format!(
        "{}{} {} {}",
        if w.as_of.is_some() { "As of " } else { "" },
        DAYS[t.weekday().number_days_from_monday() as usize],
        t.day(),
        MONTHS[u8::from(t.month()) as usize - 1]
    )
}

/// The pill that says why a task is a red flag, and its classes (solid red for late, solid amber
/// for today, tinted amber for blocked).
pub fn flag_pill(flag: Flag, overdue_days: Option<u32>) -> (String, &'static str) {
    match flag {
        Flag::Overdue => (
            match overdue_days.unwrap_or(1) {
                1 => "1 day late".to_owned(),
                n => format!("{n} days late"),
            },
            "inline-flex h-5 shrink-0 items-center rounded-sm bg-danger px-1.5 text-[10px] font-bold uppercase tracking-wide text-canvas",
        ),
        Flag::DueToday => (
            "Due today".to_owned(),
            "inline-flex h-5 shrink-0 items-center rounded-sm bg-warning px-1.5 text-[10px] font-bold uppercase tracking-wide text-canvas",
        ),
        Flag::Blocked => (
            "Blocked".to_owned(),
            "inline-flex h-5 shrink-0 items-center rounded-sm border border-warning/60 bg-warning/15 px-1.5 text-[10px] font-bold uppercase tracking-wide text-warning",
        ),
    }
}

/// How a day of the strip looks: past days with open work due are late (red), today with work
/// due is amber, other days are quiet.
pub fn day_tone(d: &WeekDay, today: Date) -> Tone {
    match (d.date.cmp(&today), d.tasks_due) {
        (_, 0) => Tone::Neutral,
        (std::cmp::Ordering::Less, _) => Tone::Danger,
        (std::cmp::Ordering::Equal, _) => Tone::Warning,
        _ => Tone::Neutral,
    }
}

/// The words under a day of the strip: "2 late" for a past day, otherwise as `day_summary`.
pub fn day_line(d: &WeekDay, today: Date) -> String {
    if d.date < today && d.tasks_due > 0 {
        let mut text = format!("{} late", d.tasks_due);
        let rest = day_summary(&WeekDay {
            tasks_due: 0,
            ..d.clone()
        });
        if !rest.is_empty() {
            text.push_str(" · ");
            text.push_str(&rest);
        }
        return text;
    }
    day_summary(d)
}

// ------------------------------------------------------------------ the screen

#[component]
pub fn ThisWeek() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let week_start = RwSignal::new(Option::<Date>::None);
    // A past day to look back at: the screen shows the week as it stood then.
    let as_of = RwSignal::new(Option::<Date>::None);
    let week = LocalResource::new(move || {
        version.track();
        api::get_this_week(week_start.get(), as_of.get())
    });
    // The last week that loaded, so the screen never blanks while it refreshes after an action.
    let shown = RwSignal::new(None::<Week>);
    Effect::new(move |_| {
        if let Some(Ok(w)) = week.get() {
            shown.set(Some(w));
        }
    });
    view! {
        <div class="flex h-full flex-col">
            {move || match (shown.get(), week.get()) {
                (Some(w), _) => view! { <Body week=w week_start=week_start as_of=as_of /> }.into_any(),
                (None, Some(Err(e))) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
                (None, _) => view! { <p class="p-6 text-muted">"Loading…"</p> }.into_any(),
            }}
        </div>
    }
}

/// A card that holds one part of the week.
const CARD: &str = "mx-4 mt-4 overflow-hidden rounded-sm border border-line bg-panel [&>[role=row]:last-child]:border-b-0";

#[component]
fn CardHead(
    title: &'static str,
    count: usize,
    #[prop(optional)] hint: &'static str,
    #[prop(optional)] tone: Option<Tone>,
) -> impl IntoView {
    let tone = tone.unwrap_or(Tone::Neutral);
    view! {
        <div class="flex items-center gap-2 border-b border-line px-4 py-2">
            <span class=format!("h-2 w-2 shrink-0 rounded-full {}", tone.dot()) />
            <h2 class=format!("text-[11px] font-semibold uppercase tracking-wider {}",
                    if tone == Tone::Neutral { "text-fg" } else { tone.text() })>{title}</h2>
            <span class="rounded-sm border border-line px-1 text-[10px] leading-4 tabular-nums text-muted">{count}</span>
            {(!hint.is_empty()).then(|| view! { <span class="ml-1 truncate text-[11px] text-muted">{hint}</span> })}
        </div>
    }
}

#[component]
fn Body(
    week: Week,
    week_start: RwSignal<Option<Date>>,
    as_of: RwSignal<Option<Date>>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();

    // Rows in display order, so j/k/Enter walk the whole screen.
    let mut nodes: Vec<NodeRef> = Vec::new();
    nodes.extend(
        week.attention
            .iter()
            .map(|f| NodeRef::new(NodeType::Task, f.task.id())),
    );
    nodes.extend(
        week.priorities
            .iter()
            .map(|t| NodeRef::new(NodeType::Task, t.id())),
    );
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
    // Looking back, the arrows move the chosen day a week at a time (reaching today ends it).
    let looking_back = week.as_of;
    let real_today = week.real_today;
    let prev = move |_| match looking_back {
        Some(day) => as_of.set(Some(shift_days(day, -7))),
        None => week_start.set(Some(prev_start)),
    };
    let next = move |_| match looking_back {
        Some(day) => {
            let later = shift_days(day, 7);
            as_of.set((later < real_today).then_some(later));
        }
        None => week_start.set(Some(next_start)),
    };
    let reset = move |_| {
        week_start.set(None);
        as_of.set(None);
    };
    // Clicking a day of the strip: a past day shows the week as it stood then; today (or a later
    // day) is the normal view.
    let pick_day = Callback::new(move |day: Date| {
        if day < real_today {
            week_start.set(None);
            as_of.set(Some(day));
        } else {
            as_of.set(None);
        }
    });
    let back_to_today = looking_back.is_some();
    let c = counts(&week);

    let mut index = 0usize;
    let attention = if week.attention.is_empty() {
        view! {
            <div class="mx-4 mt-4 flex items-center gap-2 rounded-sm border border-success/40 bg-success/10 px-4 py-2.5 text-success">
                <span class="text-[14px] leading-none">"✓"</span>
                <span class="font-medium">"Nothing is overdue, due today or blocked."</span>
            </div>
        }
        .into_any()
    } else {
        let first = index;
        index += week.attention.len();
        let rows = week
            .attention
            .iter()
            .enumerate()
            .map(|(i, f)| view! { <FlagRow item=f.clone() index=first + i /> })
            .collect_view();
        view! {
            <section class="mx-4 mt-4 overflow-hidden rounded-sm border border-danger/60 bg-danger/5 [&>[role=row]:last-child]:border-b-0"
                     aria-label="Needs attention">
                <div class="flex items-center gap-2 border-b border-danger/40 bg-danger/10 px-4 py-2">
                    <span class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-danger text-[10px] font-bold text-canvas">"!"</span>
                    <h2 class="text-[11px] font-semibold uppercase tracking-wider text-danger">"Needs attention"</h2>
                    <span class="rounded-sm bg-danger px-1 text-[10px] font-bold leading-4 tabular-nums text-canvas">{week.attention.len()}</span>
                    <span class="ml-1 truncate text-[11px] text-muted">"Overdue, due today or blocked. Deal with these first."</span>
                </div>
                {rows}
            </section>
        }
        .into_any()
    };
    let priorities = (!week.priorities.is_empty()).then(|| {
        let first = index;
        index += week.priorities.len();
        let rows = week
            .priorities
            .iter()
            .enumerate()
            .map(|(i, t)| view! { <PlanRow row=t.clone() rank=i + 1 index=first + i today=today /> })
            .collect_view();
        view! {
            <section class=CARD aria-label="Priorities this week">
                <CardHead title=if current { "Priorities this week" } else { "Priorities that week" }
                    count=week.priorities.len() tone=Tone::Accent
                    hint="Due this week and your work in progress, most important first" />
                {rows}
            </section>
        }
    });
    let waiting = (!week.waiting.is_empty()).then(|| {
        let first = index;
        index += week.waiting.len();
        let items = week
            .waiting
            .iter()
            .enumerate()
            .map(|(i, w)| view! { <WaitingRowView row=w.clone() index=first + i /> })
            .collect_view();
        view! {
            <section class=CARD aria-label="Waiting on">
                <CardHead title="Waiting on" count=week.waiting.len() tone=Tone::Warning
                    hint="Stale, overdue or expected this week: chase or resolve" />
                {items}
            </section>
        }
    });
    let reviews = (!week.reviews.is_empty()).then(|| {
        let items = week
            .reviews
            .iter()
            .map(|r| view! { <ReviewRow item=r.clone() /> })
            .collect_view();
        view! {
            <section class=CARD aria-label="Objectives to review">
                <CardHead title="Ongoing objectives to review" count=week.reviews.len() />
                {items}
            </section>
        }
    });
    let ones = (!week.one_on_ones.is_empty()).then(|| {
        let first = index;
        let items = week
            .one_on_ones
            .iter()
            .enumerate()
            .map(|(i, n)| view! { <OneOnOneRow row=n.clone() index=first + i /> })
            .collect_view();
        view! {
            <section class=CARD aria-label="1:1s">
                <CardHead title="1:1s this week" count=week.one_on_ones.len() />
                {items}
            </section>
        }
    });
    let empty = total_items(&week) == 0;
    let strip = week
        .days
        .iter()
        .map(|d| view! { <DayTile day=d.clone() today=today real_today=real_today viewing=looking_back.is_some() on_pick=pick_day /> })
        .collect_view();
    let flagged = !week.attention.is_empty();

    view! {
        <PageHeader icon="week" title="This week"
                    subtitle="What needs attention now, Monday to Sunday">
            <button class=BUTTON aria-label="Previous week" on:click=prev>"‹"</button>
            <span class="tabular-nums text-muted">{range_text(week.week_start, week.week_end)}</span>
            <button class=BUTTON aria-label="Next week" on:click=next>"›"</button>
            <button class=BUTTON disabled=current && !back_to_today on:click=reset>"This week"</button>
            <Hints keys=&[("j/k", "move"), ("Enter", "open"), ("x", "done")] />
        </PageHeader>
        <div class="min-h-0 flex-1 overflow-y-auto pb-6">
            <SyncBanner />
            {week.as_of.map(|day| view! {
                <div class="mx-4 mt-4 flex flex-wrap items-center gap-3 rounded-sm border border-accent/50 bg-accent/10 px-4 py-2 text-accent">
                    <span class="min-w-0 flex-1 text-[12px]">{as_of_note(day)}</span>
                    <button class=BUTTON on:click=reset>"Back to today"</button>
                </div>
            })}
            <div class="flex flex-wrap items-end gap-x-6 gap-y-3 px-4 pt-5">
                <div class="min-w-0">
                    <p class="text-[11px] font-semibold uppercase tracking-wider text-muted">{date_line(&week)}</p>
                    <h1 class=format!("text-[20px] font-semibold leading-7 {}", if flagged { "text-danger" } else { "text-fg" })>
                        {headline(&week)}
                    </h1>
                </div>
                <div class="ml-auto flex flex-wrap gap-2">
                    <Stat label="Overdue" value=c.overdue tone=Tone::Danger />
                    <Stat label="Due today" value=c.due_today tone=Tone::Warning />
                    <Stat label="Blocked" value=c.blocked tone=Tone::Warning />
                    <Stat label="Planned" value=c.planned tone=Tone::Accent />
                    <Stat label="Waiting on" value=c.waiting tone=Tone::Neutral />
                </div>
            </div>
            <div class="mx-4 mt-4 grid grid-cols-7 gap-1.5" role="group" aria-label="Days of the week">{strip}</div>
            {attention}
            {priorities}
            {waiting}
            {reviews}
            {ones}
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

/// One number at the top: big and coloured when there is something, quiet at zero.
#[component]
fn Stat(label: &'static str, value: usize, tone: Tone) -> impl IntoView {
    let (box_class, number) = if value == 0 {
        ("border-line bg-panel", "text-faint")
    } else {
        match tone {
            Tone::Danger => ("border-danger/60 bg-danger/10", "text-danger"),
            Tone::Warning => ("border-warning/50 bg-warning/10", "text-warning"),
            Tone::Accent => ("border-accent/40 bg-accent/10", "text-accent"),
            _ => ("border-line bg-panel", "text-fg"),
        }
    };
    view! {
        <div class=format!("min-w-[5.5rem] rounded-sm border px-3 py-1.5 {box_class}")>
            <div class=format!("text-[20px] font-semibold leading-6 tabular-nums {number}")>{value}</div>
            <div class="text-[10px] font-semibold uppercase tracking-wider text-muted">{label}</div>
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

/// One day of the strip. Click a past day to look back at the week as it stood then, or today
/// to return; days after today are not clickable.
#[component]
fn DayTile(
    day: WeekDay,
    today: Date,
    real_today: Date,
    /// A past day is being viewed (`today` is that day).
    viewing: bool,
    on_pick: Callback<Date>,
) -> impl IntoView {
    let tone = day_tone(&day, today);
    let past = day.date < today;
    let date = day.date;
    let clickable = date <= real_today;
    let is_real_today = date == real_today;
    let base = if day.is_today {
        "rounded-sm border border-accent bg-accent/10 px-2.5 py-1.5 text-left"
    } else if tone == Tone::Danger {
        "rounded-sm border border-danger/60 bg-danger/10 px-2.5 py-1.5 text-left"
    } else if past {
        "rounded-sm border border-line bg-canvas px-2.5 py-1.5 text-left opacity-60"
    } else {
        "rounded-sm border border-line bg-panel px-2.5 py-1.5 text-left"
    };
    let class = if clickable {
        format!("{base} cursor-pointer hover:border-accent hover:opacity-100")
    } else {
        format!("{base} cursor-default")
    };
    let head = if day.is_today {
        "text-accent"
    } else {
        "text-muted"
    };
    // The viewed day says so; the real today keeps its label while a past day is shown.
    let label = if day.is_today && viewing {
        Some("viewing")
    } else if day.is_today || is_real_today {
        Some("today")
    } else {
        None
    };
    let title = if !clickable {
        String::new()
    } else if is_real_today {
        "Back to today".to_owned()
    } else {
        format!("See the week as it stood on {}", day_text(date))
    };
    let line = day_line(&day, today);
    let line_class = match tone {
        Tone::Danger => "text-danger font-semibold",
        Tone::Warning => "text-warning font-semibold",
        _ => "text-muted",
    };
    view! {
        <button type="button" class=class title=title disabled=!clickable
                aria-pressed=day.is_today.to_string()
                on:click=move |_| on_pick.run(date)>
            <div class=format!("flex items-baseline justify-between text-[10px] font-semibold uppercase tracking-wider {head}")>
                <span>{weekday_short(day.date)}</span>
                {label.map(|l| view! { <span class="font-normal normal-case">{l}</span> })}
            </div>
            <div class="text-[16px] font-semibold tabular-nums leading-6">{day.date.day()}</div>
            <div class=format!("h-4 truncate text-[11px] {line_class}")>{line}</div>
        </button>
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

/// "blocked by A, B" (nothing when no open task blocks it).
pub fn blocked_text(t: &WeekTask) -> Option<String> {
    if t.blocked_by.is_empty() {
        return None;
    }
    let names: Vec<&str> = t.blocked_by.iter().map(|b| b.label.as_str()).collect();
    Some(format!("waiting for {}", names.join(", ")))
}

/// A red-flag task: why first, in a pill nobody can miss, then the task.
#[component]
fn FlagRow(item: FlaggedTask, index: usize) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let t = item.task.row.task.clone();
    let id = t.id;
    let node = NodeRef::new(NodeType::Task, id);
    let project = item.task.row.project.as_ref().map(|p| p.label.clone());
    let assignee = item.task.row.assignee.as_ref().map(|a| a.label.clone());
    let pills = item
        .flags
        .iter()
        .map(|f| {
            let (text, class) = flag_pill(*f, item.task.overdue_days);
            view! { <span class=class>{text}</span> }
        })
        .collect_view();
    let blockers = blocked_text(&item.task);
    view! {
        <NodeRow node=node index=index>
            <button class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-line-strong \
                           text-[10px] text-transparent hover:border-success hover:text-success"
                    title="Mark done" aria-label="Mark done"
                    on:click=move |ev| { ev.stop_propagation(); complete(id, toasts, version); }>"✓"</button>
            <span class="flex w-44 shrink-0 items-center gap-1">{pills}</span>
            <span class="min-w-0 truncate">
                <span class="font-semibold">{t.title.clone()}</span>
                {project.map(|p| view! { <span class="ml-2 text-muted">{p}</span> })}
            </span>
            {(t.priority <= 2).then(|| view! { <span class=CHIP_STRONG>{format!("P{}", t.priority)}</span> })}
            {blockers.map(|b| view! { <span class="min-w-0 truncate text-[11px] text-warning">{b}</span> })}
            {assignee.map(|a| view! { <span class="shrink-0 text-[11px] text-muted">{a}</span> })}
            <TaskActions id=id />
        </NodeRow>
    }
}

/// A task of the week's plan, numbered by importance; the first three stand out.
#[component]
fn PlanRow(row: WeekTask, rank: usize, index: usize, today: Date) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let t = row.row.task.clone();
    let id = t.id;
    let node = NodeRef::new(NodeType::Task, id);
    let project = row.row.project.as_ref().map(|p| p.label.clone());
    let due = t.due_date.map(|d| {
        let soon = (d - today).whole_days() <= 1;
        (due_text(d, today), soon)
    });
    let status = (t.status == TaskStatus::InProgress)
        .then(|| (task_status_tone(t.status), task_status_label(t.status)));
    let top = rank <= 3;
    let number = if top {
        "flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent text-[10px] font-bold text-canvas"
    } else {
        "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-line text-[10px] tabular-nums text-muted"
    };
    view! {
        <NodeRow node=node index=index>
            <span class=number>{rank}</span>
            <button class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-line-strong \
                           text-[10px] text-transparent hover:border-success hover:text-success"
                    title="Mark done" aria-label="Mark done"
                    on:click=move |ev| { ev.stop_propagation(); complete(id, toasts, version); }>"✓"</button>
            <span class="min-w-0 truncate">
                <span class=if top { "font-semibold" } else { "font-medium" }>{t.title.clone()}</span>
                {project.map(|p| view! { <span class="ml-2 text-muted">{p}</span> })}
            </span>
            {(t.priority <= 2).then(|| view! { <span class=CHIP_STRONG>{format!("P{}", t.priority)}</span> })}
            {status.map(|(tone, s)| view! { <span class=tone.chip()>{s}</span> })}
            {due.map(|(text, soon)| view! {
                <span class=if soon { "shrink-0 text-[12px] font-medium text-warning" } else { "shrink-0 text-[12px] tabular-nums text-muted" }>{text}</span>
            })}
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
        assert_eq!(
            range_text(date!(2027 - 03 - 01), date!(2027 - 03 - 07)),
            "2027-03-01 → 2027-03-07"
        );
        assert_eq!(weekday_short(date!(2027 - 03 - 01)), "Mon");
        assert_eq!(weekday_short(date!(2027 - 03 - 07)), "Sun");
    }

    fn task(n: u128, status: TaskStatus) -> WeekTask {
        WeekTask {
            row: minimap_types::TaskRow {
                task: minimap_types::Task {
                    id: Uuid::from_u128(n),
                    title: format!("T{n}"),
                    description: String::new(),
                    project_id: None,
                    status,
                    estimate_days: None,
                    start_date: None,
                    due_date: None,
                    completed_at: None,
                    priority: 3,
                    recurrence: None,
                    links: Vec::new(),
                    task_type: None,
                    created_at: time::OffsetDateTime::UNIX_EPOCH,
                    updated_at: time::OffsetDateTime::UNIX_EPOCH,
                    archived_at: None,
                },
                project: None,
                assignee: None,
                link_count: 0,
                attachment_count: 0,
            },
            overdue_days: None,
            blocked_by: Vec::new(),
        }
    }

    fn week(attention: Vec<FlaggedTask>, priorities: Vec<WeekTask>, current: bool) -> Week {
        Week {
            week_start: date!(2027 - 03 - 01),
            week_end: date!(2027 - 03 - 07),
            prev_week_start: date!(2027 - 02 - 22),
            next_week_start: date!(2027 - 03 - 08),
            today: date!(2027 - 03 - 04),
            as_of: None,
            real_today: date!(2027 - 03 - 04),
            is_current_week: current,
            has_self: true,
            days: Vec::new(),
            overdue: Vec::new(),
            due_this_week: Vec::new(),
            blocked: Vec::new(),
            in_progress: Vec::new(),
            waiting: Vec::new(),
            one_on_ones: Vec::new(),
            reviews: Vec::new(),
            attention,
            priorities,
        }
    }

    #[test]
    fn the_top_counts_each_kind_of_red_flag_and_says_how_the_week_stands() {
        let flagged = |n, flags: Vec<Flag>| FlaggedTask {
            task: task(n, TaskStatus::Todo),
            flags,
        };
        let w = week(
            vec![
                flagged(1, vec![Flag::Overdue, Flag::Blocked]),
                flagged(2, vec![Flag::DueToday]),
                flagged(3, vec![Flag::Blocked]),
            ],
            vec![task(4, TaskStatus::Todo)],
            true,
        );
        assert_eq!(
            counts(&w),
            Counts {
                overdue: 1,
                due_today: 1,
                blocked: 2,
                planned: 1,
                waiting: 0
            }
        );
        assert_eq!(headline(&w), "3 tasks need your attention");
        assert_eq!(date_line(&w), "Thursday 4 March");
        let calm = week(Vec::new(), vec![task(4, TaskStatus::Todo)], false);
        assert_eq!(
            headline(&calm),
            "Nothing is late or stuck. Here is the plan"
        );
        assert_eq!(date_line(&calm), "Week of 2027-03-01");
        let mut back = week(Vec::new(), Vec::new(), true);
        back.as_of = Some(back.today);
        assert_eq!(date_line(&back), "As of Thursday 4 March");
        assert!(as_of_note(date!(2027 - 03 - 02)).contains("Tue 2027-03-02"));
        assert_eq!(shift_days(date!(2027 - 03 - 02), -7), date!(2027 - 02 - 23));
        assert_eq!(shift_days(date!(2027 - 03 - 02), 7), date!(2027 - 03 - 09));
        assert_eq!(
            headline(&week(Vec::new(), Vec::new(), true)),
            "Nothing needs you this week"
        );
    }

    #[test]
    fn red_flag_pills_say_why_and_late_is_the_loudest() {
        let (text, class) = flag_pill(Flag::Overdue, Some(3));
        assert_eq!(text, "3 days late");
        assert!(class.contains("bg-danger") && class.contains("text-canvas"));
        assert_eq!(flag_pill(Flag::Overdue, Some(1)).0, "1 day late");
        let (text, class) = flag_pill(Flag::DueToday, None);
        assert_eq!(text, "Due today");
        assert!(class.contains("bg-warning "));
        assert!(flag_pill(Flag::Blocked, None).1.contains("text-warning"));
    }

    #[test]
    fn past_days_with_open_work_are_late_and_today_is_amber() {
        let today = date!(2027 - 03 - 04);
        let d = |date, due| WeekDay {
            date,
            is_today: date == today,
            tasks_due: due,
            waiting_expected: 0,
            one_on_ones: 1,
        };
        assert_eq!(day_tone(&d(date!(2027 - 03 - 02), 2), today), Tone::Danger);
        assert_eq!(
            day_line(&d(date!(2027 - 03 - 02), 2), today),
            "2 late · 1:1"
        );
        assert_eq!(day_tone(&d(today, 1), today), Tone::Warning);
        assert_eq!(day_tone(&d(date!(2027 - 03 - 05), 4), today), Tone::Neutral);
        assert_eq!(day_line(&d(date!(2027 - 03 - 05), 4), today), "4 due · 1:1");
        assert_eq!(day_tone(&d(date!(2027 - 03 - 02), 0), today), Tone::Neutral);
    }

    #[test]
    fn a_blocked_task_says_what_it_waits_for() {
        let mut t = task(1, TaskStatus::Blocked);
        assert_eq!(blocked_text(&t), None);
        t.blocked_by = vec![minimap_types::NodeSummary {
            node: NodeRef::new(NodeType::Task, Uuid::from_u128(9)),
            label: "Get the certificate".into(),
            archived: false,
        }];
        assert_eq!(
            blocked_text(&t).as_deref(),
            Some("waiting for Get the certificate")
        );
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
