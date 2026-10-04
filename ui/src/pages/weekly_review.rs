//! Weekly review (spec 19): a guided look back at the week, one screen per question (`n` and
//! `p` move between them): what slipped, what got blocked, who is overloaded, what is stale
//! in waiting-on, which decisions were made, what got done. Rows allow quick fixes in place
//! (reschedule, reassign, unblock, resolve, snooze). The last step shows the status report made
//! from the template in Settings, to save as a Markdown file or copy.

use leptos::{ev, html, prelude::*, task::spawn_local, web_sys};
use leptos_router::hooks::use_navigate;
use minimap_types::{
    Date, DecisionRow, NodeRef, NodeType, OverloadedPerson, ReviewBlocked, ReviewDone,
    ReviewFinished, ReviewSlip, SlipKind, TaskStatus, UpdateTask, Uuid, WaitingOnRow, WeeklyReview,
};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_ON, BUTTON_PRIMARY, BUTTON_SUCCESS},
        health::load_text,
        node_row::NodeRow,
        page::{EmptyState, GroupLabel, Hints, PageHeader, Tone},
        waiting_panel::age_text,
    },
    labels::{decision_status_label, decision_status_tone},
    nav::is_typing_target,
    pages::this_week::{range_text, TaskActions},
    state::{finish, DataVersion, ListNav, Toasts},
    timeline::day_text,
};

// ------------------------------------------------------------------ pure text

pub const STEPS: [&str; 7] = [
    "Slipped",
    "Blocked",
    "Overloaded",
    "Waiting on",
    "Decisions",
    "Done",
    "Report",
];
const LAST_STEP: usize = STEPS.len() - 1;

/// The step `delta` away from `current`, staying inside the list.
pub fn move_step(current: usize, delta: i32) -> usize {
    (current as i64 + i64::from(delta)).clamp(0, LAST_STEP as i64) as usize
}

/// What each step asks, shown above its rows.
pub fn step_question(step: usize) -> &'static str {
    match step {
        0 => "What slipped this week? Due dates moved later, work that came due and is still open, targets pushed out.",
        1 => "What is blocked? Unblock it, reassign it, or move the date. Newly blocked work comes first.",
        2 => "Who is overloaded, this week or next, or has too many open tasks?",
        3 => "Which waiting-ons are stale? Resolve what arrived, snooze what can wait.",
        4 => "Which decisions were made this week?",
        5 => "What got done this week?",
        _ => "The status report for a board or executive audience. Save it or copy it.",
    }
}

/// How many rows each step has (the Report step has none).
pub fn step_counts(r: &WeeklyReview) -> [usize; 7] {
    [
        r.slipped.len(),
        r.blocked.len(),
        r.overloaded.len(),
        r.waiting.len() + r.waiting_resolved.len(),
        r.decisions.len(),
        r.done.len() + r.finished.len(),
        0,
    ]
}

/// The nodes of a step's rows in display order (for `j`/`k`/`Enter`).
pub fn step_nodes(r: &WeeklyReview, step: usize) -> Vec<NodeRef> {
    match step {
        0 => r.slipped.iter().map(|s| s.node.node).collect(),
        1 => r
            .blocked
            .iter()
            .map(|b| NodeRef::new(NodeType::Task, b.task.id()))
            .collect(),
        2 => r.overloaded.iter().map(|p| p.person.node).collect(),
        3 => r
            .waiting
            .iter()
            .chain(r.waiting_resolved.iter())
            .map(|w| NodeRef::new(NodeType::WaitingOn, w.waiting.id))
            .collect(),
        4 => r
            .decisions
            .iter()
            .map(|d| NodeRef::new(NodeType::Decision, d.id))
            .collect(),
        5 => r
            .finished
            .iter()
            .map(|f| f.node.node)
            .chain(r.done.iter().map(|d| d.node.node))
            .collect(),
        _ => Vec::new(),
    }
}

