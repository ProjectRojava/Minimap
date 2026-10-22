//! Order of work (spec 60, ADR-0024): one person's tasks as steps, top to bottom. Step 1 is done
//! first; cards side by side in a step are done at the same time; a step starts when the step
//! above it has finished. The schedule follows the order, so the Gantt, the forecast and health
//! move with it. The rules are in `minimap_core::order`; this screen only moves cards and sends
//! the new steps (`set_order`, one undo step).

use leptos::{ev, prelude::*, task::spawn_local, web_sys};
use minimap_types::{Date, NodeRef, NodeType, OrderCard, OrderStep, Uuid, WorkOrder};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON_SOFT},
        page::{EmptyState, Hints, PageHeader, Tone},
        task_type::TypeChip,
    },
    state::{finish, DataVersion, Selection, Toasts},
};

// ------------------------------------------------------------------ pure logic

/// Where a card is dropped or moved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Into step `i` (side by side with what is there).
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

/// `↑` / `Alt+↑`: a card that shares its step gets a step of its own just above; a card alone
/// in its step goes above the step before it. Nothing moves above the first step.
pub fn up(steps: &[Vec<Uuid>], id: Uuid) -> Vec<Vec<Uuid>> {
    match step_of(steps, id) {
        Some(i) if steps[i].len() > 1 => moved(steps, id, Target::NewBefore(i)),
        Some(i) if i > 0 => moved(steps, id, Target::NewBefore(i - 1)),
        _ => steps.to_vec(),
    }
}

