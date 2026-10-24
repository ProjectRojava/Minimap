//! Order of work (spec 60, ADR-0024; the queue and My team, spec 61): one person's tasks as
//! steps, top to bottom. Step 1 is done first; tasks of one step are done at the same time; a step
//! starts when the step above it has finished. The schedule follows the order, so the Gantt, the
//! forecast and health move with it. The rules are in `minimap_core::order`; this screen only
//! moves tasks and sends the new steps (`set_order`, one undo step). *My team* is in
//! `order_team.rs`.

use std::collections::{HashMap, HashSet};

use leptos::{ev, prelude::*, task::spawn_local, web_sys};
use leptos_router::hooks::use_navigate;
use minimap_types::{Date, NodeRef, NodeType, OrderCard, OrderStep, TeamOrders, Uuid, WorkOrder};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::{
        avatar::PersonAvatar,
        form::{BUTTON, BUTTON_ON},
        page::{EmptyState, Hints, PageHeader, Tone},
        select::SelectField,
        task_type::TypeChip,
    },
    pages::order_team::TeamBoard,
    state::{finish, DataVersion, Selection, Toasts},
};

// ------------------------------------------------------------------ pure logic

/// Where a task is dropped or moved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Into step `i` (at the same time as what is there).
    Join(usize),
    /// A step of its own, before step `i` (`i` = the number of steps: after the last).
    NewBefore(usize),
    /// Out of the order.
    Out,
}

/// The ids of each step, top to bottom.
pub fn step_ids(order: &WorkOrder) -> Vec<Vec<Uuid>> {
    order
        .steps
        .iter()
        .map(|s| s.cards.iter().map(|c| c.id).collect())
        .collect()
}

/// The step `id` is in, if any.
fn step_of(steps: &[Vec<Uuid>], id: Uuid) -> Option<usize> {
    steps.iter().position(|s| s.contains(&id))
}

/// `steps` with `id` moved to `target` (the indices are those of `steps`, before the move).
/// Steps left empty are dropped.
pub fn moved(steps: &[Vec<Uuid>], id: Uuid, target: Target) -> Vec<Vec<Uuid>> {
    let mut out: Vec<Vec<Uuid>> = steps
        .iter()
        .map(|s| s.iter().copied().filter(|x| *x != id).collect())
        .collect();
    match target {
        Target::Join(i) if i < out.len() => out[i].push(id),
        Target::Join(_) => out.push(vec![id]),
        Target::NewBefore(i) => out.insert(i.min(out.len()), vec![id]),
        Target::Out => {}
    }
    out.retain(|s| !s.is_empty());
    out
}

/// `↑` / `Alt+↑`: a task that shares its step gets a step of its own just above; a task alone
/// in its step goes above the step before it. Nothing moves above the first step.
pub fn up(steps: &[Vec<Uuid>], id: Uuid) -> Vec<Vec<Uuid>> {
    match step_of(steps, id) {
        Some(i) if steps[i].len() > 1 => moved(steps, id, Target::NewBefore(i)),
        Some(i) if i > 0 => moved(steps, id, Target::NewBefore(i - 1)),
        _ => steps.to_vec(),
    }
}

/// `↓` / `Alt+↓`: a task that shares its step gets a step of its own just below; a task alone
/// in its step goes below the step after it. Nothing moves below the last step.
pub fn down(steps: &[Vec<Uuid>], id: Uuid) -> Vec<Vec<Uuid>> {
    match step_of(steps, id) {
        Some(i) if steps[i].len() > 1 => moved(steps, id, Target::NewBefore(i + 1)),
        Some(i) if i + 1 < steps.len() => moved(steps, id, Target::NewBefore(i + 2)),
        _ => steps.to_vec(),
    }
}

/// `t`: at the same time as the step above. The first step stays where it is.
pub fn together(steps: &[Vec<Uuid>], id: Uuid) -> Vec<Vec<Uuid>> {
    match step_of(steps, id) {
        Some(i) if i > 0 => moved(steps, id, Target::Join(i - 1)),
        _ => steps.to_vec(),
    }
}

/// `Shift+T`: a task that shares its step gets a step of its own, just below it.
pub fn own_step(steps: &[Vec<Uuid>], id: Uuid) -> Vec<Vec<Uuid>> {
    match step_of(steps, id) {
        Some(i) if steps[i].len() > 1 => moved(steps, id, Target::NewBefore(i + 1)),
        _ => steps.to_vec(),
    }
}

/// `1`-`9`: into step `n` (1 = the first); past the last, a new last step.
pub fn into_step(steps: &[Vec<Uuid>], id: Uuid, n: usize) -> Vec<Vec<Uuid>> {
    let i = n.saturating_sub(1);
    if i < steps.len() {
        moved(steps, id, Target::Join(i))
    } else {
        moved(steps, id, Target::NewBefore(usize::MAX))
    }
}

/// Where a drop over a row of step `step` lands, `y` pixels down a row `height` tall: near the
/// top edge of the step's first row, a new step before it; near the bottom edge of its last row,
/// a new step after it; anywhere else, into the step.
pub fn drop_target(step: usize, first: bool, last: bool, y: f64, height: f64) -> Target {
    let f = if height > 0.0 { y / height } else { 0.5 };
    if first && f < 0.3 {
        Target::NewBefore(step)
    } else if last && f > 0.7 {
        Target::NewBefore(step + 1)
    } else {
        Target::Join(step)
    }
}

/// `order` regrouped into `steps` at once, before the schedule answers, so a move shows the
/// moment it is made (the dates are the old ones until then). `extra` is a task that comes from
/// someone else (given on My team). Tasks no step names go to the tray.
pub fn rearranged(order: &WorkOrder, steps: &[Vec<Uuid>], extra: Option<&OrderCard>) -> WorkOrder {
    let all: Vec<&OrderCard> = order
        .steps
        .iter()
        .flat_map(|s| s.cards.iter())
        .chain(order.unordered.iter())
        .chain(extra)
        .collect();
    let by_id: HashMap<Uuid, &OrderCard> = all.iter().map(|c| (c.id, *c)).collect();
    let placed: HashSet<Uuid> = steps.iter().flatten().copied().collect();
    let new_steps: Vec<OrderStep> = steps
        .iter()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| by_id.get(id).map(|c| (*c).clone()))
                .collect::<Vec<_>>()
        })
        .filter(|cards| !cards.is_empty())
        .enumerate()
        .map(|(i, cards)| OrderStep {
            step: i as u32 + 1,
            start: cards.iter().filter_map(|c| c.start).min(),
            finish: cards.iter().filter_map(|c| c.finish).max(),
            load_pct: cards.iter().map(|c| c.allocation_pct).sum(),
            cards,
        })
        .collect();
    // The tray keeps its order; what leaves the steps goes after it.
    let mut seen = HashSet::new();
    let unordered = order
        .unordered
        .iter()
        .chain(order.steps.iter().flat_map(|s| s.cards.iter()))
        .chain(extra)
        .filter(|c| !placed.contains(&c.id) && seen.insert(c.id))
        .cloned()
        .collect();
    WorkOrder {
        steps: new_steps,
        unordered,
        ..order.clone()
    }
}

/// `order` without the task `id` (given to someone else on My team).
pub fn without(order: &WorkOrder, id: Uuid) -> WorkOrder {
    let steps: Vec<Vec<Uuid>> = step_ids(order)
        .into_iter()
        .map(|s| s.into_iter().filter(|x| *x != id).collect())
        .collect();
    let mut out = rearranged(order, &steps, None);
    out.unordered.retain(|c| c.id != id);
    out
}