/// "1 working day" / "3 working days".
pub fn working_days_text(n: u32) -> String {
    format!("{n} working day{}", if n == 1 { "" } else { "s" })
}

/// The pill and the sentence for a slip.
pub fn slip_chip(s: &ReviewSlip) -> (Tone, &'static str) {
    match s.kind {
        SlipKind::DueMoved => (Tone::Warning, "moved later"),
        SlipKind::Overdue => (Tone::Danger, "overdue"),
        SlipKind::TargetMoved => (Tone::Warning, "target moved"),
    }
}

pub fn slip_detail(s: &ReviewSlip) -> String {
    match (s.kind, s.from, s.to) {
        (SlipKind::Overdue, _, Some(due)) => format!(
            "due {} · {} overdue",
            day_text(due),
            age_text(i64::from(s.days))
        ),
        (_, Some(from), Some(to)) => format!(
            "{} → {} · {}",
            day_text(from),
            day_text(to),
            working_days_text(s.days)
        ),
        _ => String::new(),
    }
}

/// "API launch · Priya" for a task row, or the kind of thing for a project or objective.
pub fn slip_context(s: &ReviewSlip) -> String {
    match s.node.node.node_type {
        NodeType::Task => [
            s.project.as_deref(),
            s.assignee.as_ref().map(|a| a.label.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · "),
        NodeType::Objective => "objective".to_owned(),
        _ => "project".to_owned(),
    }
}

/// "blocked by A, B" or "no open blocker recorded".
pub fn blocked_by_text(b: &ReviewBlocked) -> String {
    if b.task.blocked_by.is_empty() {
        "no open blocker recorded".to_owned()
    } else {
        let names: Vec<&str> = b.task.blocked_by.iter().map(|n| n.label.as_str()).collect();
        format!("blocked by {}", names.join(", "))
    }
}

/// The suggested file name for a week's report.
pub fn export_name(week_start: Date) -> String {
    format!("status-report-{week_start}.md")
}

fn people_options(people: &[(String, String)]) -> Vec<(String, String)> {
    std::iter::once((String::new(), "Unassigned".to_owned()))
        .chain(people.iter().cloned())
        .collect()
}

// ------------------------------------------------------------------ the screen

#[component]
pub fn WeeklyReview() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let week_start = RwSignal::new(Option::<Date>::None);
    let step = RwSignal::new(0usize);
    let review = LocalResource::new(move || {
        version.track();
        api::get_weekly_review(week_start.get())
    });
    let people = LocalResource::new(api::list_people);
    view! {
        <div class="flex h-full flex-col">
            {move || {
                let options: Vec<(String, String)> = match people.get() {
                    Some(Ok(p)) => p
                        .into_iter()
                        .map(|p| (p.person.id.to_string(), p.person.name))
                        .collect(),
                    _ => Vec::new(),
                };
                match review.get() {
                    None => view! { <p class="p-6 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
                    Some(Ok(r)) => view! {
                        <Body review=r people=options week_start=week_start step=step />
                    }.into_any(),
                }
            }}
        </div>
    }
}

#[component]
fn Body(
    review: WeeklyReview,
    people: Vec<(String, String)>,
    week_start: RwSignal<Option<Date>>,
    step: RwSignal<usize>,
) -> impl IntoView {
    let list = expect_context::<ListNav>();
    let review = StoredValue::new(review);
    let people = StoredValue::new(people);

    // j/k/Enter walk the rows of the step being shown.
    Effect::new(move |_| {
        let s = step.get();
        list.set_items(review.with_value(|r| step_nodes(r, s)));
    });
    // n / p move between the steps (not while typing).
    let handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let (tag, editable) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            .map(|el| (el.tag_name(), el.is_content_editable()))
            .unwrap_or_default();
        if is_typing_target(&tag, editable) {
            return;
        }
        match e.key().as_str() {
            "n" => step.update(|s| *s = move_step(*s, 1)),
            "p" => step.update(|s| *s = move_step(*s, -1)),
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());

    let (week_start_day, week_end_day) = review.with_value(|r| (r.week_start, r.week_end));
    let (prev_start, next_start) = review.with_value(|r| (r.prev_week_start, r.next_week_start));
    let current = review.with_value(|r| r.is_current_week);
    let counts = review.with_value(step_counts);
    let prev = move |_| week_start.set(Some(prev_start));
    let next = move |_| week_start.set(Some(next_start));
    let reset = move |_| week_start.set(None);

    let tabs = STEPS
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let class = move || {
                format!(
                    "{BUTTON} flex items-center gap-1.5 {}",
                    if step.get() == i { BUTTON_ON } else { "" }
                )
            };
            let count = counts[i];
            view! {
                <button class=class on:click=move |_| step.set(i)>
                    <span class="tabular-nums text-faint">{i + 1}</span>
                    {*name}
                    {(count > 0).then(|| view! {
                        <span class="rounded-sm border border-line px-1 text-[10px] leading-4 text-muted">{count}</span>
                    })}
                </button>
            }
        })
        .collect_view();

    let back = move |_| step.update(|s| *s = move_step(*s, -1));
    let forward = move |_| step.update(|s| *s = move_step(*s, 1));
    let content = move || {
        let s = step.get();
        review.with_value(|r| match s {
            0 => slipped_step(r, people.get_value()).into_any(),
            1 => blocked_step(r, people.get_value()).into_any(),
            2 => overloaded_step(r).into_any(),
            3 => waiting_step(r).into_any(),
            4 => decisions_step(r).into_any(),
            5 => done_step(r).into_any(),
            _ => view! { <ReportStep week_start=r.week_start /> }.into_any(),
        })
    };

    view! {
        <PageHeader icon="review" title="Weekly review"
                    subtitle="A short look back at the week, ending in a status report">
            <button class=BUTTON aria-label="Previous week" on:click=prev>"‹"</button>
            <span class="tabular-nums text-muted">{range_text(week_start_day, week_end_day)}</span>
            <button class=BUTTON aria-label="Next week" on:click=next>"›"</button>
            <button class=BUTTON disabled=current on:click=reset>"This week"</button>
            <Hints keys=&[("n/p", "next/previous step"), ("j/k", "move"), ("Enter", "open")] />
        </PageHeader>
        <nav class="flex shrink-0 flex-wrap items-center gap-1 border-b border-line px-4 py-2"
             aria-label="Review steps">{tabs}</nav>
        <p class="shrink-0 border-b border-line px-4 py-2 text-[12px] text-muted">
            {move || step_question(step.get())}
        </p>
        <div class="min-h-0 flex-1 overflow-y-auto">{content}</div>
        <footer class="flex shrink-0 items-center gap-2 border-t border-line bg-panel px-4 py-2">
            <button class=BUTTON disabled=move || step.get() == 0 on:click=back>"‹ Back"</button>
            <span class="text-[11px] text-muted">
                {move || format!("Step {} of {}: {}", step.get() + 1, STEPS.len(), STEPS[step.get()])}
            </span>
            <button class=BUTTON_PRIMARY disabled=move || step.get() == LAST_STEP on:click=forward>
                "Next ›"
            </button>
        </footer>
    }
}