/// `↓` / `Alt+↓`: a card that shares its step gets a step of its own just below; a card alone
/// in its step goes below the step after it. Nothing moves below the last step.
pub fn down(steps: &[Vec<Uuid>], id: Uuid) -> Vec<Vec<Uuid>> {
    match step_of(steps, id) {
        Some(i) if steps[i].len() > 1 => moved(steps, id, Target::NewBefore(i + 1)),
        Some(i) if i + 1 < steps.len() => moved(steps, id, Target::NewBefore(i + 2)),
        _ => steps.to_vec(),
    }
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

/// What a step's header says after its number: its dates, and how much is done at once.
pub fn step_text(step: &OrderStep) -> String {
    let mut parts = vec![span(step.start, step.finish)];
    if step.cards.len() > 1 {
        parts.push(format!("{} at once, {}%", step.cards.len(), step.load_pct));
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" · ")
}

/// "3d", "0.5d", or "no estimate".
pub fn estimate_text(days: Option<f64>) -> String {
    match days {
        Some(d) => format!("{}d", (d * 10.0).round() / 10.0),
        None => "no estimate".to_owned(),
    }
}

const PERSON_KEY: &str = "minimap.order-person";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

// ------------------------------------------------------------------ the screen

#[component]
pub fn Order() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    // Whose order: the one looked at last on this computer, else yours.
    let person = RwSignal::new(
        storage()
            .and_then(|s| s.get_item(PERSON_KEY).ok().flatten())
            .and_then(|s| Uuid::parse_str(&s).ok()),
    );
    let data = LocalResource::new(move || {
        version.track();
        api::get_order(person.get())
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Person)
    });
    // The last answer, so the screen never blanks while a change is saved.
    let shown = RwSignal::new(None::<WorkOrder>);
    Effect::new(move |_| match data.get() {
        Some(Ok(o)) => shown.set(Some(o)),
        // A remembered person who is gone: back to you.
        Some(Err(_)) if person.get_untracked().is_some() => person.set(None),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });
    let dragging = RwSignal::new(None::<Uuid>);
    let over = RwSignal::new(None::<Target>);
    let focus_next = RwSignal::new(None::<Uuid>);

    // Sends the new steps; the answer is the order with its new dates.
    let save = move |steps: Vec<Vec<Uuid>>, keep_focus: Option<Uuid>| {
        let Some(who) = shown.get_untracked().map(|o| o.person.node.id) else {
            return;
        };
        spawn_local(async move {
            if let Some(o) = finish(api::set_order(who, steps).await, toasts, version) {
                shown.set(Some(o));
                focus_next.set(keep_focus);
            }
        });
    };
    let move_to = move |id: Uuid, target: Target| {
        if let Some(o) = shown.get_untracked() {
            save(moved(&step_ids(&o), id, target), Some(id));
        }
    };
    let nudge = move |id: Uuid, upward: bool| {
        if let Some(o) = shown.get_untracked() {
            let steps = step_ids(&o);
            let next = if upward {
                up(&steps, id)
            } else {
                down(&steps, id)
            };
            if next != steps {
                save(next, Some(id));
            }
        }
    };
    // After a move, the card keeps the keyboard focus.
    Effect::new(move |_| {
        shown.track();
        if let Some(id) = focus_next.get() {
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
            focus_next.set(None);
        }
    });

    let ctx = Ctx {
        dragging,
        over,
        move_to: Callback::new(move |(id, t): (Uuid, Target)| move_to(id, t)),
        nudge: Callback::new(move |(id, upward): (Uuid, bool)| nudge(id, upward)),
    };

    let picker = move || {
        let current = shown.get().map(|o| o.person.node.id.to_string())?;
        let options: Vec<(String, String)> = match people.get() {
            Some(Ok(list)) => list
                .into_iter()
                .map(|p| (p.node.id.to_string(), p.label))
                .collect(),
            _ => return None,
        };
        Some(view! {
            <SelectField compact=true options=options current=current
                on_change=move |v: String| {
                    if let Ok(id) = Uuid::parse_str(&v) {
                        if let Some(s) = storage() {
                            let _ = s.set_item(PERSON_KEY, &v);
                        }
                        person.set(Some(id));
                    }
                } />
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
        let count = o.steps.len();
        let conflicts = (!o.conflicts.is_empty()).then(|| {
            let lines = o
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
        let steps = o
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                view! {
                    <Gap index=i ctx=ctx />
                    <StepRow index=i step=s.clone() last=i + 1 == count ctx=ctx />
                }
            })
            .collect_view();
        let unordered = o.unordered.clone();
        let tray_count = unordered.len();
        let tray_cards = unordered
            .into_iter()
            .map(|c| view! { <Card card=c place=Place::Tray ctx=ctx /> })
            .collect_view();
        let first_time = o.steps.is_empty();
        view! {
            <div class="flex-1 overflow-y-auto pb-6">
                {conflicts}
                <div class="px-4 pt-3">
                    {first_time.then(|| view! {
                        <p class="mb-2 text-[12px] text-muted">
                            "Nothing is in order yet, so every task starts as soon as it can. Drag a task up here, or press Add, to make it step 1."
                        </p>
                    })}
                    {steps}
                    <Gap index=count ctx=ctx />
                </div>
                <section class=move || format!(
                        "mx-4 mt-4 rounded-sm border border-dashed p-3 {}",
                        if ctx.over.get() == Some(Target::Out) { "border-accent bg-hover" } else { "border-line" })
                    on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(Target::Out)); }
                    on:dragleave=move |_| ctx.over.set(None)
                    on:drop=move |e: ev::DragEvent| drop_on(e, Target::Out, ctx)>
                    <h2 class="mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted">
                        {format!("Not in the order yet ({tray_count})")}
                    </h2>
                    {if tray_count == 0 {
                        view! { <p class="text-[12px] text-faint">"Everything is in order. Drag a card here to take it out."</p> }.into_any()
                    } else {
                        view! { <div class="flex flex-wrap gap-2">{tray_cards}</div> }.into_any()
                    }}
                </section>
            </div>
        }
        .into_any()
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="order" title="Order"
                        subtitle="What each person does first, and what they do at the same time">
                {picker}
                <Hints keys=&[("Alt+↑/↓", "move"), ("Enter", "open")] />
            </PageHeader>
            <p class="border-b border-line px-4 py-1.5 text-[11px] text-muted">
                "Top to bottom: each step starts when the one above it is finished. Side by side: done at the same time. The Gantt and the forecast follow."
            </p>
            {body}
        </div>
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    dragging: RwSignal<Option<Uuid>>,
    over: RwSignal<Option<Target>>,
    move_to: Callback<(Uuid, Target)>,
    nudge: Callback<(Uuid, bool)>,
}