/// "Mon 3 Mar".
pub fn day(d: Date) -> String {
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let month = d.month().to_string();
    format!(
        "{} {} {}",
        WEEKDAYS[d.weekday().number_days_from_monday() as usize],
        d.day(),
        month.get(..3).unwrap_or(&month)
    )
}

/// "Mon 3 Mar → Fri 7 Mar", or one day.
pub fn span(start: Option<Date>, finish: Option<Date>) -> String {
    match (start, finish) {
        (Some(s), Some(f)) if s == f => day(s),
        (Some(s), Some(f)) => format!("{} → {}", day(s), day(f)),
        (Some(s), None) => day(s),
        _ => String::new(),
    }
}

/// "3d", "0.5d", or "no estimate".
pub fn estimate_text(days: Option<f64>) -> String {
    match days {
        Some(d) => format!("{}d", (d * 10.0).round() / 10.0),
        None => "no estimate".to_owned(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The line under the header: what the order holds and what needs a look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// "6 tasks in 5 steps".
    pub head: String,
    /// "Last step ends Wed 4 Nov".
    pub ends: Option<String>,
    /// "1 finishes after its due date" (danger).
    pub late: Option<String>,
    /// "2 have no estimate, counted as 1 day" (warning).
    pub unestimated: Option<String>,
    /// "2 tasks are already started. ..." (the note above the rows).
    pub started: Option<String>,
}

pub fn summary(order: &WorkOrder) -> Summary {
    let cards: Vec<&OrderCard> = order.steps.iter().flat_map(|s| s.cards.iter()).collect();
    let head = if cards.is_empty() {
        "Nothing in the order yet".to_owned()
    } else {
        format!(
            "{} in {}",
            plural(cards.len(), "task", "tasks"),
            plural(order.steps.len(), "step", "steps")
        )
    };
    let ends = order
        .steps
        .last()
        .and_then(|s| s.finish)
        .map(|f| format!("Last step ends {}", day(f)));
    let late = cards.iter().filter(|c| c.past_due).count();
    let late = (late > 0).then(|| {
        if late == 1 {
            "1 finishes after its due date".to_owned()
        } else {
            format!("{late} finish after their due dates")
        }
    });
    let guessed = cards.iter().filter(|c| c.estimate_days.is_none()).count();
    let unestimated = (guessed > 0).then(|| {
        if guessed == 1 {
            "1 has no estimate, counted as 1 day".to_owned()
        } else {
            format!("{guessed} have no estimate, counted as 1 day")
        }
    });
    let n = cards.iter().filter(|c| c.started).count();
    let started = (n > 0).then(|| {
        if n == 1 {
            "1 task is already started. It keeps its own start; the order holds up only what comes after it.".to_owned()
        } else {
            format!("{n} tasks are already started. They keep their own start; the order holds up only what comes after them.")
        }
    });
    Summary {
        head,
        ends,
        late,
        unestimated,
        started,
    }
}

/// What a step's label row says: *Step 3 · 2 at the same time · 150% of Uditt's time*. Only for
/// steps with more than one task.
pub fn step_label(step: &OrderStep, person: &str) -> Option<(String, String)> {
    (step.cards.len() > 1).then(|| {
        (
            format!("Step {} · {} at the same time", step.step, step.cards.len()),
            format!("{}% of {}'s time", step.load_pct, person),
        )
    })
}

/// How many working days of the ruler lie after `due` up to `finish`; `None` when the finish is
/// past the ruler (then the count is unknown here) or nothing lies between.
pub fn days_late(ruler: &[Date], due: Date, finish: Date) -> Option<usize> {
    if ruler.last().is_none_or(|last| finish > *last) {
        return None;
    }
    let n = ruler.iter().filter(|d| **d > due && **d <= finish).count();
    (n > 0).then_some(n)
}

/// The red line under a late task's dates.
pub fn late_text(ruler: &[Date], due: Date, finish: Date) -> String {
    match days_late(ruler, due, finish) {
        Some(n) => format!(
            "{} after due {}",
            plural(n, "working day", "working days"),
            day(due)
        ),
        None => format!("finishes after due {}", day(due)),
    }
}

/// A bar on the ruler, as (left, width) in percent of its width; `None` when it is off the ruler.
pub fn bar(ruler: &[Date], start: Option<Date>, finish: Option<Date>) -> Option<(f64, f64)> {
    let n = ruler.len();
    let (first, last) = (*ruler.first()?, *ruler.last()?);
    let s = start?;
    let f = finish.unwrap_or(s).max(s);
    if f < first || s > last {
        return None;
    }
    let a = ruler.iter().filter(|d| **d < s).count().min(n - 1);
    let b = ruler.iter().filter(|d| **d <= f).count().max(a + 1).min(n);
    let pct = |i: usize| i as f64 * 100.0 / n as f64;
    Some((pct(a), pct(b - a)))
}

/// Where a mark for the end of day `d` (a due date) goes, in percent; `None` off the ruler.
pub fn tick(ruler: &[Date], d: Date) -> Option<f64> {
    let (first, last) = (*ruler.first()?, *ruler.last()?);
    if d < first || d > last {
        return None;
    }
    Some(ruler.iter().filter(|r| **r <= d).count() as f64 * 100.0 / ruler.len() as f64)
}

/// Where today's line goes (the start of the day), in percent; `None` off the ruler.
pub fn today_at(ruler: &[Date], today: Date) -> Option<f64> {
    let (first, last) = (*ruler.first()?, *ruler.last()?);
    if today < first || today > last {
        return None;
    }
    Some(ruler.iter().filter(|r| **r < today).count() as f64 * 100.0 / ruler.len() as f64)
}

/// The places the ruler skips days (a weekend), in percent, for a faint line.
pub fn week_breaks(ruler: &[Date]) -> Vec<f64> {
    (1..ruler.len())
        .filter(|i| (ruler[*i] - ruler[i - 1]).whole_days() > 1)
        .map(|i| i as f64 * 100.0 / ruler.len() as f64)
        .collect()
}

/// The open tasks the tray shows for `filter` (in the title or the project, any case).
pub fn tray_matches(card: &OrderCard, filter: &str) -> bool {
    let f = filter.trim().to_lowercase();
    f.is_empty()
        || card.title.to_lowercase().contains(&f)
        || card
            .project
            .as_ref()
            .is_some_and(|p| p.label.to_lowercase().contains(&f))
}

const PERSON_KEY: &str = "minimap.order-person";
const MODE_KEY: &str = "minimap.order-mode";

pub(crate) fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

pub(crate) fn remember_person(id: Uuid) {
    if let Some(s) = storage() {
        let _ = s.set_item(PERSON_KEY, &id.to_string());
    }
}

/// Focuses the task `id` (a row of the queue, a task in the tray or a row on My team).
pub(crate) fn focus_task(id: Uuid) {
    request_animation_frame(move || {
        let el = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| {
                d.query_selector(&format!("[data-order-card=\"{id}\"]"))
                    .ok()
                    .flatten()
            })
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
        if let Some(el) = el {
            let _ = el.focus();
        }
    });
}

/// Focuses the task `delta` places after `from` on the screen (`j` / `k`).
fn focus_sibling(from: &web_sys::Element, delta: i32) {
    let Some(list) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector_all("[data-order-card]").ok())
    else {
        return;
    };
    let n = list.length() as i32;
    let Some(at) = (0..n).find(|i| {
        list.item(*i as u32)
            .is_some_and(|node| node.is_same_node(Some(from.as_ref())))
    }) else {
        return;
    };
    let next = (at + delta).clamp(0, n - 1);
    if let Some(el) = list
        .item(next as u32)
        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// Whether a key or click landed in a text box or on a button of the row (which do their own
/// thing) rather than on the row itself.
fn on_control(target: Option<web_sys::EventTarget>) -> bool {
    target
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .and_then(|el| el.closest("button, input").ok().flatten())
        .is_some()
}

/// Where in its row the pointer is: (y, height) in pixels.
pub(crate) fn pointer_in_row(e: &ev::DragEvent) -> (f64, f64) {
    e.current_target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .map(|el| {
            let r = el.get_bounding_client_rect();
            (f64::from(e.client_y()) - r.top(), r.height())
        })
        .unwrap_or((0.0, 0.0))
}

/// The task a drop carries: the one being dragged, or the id in the drag data.
pub(crate) fn dropped_id(e: &ev::DragEvent, dragging: Option<Uuid>) -> Option<Uuid> {
    dragging.or_else(|| {
        e.data_transfer()
            .and_then(|dt| dt.get_data("text/plain").ok())
            .and_then(|s| Uuid::parse_str(&s).ok())
    })
}

/// Starts a drag of `id` (WebKit only starts a drag when it carries data).
pub(crate) fn start_drag(e: &ev::DragEvent, id: Uuid) {
    if let Some(dt) = e.data_transfer() {
        let _ = dt.set_data("text/plain", &id.to_string());
        dt.set_effect_allowed("move");
    }
}

// ------------------------------------------------------------------ the screen

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Person,
    Team,
}