// ------------------------------------------------------------------ the steps

fn slipped_step(r: &WeeklyReview, people: Vec<(String, String)>) -> impl IntoView {
    if r.slipped.is_empty() {
        return view! {
            <EmptyState icon="review" title="Nothing slipped"
                hint="No due dates moved later, nothing came due and stayed open, no targets pushed out." />
        }
        .into_any();
    }
    r.slipped
        .iter()
        .enumerate()
        .map(|(i, s)| view! { <SlipRow slip=s.clone() index=i people=people.clone() /> })
        .collect_view()
        .into_any()
}

fn blocked_step(r: &WeeklyReview, people: Vec<(String, String)>) -> impl IntoView {
    if r.blocked.is_empty() {
        return view! {
            <EmptyState icon="review" title="Nothing is blocked" hint="No task has the Blocked status." />
        }
        .into_any();
    }
    r.blocked
        .iter()
        .enumerate()
        .map(|(i, b)| view! { <BlockedRow item=b.clone() index=i people=people.clone() /> })
        .collect_view()
        .into_any()
}

fn overloaded_step(r: &WeeklyReview) -> impl IntoView {
    if r.overloaded.is_empty() {
        return view! {
            <EmptyState icon="review" title="Nobody is overloaded"
                hint="Everyone is within capacity this week and next, and under the open-task limit." />
        }
        .into_any();
    }
    r.overloaded
        .iter()
        .enumerate()
        .map(|(i, p)| view! { <LoadRow person=p.clone() index=i /> })
        .collect_view()
        .into_any()
}