/// A drop on a step, a gap or the tray moves the dragged card there.
fn drop_on(e: ev::DragEvent, target: Target, ctx: Ctx) {
    e.prevent_default();
    e.stop_propagation();
    ctx.over.set(None);
    let id = ctx.dragging.get_untracked().or_else(|| {
        e.data_transfer()
            .and_then(|dt| dt.get_data("text/plain").ok())
            .and_then(|s| Uuid::parse_str(&s).ok())
    });
    ctx.dragging.set(None);
    if let Some(id) = id {
        ctx.move_to.run((id, target));
    }
}

/// The space between two steps (or above the first, below the last): a drop makes a new step.
#[component]
fn Gap(index: usize, ctx: Ctx) -> impl IntoView {
    let here = Target::NewBefore(index);
    view! {
        <div class=move || format!(
                "flex items-center justify-center rounded-sm text-[11px] transition-all {}",
                match (ctx.dragging.get().is_some(), ctx.over.get() == Some(here)) {
                    (true, true) => "my-1 h-9 border border-dashed border-accent bg-hover text-accent",
                    (true, false) => "my-1 h-6 border border-dashed border-line text-faint",
                    _ => "h-2",
                })
             on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(here)); }
             on:dragleave=move |_| ctx.over.set(None)
             on:drop=move |e: ev::DragEvent| drop_on(e, here, ctx)>
            {move || ctx.dragging.get().is_some().then_some("A step of its own here")}
        </div>
    }
}

#[component]
fn StepRow(index: usize, step: OrderStep, last: bool, ctx: Ctx) -> impl IntoView {
    let here = Target::Join(index);
    let over_load = step.load_pct > 100;
    let header = step_text(&step);
    let cards = step
        .cards
        .into_iter()
        .map(|c| view! { <Card card=c place=Place::Step ctx=ctx /> })
        .collect_view();
    view! {
        <section class=move || format!(
                "rounded-sm border px-3 py-2 {}",
                if ctx.over.get() == Some(here) { "border-accent bg-hover" } else { "border-line bg-panel" })
            aria-label=format!("Step {}", step.step)
            on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(here)); }
            on:dragleave=move |_| ctx.over.set(None)
            on:drop=move |e: ev::DragEvent| drop_on(e, here, ctx)>
            <header class="mb-2 flex items-center gap-2 text-[11px]">
                <span class="font-semibold uppercase tracking-wider text-muted">{format!("Step {}", step.step)}</span>
                <span class=if over_load { Tone::Warning.text() } else { "text-muted" }>{header}</span>
                {(!last).then(|| view! { <span class="ml-auto text-faint">"then ↓"</span> })}
            </header>
            <div class="flex flex-wrap gap-2">{cards}</div>
        </section>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Step,
    Tray,
}