#[component]
pub fn Order() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let mode = RwSignal::new(
        match storage()
            .and_then(|s| s.get_item(MODE_KEY).ok().flatten())
            .as_deref()
        {
            Some("team") => Mode::Team,
            _ => Mode::Person,
        },
    );
    let set_mode = move |m: Mode| {
        if let Some(s) = storage() {
            let _ = s.set_item(MODE_KEY, if m == Mode::Team { "team" } else { "person" });
        }
        mode.set(m);
    };
    // Whose order: the one looked at last on this computer, else yours.
    let person = RwSignal::new(
        storage()
            .and_then(|s| s.get_item(PERSON_KEY).ok().flatten())
            .and_then(|s| Uuid::parse_str(&s).ok()),
    );
    let team_data = LocalResource::new(move || {
        version.track();
        api::get_team_orders()
    });
    // The last answer, so the tabs and columns never blank while a change is saved.
    let team = RwSignal::new(None::<TeamOrders>);
    Effect::new(move |_| {
        if let Some(Ok(t)) = team_data.get() {
            team.set(Some(t));
        }
    });
    let open_person = Callback::new(move |id: Uuid| {
        remember_person(id);
        person.set(Some(id));
        set_mode(Mode::Person);
    });

    let toggle = move || {
        let class = |on: bool| format!("{BUTTON} {}", if on { BUTTON_ON } else { "" });
        view! {
            <span class="flex shrink-0" role="group" aria-label="Show">
                <button class=move || format!("{} rounded-r-none", class(mode.get() == Mode::Person))
                        aria-pressed=move || (mode.get() == Mode::Person).to_string()
                        on:click=move |_| set_mode(Mode::Person)>"One person"</button>
                <button class=move || format!("{} -ml-px rounded-l-none", class(mode.get() == Mode::Team))
                        aria-pressed=move || (mode.get() == Mode::Team).to_string()
                        on:click=move |_| set_mode(Mode::Team)>"My team"</button>
            </span>
        }
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="order" title="Order"
                        subtitle="What each person does first, and what they do at the same time">
                {toggle}
                {move || match mode.get() {
                    Mode::Person => view! {
                        <Hints keys=&[("j/k", "select"), ("Alt+↑/↓", "move"), ("t", "together"), ("e", "estimate")] />
                    }.into_any(),
                    Mode::Team => view! {
                        <Hints keys=&[("Drag", "reorder, or give to someone"), ("Enter", "open")] />
                    }.into_any(),
                }}
            </PageHeader>
            {move || match mode.get() {
                Mode::Person => view! { <PersonQueue person=person team=team /> }.into_any(),
                Mode::Team => view! { <TeamBoard team=team open_person=open_person /> }.into_any(),
            }}
        </div>
    }
}

/// The tabs of people: everyone, you first, with their open tasks (or load, when over), and
/// *Someone else* for anyone the team leaves out.
#[component]
fn PeopleTabs(
    team: RwSignal<Option<TeamOrders>>,
    current: Signal<Option<Uuid>>,
    #[prop(into)] pick: Callback<Uuid>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let people = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Person)
    });
    let tabs = move || {
        let members = team.get().map(|t| t.members).unwrap_or_default();
        let now = current.get();
        members
            .into_iter()
            .map(|m| {
                let id = m.order.person.node.id;
                let on = now == Some(id);
                let open = m.order.steps.iter().map(|s| s.cards.len()).sum::<usize>()
                    + m.order.unordered.len();
                let (note, note_class) = match m.load_pct {
                    Some(l) if l > 100 => (format!("{l}%"), Tone::Warning.text()),
                    _ => (open.to_string(), "text-muted"),
                };
                let title = match m.load_pct {
                    Some(l) => format!("{} open tasks · {l}% this week", open),
                    None => format!("{} open tasks", open),
                };
                view! {
                    <button role="tab" aria-selected=on.to_string() title=title
                        class=format!(
                            "flex h-7 shrink-0 items-center gap-1.5 rounded-sm border pl-1 pr-2 text-[12px] {}",
                            if on { "border-accent/60 bg-accent/10 text-fg" } else { "border-line text-muted hover:text-fg hover:bg-hover" })
                        on:click=move |_| pick.run(id)>
                        <PersonAvatar id=id size=18 name=m.order.person.label.clone() />
                        <span class="max-w-28 truncate">{m.order.person.label.clone()}</span>
                        <span class=format!("text-[11px] tabular-nums {note_class}")>{note}</span>
                    </button>
                }
            })
            .collect_view()
    };
    let others = move || {
        let in_team: HashSet<Uuid> = team
            .get()
            .map(|t| t.members.iter().map(|m| m.order.person.node.id).collect())
            .unwrap_or_default();
        let list = match people.get() {
            Some(Ok(list)) => list,
            _ => return None,
        };
        let mut options = vec![(String::new(), "Someone else…".to_owned())];
        options.extend(
            list.into_iter()
                .filter(|p| !in_team.contains(&p.node.id))
                .map(|p| (p.node.id.to_string(), p.label)),
        );
        if options.len() == 1 {
            return None;
        }
        let selected = current
            .get()
            .filter(|id| !in_team.contains(id))
            .map(|id| id.to_string())
            .unwrap_or_default();
        Some(view! {
            <SelectField compact=true options=options current=selected
                on_change=move |v: String| {
                    if let Ok(id) = Uuid::parse_str(&v) {
                        pick.run(id);
                    }
                } />
        })
    };
    view! {
        <div class="flex min-w-0 flex-wrap items-center gap-1.5" role="tablist" aria-label="Whose order">
            {tabs}
            {others}
        </div>
    }
}

/// A move worked out from the steps and the task (`up`, `down`, `together`, `own_step`).
type Move = fn(&[Vec<Uuid>], Uuid) -> Vec<Vec<Uuid>>;

#[derive(Clone, Copy)]
struct Ctx {
    dragging: RwSignal<Option<Uuid>>,
    over: RwSignal<Option<Target>>,
    /// A move is on its way: the dates shown are the old ones.
    pending: RwSignal<bool>,
    editing: RwSignal<Option<Uuid>>,
    move_to: Callback<(Uuid, Target)>,
    rearrange: Callback<(Uuid, Move)>,
    into: Callback<(Uuid, usize)>,
    save_estimate: Callback<(Uuid, String)>,
}