fn waiting_step(r: &WeeklyReview) -> impl IntoView {
    if r.waiting.is_empty() && r.waiting_resolved.is_empty() {
        return view! {
            <EmptyState icon="review" title="Nothing stale"
                hint="No waiting-on is overdue or older than the stale threshold, and none was resolved this week." />
        }
        .into_any();
    }
    let stale_count = r.waiting.len();
    let stale = (!r.waiting.is_empty()).then(|| {
        let rows = r
            .waiting
            .iter()
            .enumerate()
            .map(|(i, w)| view! { <StaleRow row=w.clone() index=i /> })
            .collect_view();
        view! { <GroupLabel label="Stale" count=stale_count /> {rows} }
    });
    let resolved = (!r.waiting_resolved.is_empty()).then(|| {
        let rows = r
            .waiting_resolved
            .iter()
            .enumerate()
            .map(|(i, w)| view! { <ResolvedRow row=w.clone() index=stale_count + i /> })
            .collect_view();
        view! { <GroupLabel label="Resolved this week" count=r.waiting_resolved.len() /> {rows} }
    });
    view! { {stale}{resolved} }.into_any()
}

fn decisions_step(r: &WeeklyReview) -> impl IntoView {
    if r.decisions.is_empty() {
        return view! {
            <EmptyState icon="review" title="No decisions this week"
                hint="Decisions you mark as decided during the week appear here. Log them under Decisions." />
        }
        .into_any();
    }
    r.decisions
        .iter()
        .enumerate()
        .map(|(i, d)| view! { <DecisionLine row=d.clone() index=i /> })
        .collect_view()
        .into_any()
}

fn done_step(r: &WeeklyReview) -> impl IntoView {
    if r.done.is_empty() && r.finished.is_empty() {
        return view! {
            <EmptyState icon="review" title="Nothing finished yet"
                hint="Tasks completed this week, and projects marked done, appear here." />
        }
        .into_any();
    }
    let finished_count = r.finished.len();
    let finished = (!r.finished.is_empty()).then(|| {
        let rows = r
            .finished
            .iter()
            .enumerate()
            .map(|(i, f)| view! { <FinishedRow item=f.clone() index=i /> })
            .collect_view();
        view! { <GroupLabel label="Finished" count=finished_count /> {rows} }
    });
    let tasks = (!r.done.is_empty()).then(|| {
        let rows = r
            .done
            .iter()
            .enumerate()
            .map(|(i, d)| view! { <DoneRow item=d.clone() index=finished_count + i /> })
            .collect_view();
        view! { <GroupLabel label="Tasks completed" count=r.done.len() /> {rows} }
    });
    view! { {finished}{tasks} }.into_any()
}

// ------------------------------------------------------------------ rows