#[component]
fn Card(card: OrderCard, place: Place, ctx: Ctx) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let id = card.id;
    let node = NodeRef::new(NodeType::Task, id);
    let dates = span(card.start, card.finish);
    let due = card.due.map(|d| {
        let class = if card.past_due {
            "text-warning"
        } else {
            "text-faint"
        };
        let text = if card.past_due {
            format!("finishes after its due date, {}", day(d))
        } else {
            format!("due {}", day(d))
        };
        view! { <span class=class>{text}</span> }
    });
    let project = card.project.as_ref().map(|p| p.label.clone());
    let on_key = move |e: ev::KeyboardEvent| {
        if e.alt_key() && e.key() == "ArrowUp" && place == Place::Step {
            e.prevent_default();
            ctx.nudge.run((id, true));
        } else if e.alt_key() && e.key() == "ArrowDown" && place == Place::Step {
            e.prevent_default();
            ctx.nudge.run((id, false));
        } else if e.key() == "Enter" {
            e.prevent_default();
            selection.open(node);
        }
    };
    let small = "rounded px-1 text-muted hover:bg-hover hover:text-fg";
    view! {
        <article tabindex="0" draggable="true" data-order-card=id.to_string()
            class=move || format!(
                "w-64 cursor-grab rounded-sm border bg-canvas px-2.5 py-2 text-[12px] outline-none \
                 focus:ring-1 focus:ring-accent {}",
                if ctx.dragging.get() == Some(id) { "border-accent opacity-60" } else { "border-line" })
            on:dragstart=move |e: ev::DragEvent| {
                // WebKit only starts a drag when it carries data.
                if let Some(dt) = e.data_transfer() {
                    let _ = dt.set_data("text/plain", &id.to_string());
                    dt.set_effect_allowed("move");
                }
                ctx.dragging.set(Some(id));
            }
            on:dragend=move |_| { ctx.dragging.set(None); ctx.over.set(None); }
            on:keydown=on_key>
            <div class="flex items-start gap-1">
                <button class="min-w-0 flex-1 text-left font-medium hover:underline"
                        title="Open it on the right" on:click=move |_| selection.open(node)>{card.title.clone()}</button>
                {match place {
                    Place::Step => view! {
                        <span class="flex shrink-0">
                            <button class=small title="Earlier (Alt+↑)" aria-label="Earlier"
                                    on:click=move |_| ctx.nudge.run((id, true))>"↑"</button>
                            <button class=small title="Later (Alt+↓)" aria-label="Later"
                                    on:click=move |_| ctx.nudge.run((id, false))>"↓"</button>
                            <button class=small title="Take it out of the order" aria-label="Take it out of the order"
                                    on:click=move |_| ctx.move_to.run((id, Target::Out))>"✕"</button>
                        </span>
                    }.into_any(),
                    Place::Tray => view! {
                        <button class=format!("{BUTTON_SOFT} shrink-0") title="Put it in a new last step"
                                on:click=move |_| ctx.move_to.run((id, Target::NewBefore(usize::MAX)))>"Add"</button>
                    }.into_any(),
                }}
            </div>
            <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[11px] text-muted">
                <TypeChip id=card.task_type.clone() />
                {project.map(|p| view! { <span class="truncate">{p}</span> })}
                <span>{estimate_text(card.estimate_days)}</span>
                {(card.allocation_pct < 100).then(|| view! { <span>{format!("{}%", card.allocation_pct)}</span> })}
                {card.started.then(|| view! { <span class=Tone::Accent.chip()>"started"</span> })}
            </div>
            <div class="mt-1 text-[11px]">
                <span class="text-fg">{dates}</span>
            </div>
            {due.map(|d| view! { <div class="text-[11px]">{d}</div> })}
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn steps() -> Vec<Vec<Uuid>> {
        vec![vec![id(1), id(2)], vec![id(3)], vec![id(4)]]
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
    fn dates_and_headers_read_plainly() {
        let mon = date!(2027 - 03 - 01);
        let fri = date!(2027 - 03 - 05);
        assert_eq!(day(mon), "Mon 1 Mar");
        assert_eq!(span(Some(mon), Some(fri)), "Mon 1 Mar → Fri 5 Mar");
        assert_eq!(span(Some(mon), Some(mon)), "Mon 1 Mar");
        assert_eq!(span(None, None), "");
        let card = |n: u128| OrderCard {
            id: id(n),
            title: format!("T{n}"),
            project: None,
            status: minimap_types::TaskStatus::Todo,
            priority: 3,
            task_type: None,
            estimate_days: Some(2.0),
            start: Some(mon),
            finish: Some(fri),
            due: None,
            past_due: false,
            started: false,
            allocation_pct: 100,
        };
        let step = OrderStep {
            step: 1,
            start: Some(mon),
            finish: Some(fri),
            load_pct: 200,
            cards: vec![card(1), card(2)],
        };
        assert_eq!(step_text(&step), "Mon 1 Mar → Fri 5 Mar · 2 at once, 200%");
        assert_eq!(estimate_text(Some(2.5)), "2.5d");
        assert_eq!(estimate_text(None), "no estimate");
    }
}