/// One person's order as a queue (spec 61, option A).
#[component]
fn PersonQueue(
    person: RwSignal<Option<Uuid>>,
    team: RwSignal<Option<TeamOrders>>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let navigate = use_navigate();
    let data = LocalResource::new(move || {
        version.track();
        api::get_order(person.get())
    });
    let shown = RwSignal::new(None::<WorkOrder>);
    let pending = RwSignal::new(false);
    Effect::new(move |_| match data.get() {
        Some(Ok(o)) => {
            if !pending.get_untracked() {
                shown.set(Some(o));
            }
        }
        // A remembered person who is gone: back to you.
        Some(Err(_)) if person.get_untracked().is_some() => person.set(None),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });
    let dragging = RwSignal::new(None::<Uuid>);
    let over = RwSignal::new(None::<Target>);
    let editing = RwSignal::new(None::<Uuid>);
    let filter = RwSignal::new(String::new());

    // Shows the new steps at once, then sends them; the answer has the new dates.
    let save = move |steps: Vec<Vec<Uuid>>, keep_focus: Option<Uuid>| {
        let Some(before) = shown.get_untracked() else {
            return;
        };
        if steps == step_ids(&before) {
            return;
        }
        let who = before.person.node.id;
        shown.set(Some(rearranged(&before, &steps, None)));
        pending.set(true);
        if let Some(id) = keep_focus {
            focus_task(id);
        }
        spawn_local(async move {
            let answer = api::set_order(who, steps).await;
            pending.set(false);
            match finish(answer, toasts, version) {
                Some(o) => shown.set(Some(o)),
                // Refused: back to how it was (the toast says why).
                None => shown.set(Some(before)),
            }
            if let Some(id) = keep_focus {
                focus_task(id);
            }
        });
    };
    let current_steps = move || shown.get_untracked().map(|o| step_ids(&o));
    let ctx = Ctx {
        dragging,
        over,
        pending,
        editing,
        move_to: Callback::new(move |(id, t): (Uuid, Target)| {
            if let Some(s) = current_steps() {
                save(moved(&s, id, t), Some(id));
            }
        }),
        rearrange: Callback::new(move |(id, f): (Uuid, Move)| {
            if let Some(s) = current_steps() {
                save(f(&s, id), Some(id));
            }
        }),
        into: Callback::new(move |(id, n): (Uuid, usize)| {
            if let Some(s) = current_steps() {
                save(into_step(&s, id, n), Some(id));
            }
        }),
        save_estimate: Callback::new(move |(id, text): (Uuid, String)| {
            editing.set(None);
            if text.trim().is_empty() {
                focus_task(id);
                return;
            }
            spawn_local(async move {
                finish(api::set_task_estimate(id, text).await, toasts, version);
                focus_task(id);
            });
        }),
    };

    // `[` and `]`: the previous or next person in the tabs.
    let switch = move |delta: i32| {
        let ids: Vec<Uuid> = team
            .get_untracked()
            .map(|t| t.members.iter().map(|m| m.order.person.node.id).collect())
            .unwrap_or_default();
        let now = shown.get_untracked().map(|o| o.person.node.id);
        if ids.is_empty() {
            return;
        }
        let at = now
            .and_then(|n| ids.iter().position(|i| *i == n))
            .unwrap_or(0) as i32;
        let next = ids[(at + delta).rem_euclid(ids.len() as i32) as usize];
        remember_person(next);
        person.set(Some(next));
    };
    let on_key = move |e: ev::KeyboardEvent| {
        if e.ctrl_key() || e.meta_key() || e.alt_key() || on_control(e.target()) {
            return;
        }
        match e.key().as_str() {
            "[" => switch(-1),
            "]" => switch(1),
            _ => return,
        }
        e.prevent_default();
    };
    let pick = Callback::new(move |id: Uuid| {
        remember_person(id);
        person.set(Some(id));
    });
    let current = Signal::derive(move || shown.get().map(|o| o.person.node.id));

    let summary_bar = move || {
        let o = shown.get()?;
        let s = summary(&o);
        let sep = || view! { <span class="h-3.5 w-px bg-line-strong" aria-hidden="true"></span> };
        let navigate = navigate.clone();
        Some(view! {
            <div class="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-4 py-2 text-[12px] text-muted">
                <span class="font-semibold text-fg">{s.head}</span>
                {s.ends.map(|t| view! { {sep()} <span>{t}</span> })}
                {s.late.map(|t| view! { {sep()} <span class="text-danger">{t}</span> })}
                {s.unestimated.map(|t| view! { {sep()} <span class=Tone::Warning.text()>{t}</span> })}
                <button class=format!("{BUTTON} ml-auto") on:click=move |_| navigate("/gantt", Default::default())>
                    "Open the Gantt"
                </button>
            </div>
        })
    };

    let body = move || {
        let Some(o) = shown.get() else {
            return view! { <p class="p-6 text-muted">"Loading…"</p> }.into_any();
        };
        if o.steps.is_empty() && o.unordered.is_empty() {
            return view! {
                <EmptyState icon="order" title="No open tasks"
                    hint=format!("{} has no open tasks assigned. Tasks assigned to them show here, ready to be put in order.", o.person.label) />
            }
            .into_any();
        }
        view! { <Queue order=o filter=filter ctx=ctx /> }.into_any()
    };

    view! {
        <div class="flex min-h-0 flex-1 flex-col" on:keydown=on_key>
            <div class="flex shrink-0 items-center gap-2 border-b border-line px-4 py-1.5">
                <PeopleTabs team=team current=current pick=pick />
            </div>
            {summary_bar}
            {body}
        </div>
    }
}

/// The grid of a queue row: grip, step, task, estimate, ruler (wide windows), dates.
const ROW_COLS: &str = "grid items-center gap-x-2 \
    grid-cols-[14px_26px_minmax(0,1fr)_48px_minmax(9rem,13rem)] \
    xl:grid-cols-[14px_26px_minmax(0,1fr)_48px_minmax(12rem,20rem)_minmax(9rem,13rem)]";