/// Reassign a task from a row.
#[component]
fn AssigneePick(task: Uuid, current: Option<Uuid>, people: Vec<(String, String)>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let on_change = move |v: String| {
        let person = Uuid::parse_str(&v).ok();
        spawn_local(async move {
            finish(api::set_assignee(task, person).await, toasts, version);
        });
    };
    view! {
        <span on:click=|ev| ev.stop_propagation()>
            <SelectField compact=true options=people_options(&people)
                current=current.map(|u| u.to_string()).unwrap_or_default() on_change=on_change />
        </span>
    }
}

#[component]
fn SlipRow(slip: ReviewSlip, index: usize, people: Vec<(String, String)>) -> impl IntoView {
    let node = slip.node.node;
    let is_task = node.node_type == NodeType::Task;
    let (tone, chip) = slip_chip(&slip);
    let detail = slip_detail(&slip);
    let context = slip_context(&slip);
    let assignee = slip.assignee.as_ref().map(|a| a.node.id);
    view! {
        <NodeRow node=node index=index>
            <span class=tone.chip()>{chip}</span>
            <span class="min-w-0 truncate">
                <span class="font-medium">{slip.node.label.clone()}</span>
                <span class="ml-2 text-muted">{context}</span>
            </span>
            <span class="shrink-0 text-[11px] tabular-nums text-muted">{detail}</span>
            {is_task.then(|| view! {
                <TaskActions id=node.id always=true />
                <AssigneePick task=node.id current=assignee people=people.clone() />
            })}
        </NodeRow>
    }
}

#[component]
fn BlockedRow(item: ReviewBlocked, index: usize, people: Vec<(String, String)>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let row = item.task.row.clone();
    let id = row.task.id;
    let node = NodeRef::new(NodeType::Task, id);
    let by = blocked_by_text(&item);
    let context = [
        row.project.as_ref().map(|p| p.label.as_str()),
        row.assignee.as_ref().map(|a| a.label.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let assignee = row.assignee.as_ref().map(|a| a.node.id);
    let unblock = move |_| {
        spawn_local(async move {
            let patch = UpdateTask {
                status: Some(TaskStatus::Todo),
                ..Default::default()
            };
            finish(api::update_task(id, patch).await, toasts, version);
        });
    };
    view! {
        <NodeRow node=node index=index>
            {item.newly_blocked.then(|| view! { <span class=Tone::Warning.chip()>"new"</span> })}
            <span class="min-w-0 truncate">
                <span class="font-medium">{row.task.title.clone()}</span>
                <span class="ml-2 text-muted">{context}</span>
            </span>
            <span class="min-w-0 truncate text-[11px] text-muted">{by}</span>
            <span class="ml-auto flex shrink-0 items-center gap-1" on:click=|ev| ev.stop_propagation()>
                <button class=BUTTON_SUCCESS title="Set the task back to To do" on:click=unblock>"Unblock"</button>
            </span>
            <TaskActions id=id always=true />
            <AssigneePick task=id current=assignee people=people />
        </NodeRow>
    }
}

#[component]
fn LoadRow(person: OverloadedPerson, index: usize) -> impl IntoView {
    let navigate = use_navigate();
    let node = person.person.node;
    let text = load_text(&person);
    let over = person.load_pct > 100.0;
    view! {
        <NodeRow node=node index=index>
            <span class=if over { Tone::Danger.chip() } else { Tone::Warning.chip() }>
                {if over { "over capacity" } else { "many tasks" }}
            </span>
            <span class="min-w-0 truncate font-medium">{person.person.label.clone()}</span>
            <span class="shrink-0 text-[11px] text-muted">{text}</span>
            <span class="ml-auto shrink-0" on:click=|ev| ev.stop_propagation()>
                <button class=BUTTON title="See which tasks make up the load"
                        on:click=move |_| navigate("/capacity", Default::default())>
                    "Open Capacity"
                </button>
            </span>
        </NodeRow>
    }
}

#[component]
fn StaleRow(row: WaitingOnRow, index: usize) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = row.waiting.id;
    let node = NodeRef::new(NodeType::WaitingOn, id);
    let expected = row
        .waiting
        .expected_by
        .map(|d| format!("expected {d}"))
        .unwrap_or_default();
    let (state, tone) = if row.overdue {
        ("overdue", Tone::Danger)
    } else {
        ("stale", Tone::Warning)
    };
    view! {
        <NodeRow node=node index=index>
            <span class=tone.chip()>{state}</span>
            <span class="w-24 shrink-0 truncate text-muted">{row.person.label.clone()}</span>
            <span class="min-w-0 truncate font-medium">{row.waiting.description.clone()}</span>
            <span class="shrink-0 text-[11px] text-muted">{format!("waiting {}", age_text(row.age_days))}</span>
            <span class="shrink-0 text-[11px] tabular-nums text-muted">{expected}</span>
            <span class="ml-auto flex shrink-0 items-center gap-1" on:click=|ev| ev.stop_propagation()>
                <button class=BUTTON_SUCCESS
                        on:click=move |_| spawn_local(async move {
                            finish(api::resolve_waiting_on(id).await, toasts, version);
                        })>"Resolve"</button>
                <button class=BUTTON title="Hide it for 3 days"
                        on:click=move |_| spawn_local(async move {
                            finish(api::snooze_waiting_on(id, Some(3)).await, toasts, version);
                        })>"Snooze 3 days"</button>
            </span>
        </NodeRow>
    }
}

#[component]
fn ResolvedRow(row: WaitingOnRow, index: usize) -> impl IntoView {
    let node = NodeRef::new(NodeType::WaitingOn, row.waiting.id);
    let on = row.waiting.resolved_on.map(day_text).unwrap_or_default();
    view! {
        <NodeRow node=node index=index>
            <span class=Tone::Success.chip()>"resolved"</span>
            <span class="w-24 shrink-0 truncate text-muted">{row.person.label.clone()}</span>
            <span class="min-w-0 truncate">{row.waiting.description.clone()}</span>
            <span class="ml-auto shrink-0 text-[11px] tabular-nums text-muted">{on}</span>
        </NodeRow>
    }
}

#[component]
fn DecisionLine(row: DecisionRow, index: usize) -> impl IntoView {
    let node = NodeRef::new(NodeType::Decision, row.id);
    let affects = row
        .affects
        .iter()
        .map(|a| a.label.clone())
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <NodeRow node=node index=index>
            <span class=decision_status_tone(row.status).chip()>{decision_status_label(row.status)}</span>
            <span class="w-28 shrink-0 tabular-nums text-muted">
                {row.decided_on.map(day_text).unwrap_or_default()}
            </span>
            <span class="min-w-0 truncate font-medium">{row.title.clone()}</span>
            <span class="min-w-0 truncate text-muted">{affects}</span>
        </NodeRow>
    }
}

#[component]
fn FinishedRow(item: ReviewFinished, index: usize) -> impl IntoView {
    let what = if item.node.node.node_type == NodeType::Objective {
        "objective"
    } else {
        "project"
    };
    view! {
        <NodeRow node=item.node.node index=index>
            <span class=Tone::Success.chip()>"finished"</span>
            <span class="min-w-0 truncate">
                <span class="font-medium">{item.node.label.clone()}</span>
                <span class="ml-2 text-muted">{what}</span>
            </span>
            <span class="ml-auto shrink-0 text-[11px] tabular-nums text-muted">{day_text(item.on)}</span>
        </NodeRow>
    }
}