#[component]
fn Queue(order: WorkOrder, filter: RwSignal<String>, ctx: Ctx) -> impl IntoView {
    let ruler = order.ruler.clone();
    let today = order.today;
    let count = order.steps.len();
    let person = order.person.label.clone();
    let s = summary(&order);
    let started = s.started.map(|t| {
        view! {
            <div class="mx-4 mt-3 flex items-center gap-2 rounded-sm border border-accent/30 bg-accent/5 px-3 py-1.5 text-[12px]">
                <span class=Tone::Accent.chip()>"started"</span>
                <span>{t}</span>
            </div>
        }
    });
    let conflicts = (!order.conflicts.is_empty()).then(|| {
        let lines = order
            .conflicts
            .iter()
            .map(|c| view! { <li>{c.clone()}</li> })
            .collect_view();
        view! {
            <div class="mx-4 mt-3 rounded-sm border border-warning/40 bg-warning/10 px-3 py-2 text-[12px]">
                <p class="font-medium text-warning">"Part of this order can't be followed"</p>
                <ul class="mt-1 list-disc pl-5 text-muted">{lines}</ul>
                <p class="mt-1 text-muted">"The schedule goes by the blocks link. Move the task, or remove the link."</p>
            </div>
        }
    });
    let ruler_head = {
        let ruler = ruler.clone();
        let labels = ruler
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let first_of_week = i == 0 || (*d - ruler[i - 1]).whole_days() > 1;
                let is_today = Some(*d) == today;
                let class = if is_today {
                    "text-accent"
                } else if first_of_week {
                    "text-muted"
                } else {
                    "text-faint"
                };
                let text = if first_of_week || is_today {
                    format!("{} {}", &day(*d)[..3], d.day())
                } else {
                    d.day().to_string()
                };
                view! { <span class=format!("min-w-0 overflow-hidden text-center normal-case tracking-normal {class}") title=day(*d)>{text}</span> }
            })
            .collect_view();
        view! {
            <div class="hidden h-full items-center xl:grid"
                 style=format!("grid-template-columns: repeat({}, minmax(0, 1fr))", ruler.len().max(1))>
                {labels}
            </div>
        }
    };
    let steps = order
        .steps
        .iter()
        .enumerate()
        .map(|(i, st)| {
            let label = step_label(st, &person);
            view! {
                <InsertLine index=i ctx=ctx />
                <StepBlock index=i step=st.clone() label=label ruler=ruler.clone() today=today ctx=ctx />
            }
        })
        .collect_view();
    let first_time = order.steps.is_empty();
    let tray = order.unordered.clone();
    let tray_count = tray.len();
    let end_here = Target::NewBefore(count);
    view! {
        <div class="flex min-h-0 flex-1 flex-col lg:flex-row">
            <div class="min-h-0 min-w-0 flex-1 overflow-y-auto pb-6">
                {started}
                {conflicts}
                <div class=format!("{} {}", column_head(ROW_COLS), "mt-3")>
                    <span></span>
                    <span class="text-right">"#"</span>
                    <span>"Task"</span>
                    <span class="text-right">"Est"</span>
                    {ruler_head}
                    <span>"When"</span>
                </div>
                {first_time.then(|| view! {
                    <p class="px-4 py-3 text-[12px] text-muted">
                        "Nothing is in order yet, so every task starts as soon as it can. Press + on a task on the right, or drag it here, to make it step 1."
                    </p>
                })}
                {steps}
                <InsertLine index=count ctx=ctx />
                <div class=move || format!(
                        "mx-4 mt-1 rounded-sm border border-dashed px-3 py-2 text-[11px] {}",
                        if ctx.over.get() == Some(end_here) { "border-accent bg-hover text-accent" } else { "border-transparent text-faint" })
                    on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(end_here)); }
                    on:dragleave=move |_| ctx.over.set(None)
                    on:drop=move |e: ev::DragEvent| drop_on(e, end_here, ctx)>
                    "Drop a task here, or press a on a task on the right, to add a last step."
                </div>
            </div>
            <Tray cards=tray count=tray_count filter=filter ctx=ctx />
        </div>
    }
}

fn column_head(cols: &str) -> String {
    crate::components::page::column_head(cols)
}

/// A drop on a step, a line, the end of the queue or the tray moves the dragged task there.
fn drop_on(e: ev::DragEvent, target: Target, ctx: Ctx) {
    e.prevent_default();
    e.stop_propagation();
    ctx.over.set(None);
    let id = dropped_id(&e, ctx.dragging.get_untracked());
    ctx.dragging.set(None);
    if let Some(id) = id {
        ctx.move_to.run((id, target));
    }
}

/// The line between two steps where a dragged task would become a step of its own.
#[component]
fn InsertLine(index: usize, ctx: Ctx) -> impl IntoView {
    let here = Target::NewBefore(index);
    view! {
        <div class="relative mx-4 h-0" aria-hidden="true">
            {move || (ctx.over.get() == Some(here)).then(|| view! {
                <div class="pointer-events-none absolute inset-x-0 -top-px z-10 border-t-2 border-accent">
                    <span class="absolute -top-2.5 left-9 bg-canvas px-1 text-[10px] text-accent">"New step here"</span>
                </div>
            })}
        </div>
    }
}

#[component]
fn StepBlock(
    index: usize,
    step: OrderStep,
    label: Option<(String, String)>,
    ruler: Vec<Date>,
    today: Option<Date>,
    ctx: Ctx,
) -> impl IntoView {
    let here = Target::Join(index);
    let shared = step.cards.len() > 1;
    let over_load = step.load_pct > 100;
    let n = step.cards.len();
    let rows = step
        .cards
        .into_iter()
        .enumerate()
        .map(|(j, c)| {
            view! {
                <QueueRow card=c step=index first=j == 0 last=j + 1 == n number=(j == 0).then_some(step.step)
                    shared=shared ruler=ruler.clone() today=today ctx=ctx />
            }
        })
        .collect_view();
    view! {
        <section aria-label=format!("Step {}", step.step)
            class=move || format!(
                "relative mx-4 border-b border-line {}",
                if ctx.over.get() == Some(here) { "rounded-sm outline outline-1 outline-dashed outline-accent bg-accent/5" } else { "" })>
            {label.map(|(what, load)| view! {
                <div class="flex h-6 items-center gap-2 pl-12 text-[11px] text-muted">
                    <span>{what}</span>
                    <span class=if over_load { Tone::Warning.text() } else { "text-muted" }>{load}</span>
                    {move || (ctx.over.get() == Some(here)).then(|| view! { <span class="ml-auto pr-2 text-accent">"together"</span> })}
                </div>
            })}
            {shared.then(|| view! {
                <span class="pointer-events-none absolute bottom-2 left-[2.6rem] top-8 w-1.5 rounded-l-sm border border-r-0 border-line-strong" aria-hidden="true"></span>
            })}
            {rows}
        </section>
    }
}

#[component]
#[allow(clippy::too_many_arguments)]
fn QueueRow(
    card: OrderCard,
    step: usize,
    first: bool,
    last: bool,
    number: Option<u32>,
    shared: bool,
    ruler: Vec<Date>,
    today: Option<Date>,
    ctx: Ctx,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let id = card.id;
    let node = NodeRef::new(NodeType::Task, id);
    let on_key = move |e: ev::KeyboardEvent| {
        if e.ctrl_key() || e.meta_key() || on_control(e.target()) {
            return;
        }
        let key = e.key();
        let target = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        match (e.alt_key(), key.as_str()) {
            (true, "ArrowUp") => ctx.rearrange.run((id, up)),
            (true, "ArrowDown") => ctx.rearrange.run((id, down)),
            (true, _) => return,
            (false, "j" | "ArrowDown") => {
                if let Some(t) = &target {
                    focus_sibling(t, 1);
                }
            }
            (false, "k" | "ArrowUp") => {
                if let Some(t) = &target {
                    focus_sibling(t, -1);
                }
            }
            (false, "t") => ctx.rearrange.run((id, together)),
            (false, "T") => ctx.rearrange.run((id, own_step)),
            (false, "x") => ctx.move_to.run((id, Target::Out)),
            (false, "e") => ctx.editing.set(Some(id)),
            (false, "Enter") => selection.open(node),
            (false, k) if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() && k != "0" => {
                ctx.into.run((id, usize::from(k.as_bytes()[0] - b'0')));
            }
            _ => return,
        }
        // The row's keys win over the app's (Alt+↑/↓ also zoom elsewhere).
        e.prevent_default();
        e.stop_propagation();
    };
    let lane = lane_view(&card, &ruler, today);
    let when = when_view(&card, &ruler, ctx);
    let small = "rounded-sm px-1 text-muted hover:bg-active hover:text-fg";
    view! {
        <div tabindex="0" draggable="true" data-order-card=id.to_string()
            class=move || format!(
                "{ROW_COLS} group h-10 cursor-grab rounded-sm px-1 outline-none hover:bg-hover \
                 focus:bg-hover focus:shadow-[inset_2px_0_0_var(--muted)] {}",
                if ctx.dragging.get() == Some(id) { "opacity-40" } else { "" })
            on:dragstart=move |e: ev::DragEvent| { start_drag(&e, id); ctx.dragging.set(Some(id)); }
            on:dragend=move |_| { ctx.dragging.set(None); ctx.over.set(None); }
            on:dragover=move |e: ev::DragEvent| {
                e.prevent_default();
                let (y, h) = pointer_in_row(&e);
                ctx.over.set(Some(drop_target(step, first, last, y, h)));
            }
            on:drop=move |e: ev::DragEvent| {
                let (y, h) = pointer_in_row(&e);
                drop_on(e, drop_target(step, first, last, y, h), ctx);
            }
            on:click=move |e: ev::MouseEvent| {
                if !on_control(e.target()) {
                    selection.open(node);
                }
            }
            on:keydown=on_key>
            <span class="select-none text-center text-[10px] tracking-[-2px] text-faint" aria-hidden="true">"⋮⋮"</span>
            <span class=format!("text-right font-mono text-[12px] {}", if shared && number.is_none() { "text-faint" } else { "text-muted" })>
                {number.map(|n| n.to_string())}
            </span>
            <div class="grid min-w-0 gap-0.5">
                <span class="truncate text-[12.5px] font-medium">{card.title.clone()}</span>
                <span class="flex min-w-0 items-center gap-1.5 overflow-hidden whitespace-nowrap text-[11px] text-muted">
                    <TypeChip id=card.task_type.clone() />
                    {card.project.as_ref().map(|p| view! { <span class="truncate">{p.label.clone()}</span> })}
                    {(card.allocation_pct < 100).then(|| view! { <span>{format!("{}%", card.allocation_pct)}</span> })}
                    {card.started.then(|| view! { <span class=Tone::Accent.chip()>"started"</span> })}
                </span>
            </div>
            <Estimate id=id days=card.estimate_days ctx=ctx />
            {lane}
            <div class="flex min-w-0 items-center gap-1">
                <div class="min-w-0 flex-1">{when}</div>
                <span class="flex shrink-0 opacity-0 group-hover:opacity-100 group-focus:opacity-100 focus-within:opacity-100">
                    <button class=small title="Earlier (Alt+↑)" aria-label="Earlier"
                            on:click=move |_| ctx.rearrange.run((id, up))>"↑"</button>
                    <button class=small title="Later (Alt+↓)" aria-label="Later"
                            on:click=move |_| ctx.rearrange.run((id, down))>"↓"</button>
                    <button class=small title="Take it out of the order (x)" aria-label="Take it out of the order"
                            on:click=move |_| ctx.move_to.run((id, Target::Out))>"✕"</button>
                </span>
            </div>
        </div>
    }
}

/// The bar on the ruler (wide windows only).
fn lane_view(card: &OrderCard, ruler: &[Date], today: Option<Date>) -> impl IntoView {
    let breaks = week_breaks(ruler)
        .into_iter()
        .map(|x| view! { <span class="absolute inset-y-1 w-px bg-line" style=format!("left:{x:.3}%")></span> })
        .collect_view();
    let now = today
        .and_then(|t| today_at(ruler, t))
        .map(|x| view! { <span class="absolute inset-y-0 z-10 w-px bg-accent" style=format!("left:{x:.3}%") title="Today"></span> });
    let class = if card.past_due {
        "border-danger bg-danger/40"
    } else if card.estimate_days.is_none() {
        "border-dashed border-warning bg-warning/15"
    } else if card.started {
        "border-accent bg-accent"
    } else {
        "border-accent bg-accent/40"
    };
    let height = if card.allocation_pct < 100 {
        "h-1.5"
    } else {
        "h-3"
    };
    let bar_el = bar(ruler, card.start, card.finish).map(|(left, width)| {
        view! {
            <span class=format!("absolute top-1/2 -translate-y-1/2 rounded-sm border {height} {class}")
                  style=format!("left:{left:.3}%;width:{width:.3}%")
                  title=span(card.start, card.finish)></span>
        }
    });
    let due = card.due.and_then(|d| {
        tick(ruler, d).map(|x| {
            let line = if card.past_due {
                "border-danger"
            } else {
                "border-faint"
            };
            view! {
                <span class=format!("absolute inset-y-1 z-10 border-l border-dashed {line}")
                      style=format!("left:{x:.3}%") title=format!("Due {}", day(d))></span>
            }
        })
    });
    view! {
        <div class="relative hidden h-full xl:block" aria-hidden="true">
            {breaks}
            {now}
            {bar_el}
            {due}
        </div>
    }
}

/// The dates the schedule gives the task, and its due date.
fn when_view(card: &OrderCard, ruler: &[Date], ctx: Ctx) -> impl IntoView {
    let dates = span(card.start, card.finish);
    let due = card.due.map(|d| {
        let (class, text) = match (card.past_due, card.finish) {
            (true, Some(f)) => ("text-danger", late_text(ruler, d, f)),
            (true, None) => ("text-danger", format!("finishes after due {}", day(d))),
            (false, _) => ("text-muted", format!("due {}", day(d))),
        };
        view! { <span class=format!("block truncate text-[10.5px] {class}")>{text}</span> }
    });
    view! {
        <span class=move || format!(
                "block truncate text-[11px] tabular-nums transition-opacity {}",
                if ctx.pending.get() { "opacity-40" } else { "" })
              title=move || ctx.pending.get().then_some("Working out the new dates…")>
            {dates}
        </span>
        {due}
    }
}

/// The estimate, edited in place: click it or press `e`, type `3d` or `4h`, `Enter`.
#[component]
fn Estimate(id: Uuid, days: Option<f64>, ctx: Ctx) -> impl IntoView {
    let input = leptos::prelude::NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if ctx.editing.get() == Some(id) {
            if let Some(el) = input.get() {
                let _ = el.focus();
                el.select();
            }
        }
    });
    move || {
        if ctx.editing.get() == Some(id) {
            let start = days.map(|d| estimate_text(Some(d))).unwrap_or_default();
            view! {
                <input node_ref=input type="text" value=start aria-label="Estimate, like 3d or 4h"
                    class="h-6 w-full rounded-sm border border-accent bg-canvas px-1 text-right font-mono text-[11px] outline-none"
                    on:keydown=move |e: ev::KeyboardEvent| {
                        e.stop_propagation();
                        match e.key().as_str() {
                            "Enter" => {
                                e.prevent_default();
                                ctx.save_estimate.run((id, event_target_value(&e)));
                            }
                            "Escape" => {
                                e.prevent_default();
                                ctx.editing.set(None);
                                focus_task(id);
                            }
                            _ => {}
                        }
                    }
                    on:blur=move |_| {
                        if ctx.editing.get_untracked() == Some(id) {
                            ctx.editing.set(None);
                        }
                    } />
            }
            .into_any()
        } else {
            match days {
                Some(d) => view! {
                    <button class="justify-self-end rounded-sm px-1 text-right font-mono text-[11px] text-fg hover:bg-active"
                            title="Change the estimate (e)" on:click=move |_| ctx.editing.set(Some(id))>
                        {estimate_text(Some(d))}
                    </button>
                }
                .into_any(),
                None => view! {
                    <button class=format!("justify-self-end border-b border-dashed border-warning px-1 font-mono text-[11px] {}", Tone::Warning.text())
                            title="No estimate: counted as 1 day. Add one (e)" aria-label="Add an estimate"
                            on:click=move |_| ctx.editing.set(Some(id))>"—"</button>
                }
                .into_any(),
            }
        }
    }
}