#[component]
fn DoneRow(item: ReviewDone, index: usize) -> impl IntoView {
    let context = [item.project.as_deref(), item.assignee.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    view! {
        <NodeRow node=item.node.node index=index>
            <span class=Tone::Success.chip()>"done"</span>
            <span class="min-w-0 truncate">
                <span class="font-medium">{item.node.label.clone()}</span>
                <span class="ml-2 text-muted">{context}</span>
            </span>
            <span class="ml-auto shrink-0 text-[11px] tabular-nums text-muted">{day_text(item.completed_on)}</span>
        </NodeRow>
    }
}

// ------------------------------------------------------------------ the report

#[component]
fn ReportStep(week_start: Date) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let navigate = use_navigate();
    let report = LocalResource::new(move || {
        version.track();
        api::render_report(Some(week_start))
    });
    let area = leptos::prelude::NodeRef::<html::Textarea>::new();

    let save = move |_| {
        spawn_local(async move {
            let path = match api::save_dialog(&export_name(week_start)).await {
                Ok(Some(path)) => path,
                Ok(None) => return, // cancelled
                Err(e) => return toasts.error(&e),
            };
            match api::export_markdown(Some(week_start), path).await {
                Ok(done) => toasts.info(format!("Saved the report to {}", done.path)),
                Err(e) => toasts.error(&e),
            }
        });
    };
    let copy = move |_| {
        let Some(Ok(text)) = report.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match api::copy_text(&text).await {
                Ok(()) => toasts.info("Copied the report to the clipboard"),
                Err(_) => {
                    // No clipboard access: select the text so Ctrl+C works.
                    if let Some(a) = area.get_untracked() {
                        a.select();
                    }
                    toasts.info("Couldn't copy by itself. The report is selected: press Ctrl+C.");
                }
            }
        });
    };
    let text = move || match report.get() {
        Some(Ok(t)) => t,
        Some(Err(e)) => e.message,
        None => "Loading…".to_owned(),
    };
    let ready = move || matches!(report.get(), Some(Ok(_)));
    view! {
        <div class="flex h-full flex-col gap-3 p-4">
            <div class="flex shrink-0 flex-wrap items-center gap-2">
                <button class=BUTTON_PRIMARY disabled=move || !ready() on:click=save>
                    "Save as Markdown…"
                </button>
                <button class=BUTTON disabled=move || !ready() on:click=copy>"Copy to clipboard"</button>
                <span class="text-[11px] text-muted">
                    "The layout comes from the report template in Settings."
                </span>
                <button class=BUTTON on:click=move |_| navigate("/settings", Default::default())>
                    "Edit template"
                </button>
            </div>
            <textarea node_ref=area readonly=true spellcheck="false" aria-label="The status report"
                class="min-h-0 w-full flex-1 resize-none rounded-sm border border-line bg-canvas p-3 \
                       font-mono text-[12px] leading-5 text-fg focus:border-accent focus:outline-none"
                prop:value=text></textarea>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        DecisionStatus, NodeSummary, ObjectiveHealthRow, OverviewCounts, WaitingOn,
    };
    use time::macros::date;

    fn node(t: NodeType, n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(t, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn review() -> WeeklyReview {
        WeeklyReview {
            week_start: date!(2027 - 03 - 01),
            week_end: date!(2027 - 03 - 07),
            prev_week_start: date!(2027 - 02 - 22),
            next_week_start: date!(2027 - 03 - 08),
            today: date!(2027 - 03 - 03),
            is_current_week: true,
            slipped: vec![],
            blocked: vec![],
            overloaded: vec![],
            waiting: vec![],
            waiting_resolved: vec![],
            decisions: vec![],
            done: vec![],
            finished: vec![],
            counts: OverviewCounts {
                red: 0,
                amber: 0,
                green: 0,
                idle: 0,
            },
            objectives: Vec::<ObjectiveHealthRow>::new(),
            risks: vec![],
            more_risks: 0,
            warnings: vec![],
        }
    }

    fn slip(kind: SlipKind, t: NodeType) -> ReviewSlip {
        ReviewSlip {
            node: node(t, 1, "Load test"),
            kind,
            project: Some("API launch".into()),
            assignee: Some(node(NodeType::Person, 2, "Priya")),
            from: Some(date!(2027 - 03 - 03)),
            to: Some(date!(2027 - 03 - 08)),
            days: 3,
        }
    }

    #[test]
    fn steps_stay_inside_the_list() {
        assert_eq!(STEPS.len(), 7);
        assert_eq!(STEPS[LAST_STEP], "Report");
        assert_eq!(move_step(0, -1), 0);
        assert_eq!(move_step(0, 1), 1);
        assert_eq!(move_step(6, 1), 6);
        assert_eq!(move_step(6, -1), 5);
        assert_eq!(move_step(3, 100), 6);
        for i in 0..STEPS.len() {
            assert!(!step_question(i).is_empty());
        }
    }

    #[test]
    fn slips_read_naturally() {
        let moved = slip(SlipKind::DueMoved, NodeType::Task);
        assert_eq!(slip_chip(&moved), (Tone::Warning, "moved later"));
        assert_eq!(
            slip_detail(&moved),
            "Wed 2027-03-03 → Mon 2027-03-08 · 3 working days"
        );
        assert_eq!(slip_context(&moved), "API launch · Priya");
        let mut late = slip(SlipKind::Overdue, NodeType::Task);
        late.from = None;
        late.to = Some(date!(2027 - 03 - 02));
        late.days = 1;
        assert_eq!(slip_chip(&late).0, Tone::Danger);
        assert_eq!(slip_detail(&late), "due Tue 2027-03-02 · 1 day overdue");
        let target = slip(SlipKind::TargetMoved, NodeType::Project);
        assert_eq!(slip_context(&target), "project");
        assert_eq!(
            slip_context(&slip(SlipKind::TargetMoved, NodeType::Objective)),
            "objective"
        );
        assert_eq!(working_days_text(1), "1 working day");
    }

    #[test]
    fn row_nodes_follow_what_each_step_shows() {
        let mut r = review();
        assert!(step_nodes(&r, 0).is_empty());
        r.slipped = vec![slip(SlipKind::DueMoved, NodeType::Task)];
        r.decisions = vec![DecisionRow {
            id: Uuid::from_u128(9),
            title: "D".into(),
            status: DecisionStatus::Decided,
            decided_on: None,
            excerpt: String::new(),
            affects: vec![],
            superseded_by: None,
        }];
        let wait = |n: u128| WaitingOnRow {
            waiting: WaitingOn {
                id: Uuid::from_u128(n),
                description: "x".into(),
                person_id: Uuid::from_u128(1),
                asked_on: date!(2027 - 02 - 01),
                expected_by: None,
                follow_up_on: None,
                resolved_on: None,
                created_at: time::OffsetDateTime::UNIX_EPOCH,
                updated_at: time::OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            person: node(NodeType::Person, 1, "Raj"),
            about: None,
            age_days: 3,
            stale: true,
            snoozed: false,
            overdue: false,
        };
        r.waiting = vec![wait(30)];
        r.waiting_resolved = vec![wait(31)];
        assert_eq!(step_nodes(&r, 0).len(), 1);
        let waiting: Vec<Uuid> = step_nodes(&r, 3).iter().map(|n| n.id).collect();
        assert_eq!(waiting, vec![Uuid::from_u128(30), Uuid::from_u128(31)]);
        assert_eq!(step_nodes(&r, 4)[0].node_type, NodeType::Decision);
        assert!(step_nodes(&r, LAST_STEP).is_empty());
        let counts = step_counts(&r);
        assert_eq!(counts, [1, 0, 0, 2, 1, 0, 0]);
        for (step, count) in counts.iter().enumerate() {
            assert_eq!(step_nodes(&r, step).len(), *count, "step {step}");
        }
    }

    #[test]
    fn the_report_file_is_named_after_its_week() {
        assert_eq!(
            export_name(date!(2027 - 03 - 01)),
            "status-report-2027-03-01.md"
        );
    }
}