/// The open tasks that are not in the order, on the right.
#[component]
fn Tray(cards: Vec<OrderCard>, count: usize, filter: RwSignal<String>, ctx: Ctx) -> impl IntoView {
    let cards = StoredValue::new(cards);
    let list = move || {
        let f = filter.get();
        let shown: Vec<OrderCard> =
            cards.with_value(|c| c.iter().filter(|c| tray_matches(c, &f)).cloned().collect());
        if count == 0 {
            return view! { <p class="px-3 py-3 text-[12px] text-faint">"Everything is in order. Drag a row here to take it out."</p> }.into_any();
        }
        if shown.is_empty() {
            return view! { <p class="px-3 py-3 text-[12px] text-faint">"No task matches the filter."</p> }.into_any();
        }
        shown
            .into_iter()
            .map(|c| view! { <TrayItem card=c ctx=ctx /> })
            .collect_view()
            .into_any()
    };
    view! {
        <aside class=move || format!(
                "flex min-h-48 shrink-0 flex-col border-t border-line bg-panel lg:w-72 lg:border-l lg:border-t-0 {}",
                if ctx.over.get() == Some(Target::Out) { "outline outline-1 -outline-offset-2 outline-dashed outline-accent" } else { "" })
            aria-label="Not in the order"
            on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(Target::Out)); }
            on:dragleave=move |_| ctx.over.set(None)
            on:drop=move |e: ev::DragEvent| drop_on(e, Target::Out, ctx)>
            <header class="grid gap-0.5 px-3 pb-2 pt-3">
                <h2 class="text-[10px] font-semibold uppercase tracking-wider text-muted">{format!("Not in the order · {count}")}</h2>
                <p class="text-[11px] text-muted">"These start as soon as their links and dates allow."</p>
            </header>
            {(count > 4).then(|| view! {
                <input type="search" placeholder="Filter" aria-label="Filter the tasks not in the order"
                    class="mx-3 mb-2 h-7 rounded-sm border border-line bg-canvas px-2 text-[12px] outline-none focus:border-line-strong"
                    prop:value=move || filter.get()
                    on:input=move |e| filter.set(event_target_value(&e)) />
            })}
            <div class="min-h-0 flex-1 overflow-y-auto">{list}</div>
        </aside>
    }
}

#[component]
fn TrayItem(card: OrderCard, ctx: Ctx) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let id = card.id;
    let node = NodeRef::new(NodeType::Task, id);
    let add = move || ctx.move_to.run((id, Target::NewBefore(usize::MAX)));
    let on_key = move |e: ev::KeyboardEvent| {
        if e.ctrl_key() || e.meta_key() || e.alt_key() || on_control(e.target()) {
            return;
        }
        let target = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        match e.key().as_str() {
            "a" => add(),
            "j" | "ArrowDown" => {
                if let Some(t) = &target {
                    focus_sibling(t, 1);
                }
            }
            "k" | "ArrowUp" => {
                if let Some(t) = &target {
                    focus_sibling(t, -1);
                }
            }
            "Enter" => selection.open(node),
            _ => return,
        }
        e.prevent_default();
        e.stop_propagation();
    };
    let due = card.due.map(|d| {
        let class = if card.past_due {
            "text-danger"
        } else {
            "text-muted"
        };
        view! { <span class=class>{format!("due {}", day(d))}</span> }
    });
    view! {
        <div tabindex="0" draggable="true" data-order-card=id.to_string()
            class=move || format!(
                "grid cursor-grab grid-cols-[minmax(0,1fr)_auto] items-center gap-x-2 gap-y-0.5 border-t border-line px-3 py-2 \
                 outline-none hover:bg-hover focus:bg-hover {}",
                if ctx.dragging.get() == Some(id) { "opacity-40" } else { "" })
            on:dragstart=move |e: ev::DragEvent| { start_drag(&e, id); ctx.dragging.set(Some(id)); }
            on:dragend=move |_| { ctx.dragging.set(None); ctx.over.set(None); }
            on:click=move |e: ev::MouseEvent| {
                if !on_control(e.target()) {
                    selection.open(node);
                }
            }
            on:keydown=on_key>
            <span class="truncate text-[12px] font-medium">{card.title.clone()}</span>
            <button class="row-span-2 flex h-6 w-6 items-center justify-center rounded-sm border border-accent/40 text-[14px] text-accent hover:bg-accent/15"
                    title="Put it in a new last step (a)" aria-label="Put it in a new last step"
                    on:click=move |_| add()>"+"</button>
            <span class="flex min-w-0 items-center gap-1.5 overflow-hidden whitespace-nowrap text-[11px] text-muted">
                <TypeChip id=card.task_type.clone() />
                {card.project.as_ref().map(|p| view! { <span class="truncate">{p.label.clone()}</span> })}
                <span>{estimate_text(card.estimate_days)}</span>
                {due}
            </span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeSummary, TaskStatus};
    use time::{macros::date, Weekday};

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn steps() -> Vec<Vec<Uuid>> {
        vec![vec![id(1), id(2)], vec![id(3)], vec![id(4)]]
    }

    fn card(n: u128, start: Date, finish: Date) -> OrderCard {
        OrderCard {
            id: id(n),
            title: format!("T{n}"),
            project: None,
            status: TaskStatus::Todo,
            priority: 3,
            task_type: None,
            estimate_days: Some(2.0),
            start: Some(start),
            finish: Some(finish),
            due: None,
            past_due: false,
            started: false,
            allocation_pct: 100,
        }
    }

    fn order(steps: Vec<Vec<OrderCard>>, unordered: Vec<OrderCard>) -> WorkOrder {
        WorkOrder {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, id(900)),
                label: "Uditt".into(),
                archived: false,
            },
            steps: steps
                .into_iter()
                .enumerate()
                .map(|(i, cards)| OrderStep {
                    step: i as u32 + 1,
                    start: cards.iter().filter_map(|c| c.start).min(),
                    finish: cards.iter().filter_map(|c| c.finish).max(),
                    load_pct: cards.iter().map(|c| c.allocation_pct).sum(),
                    cards,
                })
                .collect(),
            unordered,
            conflicts: Vec::new(),
            ruler: Vec::new(),
            today: None,
        }
    }

    #[test]
    fn a_card_joins_a_step_gets_its_own_or_leaves() {
        let s = steps();
        assert_eq!(
            moved(&s, id(3), Target::Join(0)),
            [vec![id(1), id(2), id(3)], vec![id(4)]]
        );
        assert_eq!(
            moved(&s, id(4), Target::NewBefore(0)),
            [vec![id(4)], vec![id(1), id(2)], vec![id(3)]]
        );
        // Between the first and the second step, using the indices from before the move.
        assert_eq!(
            moved(&s, id(4), Target::NewBefore(1)),
            [vec![id(1), id(2)], vec![id(4)], vec![id(3)]]
        );
        assert_eq!(
            moved(&s, id(3), Target::Out),
            [vec![id(1), id(2)], vec![id(4)]]
        );
        // A new task goes at the end (any index past the last does).
        assert_eq!(
            moved(&s, id(9), Target::NewBefore(usize::MAX))
                .last()
                .unwrap(),
            &vec![id(9)]
        );
        // Dropping a lone card on its own gap leaves the order as it was.
        assert_eq!(moved(&s, id(3), Target::NewBefore(1)), s);
    }

    #[test]
    fn up_and_down_split_a_shared_step_or_hop_over_the_next() {
        let s = steps();
        // Sharing step 1: up gives it a step of its own above, down one below.
        assert_eq!(
            up(&s, id(2)),
            [vec![id(2)], vec![id(1)], vec![id(3)], vec![id(4)]]
        );
        assert_eq!(
            down(&s, id(2)),
            [vec![id(1)], vec![id(2)], vec![id(3)], vec![id(4)]]
        );
        // Alone: it hops over the step before or after it.
        assert_eq!(
            up(&s, id(3)),
            [vec![id(3)], vec![id(1), id(2)], vec![id(4)]]
        );
        assert_eq!(
            down(&s, id(3)),
            [vec![id(1), id(2)], vec![id(4)], vec![id(3)]]
        );
        // Nothing above the first or below the last.
        let alone = vec![vec![id(1)], vec![id(2)]];
        assert_eq!(up(&alone, id(1)), alone);
        assert_eq!(down(&alone, id(2)), alone);
        // Unknown cards change nothing.
        assert_eq!(up(&s, id(99)), s);
    }

    #[test]
    fn together_own_step_and_step_n() {
        let s = steps();
        // `t`: into the step above; the first step can't go higher.
        assert_eq!(
            together(&s, id(3)),
            [vec![id(1), id(2), id(3)], vec![id(4)]]
        );
        assert_eq!(together(&s, id(1)), s);
        // `Shift+T`: a step of its own just below, only when shared.
        assert_eq!(
            own_step(&s, id(1)),
            [vec![id(2)], vec![id(1)], vec![id(3)], vec![id(4)]]
        );
        assert_eq!(own_step(&s, id(3)), s);
        // `1`-`9`: into that step; past the last, a new last step.
        assert_eq!(
            into_step(&s, id(4), 1),
            [vec![id(1), id(2), id(4)], vec![id(3)]]
        );
        assert_eq!(
            into_step(&s, id(1), 9),
            [vec![id(2)], vec![id(3)], vec![id(4)], vec![id(1)]]
        );
    }

    #[test]
    fn a_drop_near_the_edge_of_a_step_makes_a_new_step() {
        // One row in its step: top edge before, bottom edge after, middle into it.
        assert_eq!(drop_target(2, true, true, 5.0, 40.0), Target::NewBefore(2));
        assert_eq!(drop_target(2, true, true, 35.0, 40.0), Target::NewBefore(3));
        assert_eq!(drop_target(2, true, true, 20.0, 40.0), Target::Join(2));
        // Between two rows of one step, it joins the step.
        assert_eq!(drop_target(2, true, false, 35.0, 40.0), Target::Join(2));
        assert_eq!(drop_target(2, false, true, 5.0, 40.0), Target::Join(2));
        assert_eq!(drop_target(0, true, true, 0.0, 0.0), Target::Join(0));
    }

    #[test]
    fn a_move_shows_at_once_with_the_old_dates() {
        let mon = date!(2027 - 03 - 01);
        let fri = date!(2027 - 03 - 05);
        let o = order(
            vec![vec![card(1, mon, mon)], vec![card(2, fri, fri)]],
            vec![card(3, mon, fri)],
        );
        // 3 from the tray joins step 1; 2 leaves for the tray.
        let r = rearranged(&o, &[vec![id(1), id(3)]], None);
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.steps[0].step, 1);
        assert_eq!(r.steps[0].load_pct, 200);
        assert_eq!(r.steps[0].finish, Some(fri));
        assert_eq!(
            r.unordered.iter().map(|c| c.id).collect::<Vec<_>>(),
            [id(2)]
        );
        // A task from someone else comes in; one that leaves is gone from both places.
        let given = card(7, mon, mon);
        let r = rearranged(&o, &[vec![id(7)], vec![id(1)]], Some(&given));
        assert_eq!(r.steps[0].cards[0].id, id(7));
        let w = without(&o, id(1));
        assert_eq!(w.steps.len(), 1);
        assert_eq!(w.steps[0].step, 1);
        let w = without(&o, id(3));
        assert!(w.unordered.is_empty());
    }

    #[test]
    fn the_summary_says_what_the_order_holds_and_what_needs_a_look() {
        let mon = date!(2027 - 03 - 01);
        let wed = date!(2027 - 03 - 03);
        let mut late = card(2, mon, wed);
        late.past_due = true;
        let mut guess = card(3, wed, wed);
        guess.estimate_days = None;
        guess.started = true;
        let o = order(vec![vec![card(1, mon, mon)], vec![late, guess]], vec![]);
        let s = summary(&o);
        assert_eq!(s.head, "3 tasks in 2 steps");
        assert_eq!(s.ends.as_deref(), Some("Last step ends Wed 3 Mar"));
        assert_eq!(s.late.as_deref(), Some("1 finishes after its due date"));
        assert_eq!(
            s.unestimated.as_deref(),
            Some("1 has no estimate, counted as 1 day")
        );
        assert!(s.started.unwrap().starts_with("1 task is already started"));
        assert_eq!(
            step_label(&o.steps[1], "Uditt"),
            Some((
                "Step 2 · 2 at the same time".to_owned(),
                "200% of Uditt's time".to_owned()
            ))
        );
        assert_eq!(step_label(&o.steps[0], "Uditt"), None);
        let empty = summary(&order(vec![], vec![card(1, mon, mon)]));
        assert_eq!(empty.head, "Nothing in the order yet");
        assert!(empty.ends.is_none() && empty.late.is_none());
    }

    #[test]
    fn bars_ticks_and_lateness_on_the_ruler() {
        // Mon 1 to Fri 12 Mar, weekends left out: 10 days.
        let ruler: Vec<Date> = (1..=12)
            .map(|d| Date::from_calendar_date(2027, time::Month::March, d).unwrap())
            .filter(|d| !matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday))
            .collect();
        assert_eq!(ruler.len(), 10);
        let d = |n| Date::from_calendar_date(2027, time::Month::March, n).unwrap();
        // Tue to Thu: 3 days from the second.
        assert_eq!(bar(&ruler, Some(d(2)), Some(d(4))), Some((10.0, 30.0)));
        // Starting before the ruler: cut at its first day.
        assert_eq!(
            bar(&ruler, Some(date!(2027 - 02 - 25)), Some(d(1))),
            Some((0.0, 10.0))
        );
        assert_eq!(bar(&ruler, Some(d(20)), Some(d(22))), None);
        // Due on Sat 6: the line sits after Friday.
        assert_eq!(tick(&ruler, d(6)), Some(50.0));
        assert_eq!(today_at(&ruler, d(3)), Some(20.0));
        assert_eq!(week_breaks(&ruler), [50.0]);
        // Due Fri 5, finishing Tue 9: 2 working days late.
        assert_eq!(days_late(&ruler, d(5), d(9)), Some(2));
        assert_eq!(
            late_text(&ruler, d(5), d(9)),
            "2 working days after due Fri 5 Mar"
        );
        assert_eq!(
            late_text(&ruler, d(5), d(30)),
            "finishes after due Fri 5 Mar"
        );
    }

    #[test]
    fn dates_read_plainly_and_the_tray_filters() {
        let mon = date!(2027 - 03 - 01);
        let fri = date!(2027 - 03 - 05);
        assert_eq!(day(mon), "Mon 1 Mar");
        assert_eq!(span(Some(mon), Some(fri)), "Mon 1 Mar → Fri 5 Mar");
        assert_eq!(span(Some(mon), Some(mon)), "Mon 1 Mar");
        assert_eq!(span(None, None), "");
        assert_eq!(estimate_text(Some(2.5)), "2.5d");
        assert_eq!(estimate_text(None), "no estimate");
        let mut c = card(1, mon, fri);
        c.title = "Queries for GoDigit".into();
        c.project = Some(NodeSummary {
            node: NodeRef::new(NodeType::Project, id(50)),
            label: "Small Bonds Portal".into(),
            archived: false,
        });
        assert!(tray_matches(&c, ""));
        assert!(tray_matches(&c, "godigit"));
        assert!(tray_matches(&c, " bonds "));
        assert!(!tray_matches(&c, "orbis"));
    }
}
