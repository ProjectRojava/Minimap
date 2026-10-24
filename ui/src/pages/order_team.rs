//! *My team* on the Order screen (spec 61, option D): a column for you and for everyone else in
//! Minimap, each with their order. Dragging a row within a column reorders it
//! (`set_order`); dragging it to another person's column gives the task to them at that place
//! (`give_task`, one undo step). The moves and their wording are the pure functions below.

use leptos::{ev, prelude::*, task::spawn_local};
use minimap_types::{NodeRef, NodeType, OrderCard, TeamOrders, Uuid, WorkOrder};

use crate::{
    api,
    components::{
        avatar::PersonAvatar,
        page::{EmptyState, Tone},
    },
    pages::order::{
        day, drop_target, dropped_id, moved, pointer_in_row, rearranged, start_drag, step_ids,
        without, Target,
    },
    state::{finish, DataVersion, Selection, Toasts},
};

// ------------------------------------------------------------------ pure logic

/// What a drop on My team does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Within one person's column: their new order.
    Reorder { person: Uuid, steps: Vec<Vec<Uuid>> },
    /// To someone else's column: the task becomes theirs, and this is their order.
    Give { person: Uuid, steps: Vec<Vec<Uuid>> },
    /// Nothing changes.
    Nothing,
}

fn holds(order: &WorkOrder, task: Uuid) -> bool {
    order
        .steps
        .iter()
        .flat_map(|s| s.cards.iter())
        .chain(order.unordered.iter())
        .any(|c| c.id == task)
}

/// The task `task` dropped at `target` in `to`'s column.
pub fn team_drop(team: &TeamOrders, task: Uuid, to: Uuid, target: Target) -> Action {
    let Some(dest) = team.members.iter().find(|m| m.order.person.node.id == to) else {
        return Action::Nothing;
    };
    let now = step_ids(&dest.order);
    let steps = moved(&now, task, target);
    if holds(&dest.order, task) {
        if steps == now {
            Action::Nothing
        } else {
            Action::Reorder { person: to, steps }
        }
    } else {
        Action::Give { person: to, steps }
    }
}

/// The task as one of the columns shows it.
fn card_of(team: &TeamOrders, task: Uuid) -> Option<OrderCard> {
    team.members.iter().find_map(|m| {
        m.order
            .steps
            .iter()
            .flat_map(|s| s.cards.iter())
            .chain(m.order.unordered.iter())
            .find(|c| c.id == task)
            .cloned()
    })
}

/// `team` after `action`, shown at once while it is saved.
pub fn after(team: &TeamOrders, task: Uuid, action: &Action) -> TeamOrders {
    let mut out = team.clone();
    let card = card_of(team, task);
    for m in &mut out.members {
        let id = m.order.person.node.id;
        match action {
            Action::Reorder { person, steps } if *person == id => {
                m.order = rearranged(&m.order, steps, None);
            }
            Action::Give { person, steps } if *person == id => {
                m.order = rearranged(&m.order, steps, card.as_ref());
            }
            Action::Give { .. } if holds(&m.order, task) => m.order = without(&m.order, task),
            _ => {}
        }
    }
    out
}

/// The message after giving a task: *"Pen-test follow-ups" is now Raj's, step 2.*
pub fn given_text(title: &str, name: &str, steps: &[Vec<Uuid>], task: Uuid) -> String {
    match steps.iter().position(|s| s.contains(&task)) {
        Some(i) => format!("\"{title}\" is now {name}'s, step {}.", i + 1),
        None => format!("\"{title}\" is now {name}'s, not in their order yet."),
    }
}

/// The line under a person's name: *5 steps · ends Wed 4 Nov*.
pub fn column_sub(order: &WorkOrder) -> String {
    let n = order.steps.len();
    let open = order.unordered.len() + order.steps.iter().map(|s| s.cards.len()).sum::<usize>();
    if n > 0 {
        let steps = if n == 1 {
            "1 step".to_owned()
        } else {
            format!("{n} steps")
        };
        match order.steps.last().and_then(|s| s.finish) {
            Some(f) => format!("{steps} · ends {}", day(f)),
            None => steps,
        }
    } else if open > 0 {
        format!(
            "No order yet · {open} open {}",
            if open == 1 { "task" } else { "tasks" }
        )
    } else {
        "No open tasks".to_owned()
    }
}

// ------------------------------------------------------------------ the board

#[derive(Clone, Copy)]
struct TeamCtx {
    dragging: RwSignal<Option<Uuid>>,
    over: RwSignal<Option<(Uuid, Target)>>,
    pending: RwSignal<bool>,
    drop: Callback<(Uuid, Uuid, Target)>,
    open_person: Callback<Uuid>,
}

#[component]
pub fn TeamBoard(team: RwSignal<Option<TeamOrders>>, open_person: Callback<Uuid>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let pending = RwSignal::new(false);
    let drop = Callback::new(move |(task, to, target): (Uuid, Uuid, Target)| {
        let Some(before) = team.get_untracked() else {
            return;
        };
        let action = team_drop(&before, task, to, target);
        if action == Action::Nothing {
            return;
        }
        team.set(Some(after(&before, task, &action)));
        pending.set(true);
        let title = card_of(&before, task).map(|c| c.title).unwrap_or_default();
        let name = before
            .members
            .iter()
            .find(|m| m.order.person.node.id == to)
            .map(|m| m.order.person.label.clone())
            .unwrap_or_default();
        spawn_local(async move {
            let answer = match action {
                Action::Reorder { person, steps } => {
                    api::set_order(person, steps).await.map(|_| None)
                }
                Action::Give { person, steps } => {
                    let text = given_text(&title, &name, &steps, task);
                    api::give_task(task, person, steps)
                        .await
                        .map(|t| Some((t, text)))
                }
                Action::Nothing => Ok(None),
            };
            pending.set(false);
            match finish(answer, toasts, version) {
                Some(Some((t, text))) => {
                    team.set(Some(t));
                    toasts.info_with_undo(text);
                }
                // Reordered: the reload brings the new dates.
                Some(None) => {}
                None => team.set(Some(before)),
            }
        });
    });
    let ctx = TeamCtx {
        dragging: RwSignal::new(None),
        over: RwSignal::new(None),
        pending,
        drop,
        open_person,
    };
    move || {
        let Some(t) = team.get() else {
            return view! { <p class="p-6 text-muted">"Loading…"</p> }.into_any();
        };
        if t.members.is_empty() {
            return view! {
                <EmptyState icon="order" title="No people yet"
                    hint="Add people on the People screen. Each person gets a column here with their order of work." />
            }
            .into_any();
        }
        let alone = t.members.len() == 1;
        let columns = t
            .members
            .into_iter()
            .map(|m| view! { <TeamColumn order=m.order load=m.load_pct ctx=ctx /> })
            .collect_view();
        view! {
            <div class="min-h-0 flex-1 overflow-auto p-4">
                <div class="grid items-start gap-3 grid-cols-[repeat(auto-fill,minmax(15rem,1fr))]">
                    {columns}
                    {alone.then(|| view! {
                        <p class="rounded-sm border border-dashed border-line p-3 text-[12px] text-muted">
                            "Nobody else is in Minimap yet. Add people on the People screen: each gets a column here."
                        </p>
                    })}
                </div>
            </div>
        }
        .into_any()
    }
}

#[component]
fn TeamColumn(order: WorkOrder, load: Option<u32>, ctx: TeamCtx) -> impl IntoView {
    let person = order.person.node.id;
    let name = order.person.label.clone();
    let sub = column_sub(&order);
    let tray = order.unordered.len();
    let load_view = load.map(|l| {
        let class = if l > 100 {
            Tone::Warning.text()
        } else {
            "text-muted"
        };
        view! { <span class=format!("ml-auto shrink-0 text-[11px] tabular-nums {class}") title="This week's load">{format!("{l}%")}</span> }
    });
    let steps = order
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = s.cards.len();
            let shared = n > 1;
            let here = (person, Target::Join(i));
            let rows = s
                .cards
                .iter()
                .enumerate()
                .map(|(j, c)| {
                    view! {
                        <TeamRow card=c.clone() person=person step=i first=j == 0 last=j + 1 == n
                            number=(j == 0).then_some(s.step) ctx=ctx />
                    }
                })
                .collect_view();
            view! {
                <TeamLine person=person index=i ctx=ctx />
                <div class=move || format!(
                        "{} {}",
                        if shared { "ml-2 border-l-2 border-line-strong" } else { "" },
                        if ctx.over.get() == Some(here) { "outline outline-1 -outline-offset-1 outline-dashed outline-accent bg-accent/5" } else { "" })>
                    {rows}
                </div>
            }
        })
        .collect_view();
    let count = order.steps.len();
    let out = (person, Target::Out);
    let end = (person, Target::NewBefore(count));
    let empty = count == 0;
    view! {
        <section class="flex min-w-0 flex-col rounded-sm border border-line bg-canvas" aria-label=format!("{name}'s order")>
            <header class="border-b border-line bg-panel">
                <button class="flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left hover:bg-hover"
                        title=format!("Open {name}'s order") on:click=move |_| ctx.open_person.run(person)>
                    <PersonAvatar id=person size=22 name=name.clone() />
                    <span class="grid min-w-0">
                        <span class="truncate text-[12.5px] font-semibold">{name.clone()}</span>
                        <span class="truncate text-[11px] text-muted">{sub}</span>
                    </span>
                    {load_view}
                </button>
            </header>
            <div class=move || if ctx.pending.get() { "opacity-80" } else { "" }>
                {steps}
                <TeamLine person=person index=count ctx=ctx />
                {empty.then(|| view! {
                    <div class=move || format!(
                            "m-2 rounded-sm border border-dashed px-2 py-3 text-center text-[11px] {}",
                            if ctx.over.get() == Some(end) { "border-accent text-accent" } else { "border-line text-faint" })
                        on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(end)); }
                        on:dragleave=move |_| ctx.over.set(None)
                        on:drop=move |e: ev::DragEvent| team_drop_on(e, end, ctx)>
                        "Nothing in order. Drag a task here to make it step 1."
                    </div>
                })}
            </div>
            <button class=move || format!(
                    "mt-auto border-t px-3 py-1.5 text-left text-[11px] {}",
                    if ctx.over.get() == Some(out) { "border-accent bg-hover text-accent" } else { "border-line text-muted hover:bg-hover hover:text-fg" })
                title=format!("Open {name}'s order")
                on:click=move |_| ctx.open_person.run(person)
                on:dragover=move |e: ev::DragEvent| { e.prevent_default(); ctx.over.set(Some(out)); }
                on:dragleave=move |_| ctx.over.set(None)
                on:drop=move |e: ev::DragEvent| team_drop_on(e, out, ctx)>
                {if tray == 0 { "Nothing outside the order".to_owned() } else { format!("+ {tray} not in the order") }}
            </button>
        </section>
    }
}

fn team_drop_on(e: ev::DragEvent, (person, target): (Uuid, Target), ctx: TeamCtx) {
    e.prevent_default();
    e.stop_propagation();
    ctx.over.set(None);
    let id = dropped_id(&e, ctx.dragging.get_untracked());
    ctx.dragging.set(None);
    if let Some(id) = id {
        ctx.drop.run((id, person, target));
    }
}

/// Where a dragged task would become a step of its own in a column.
#[component]
fn TeamLine(person: Uuid, index: usize, ctx: TeamCtx) -> impl IntoView {
    let here = (person, Target::NewBefore(index));
    view! {
        <div class="relative h-0" aria-hidden="true">
            {move || (ctx.over.get() == Some(here)).then(|| view! {
                <div class="pointer-events-none absolute inset-x-1 -top-px z-10 border-t-2 border-accent"></div>
            })}
        </div>
    }
}

#[component]
fn TeamRow(
    card: OrderCard,
    person: Uuid,
    step: usize,
    first: bool,
    last: bool,
    number: Option<u32>,
    ctx: TeamCtx,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let id = card.id;
    let node = NodeRef::new(NodeType::Task, id);
    let date = card.finish.map(|f| {
        let class = if card.past_due {
            "text-danger"
        } else {
            "text-muted"
        };
        let title = if card.past_due {
            card.due
                .map(|d| format!("Finishes after its due date, {}", day(d)))
                .unwrap_or_default()
        } else {
            format!("Finishes {}", day(f))
        };
        view! { <span class=format!("shrink-0 text-[11px] tabular-nums {class}") title=title>{day(f)}</span> }
    });
    view! {
        <div tabindex="0" draggable="true" data-order-card=id.to_string()
            class=move || format!(
                "grid cursor-grab grid-cols-[1.25rem_minmax(0,1fr)_auto] items-center gap-2 border-b border-line px-2.5 py-1.5 \
                 outline-none hover:bg-hover focus:bg-hover {}",
                if ctx.dragging.get() == Some(id) { "opacity-40" } else { "" })
            on:dragstart=move |e: ev::DragEvent| { start_drag(&e, id); ctx.dragging.set(Some(id)); }
            on:dragend=move |_| { ctx.dragging.set(None); ctx.over.set(None); }
            on:dragover=move |e: ev::DragEvent| {
                e.prevent_default();
                let (y, h) = pointer_in_row(&e);
                ctx.over.set(Some((person, drop_target(step, first, last, y, h))));
            }
            on:drop=move |e: ev::DragEvent| {
                let (y, h) = pointer_in_row(&e);
                team_drop_on(e, (person, drop_target(step, first, last, y, h)), ctx);
            }
            on:click=move |_| selection.open(node)
            on:keydown=move |e: ev::KeyboardEvent| {
                if e.key() == "Enter" {
                    e.prevent_default();
                    e.stop_propagation();
                    selection.open(node);
                }
            }>
            <span class="font-mono text-[11px] text-muted">{number.map(|n| n.to_string())}</span>
            <span class="flex min-w-0 items-center gap-1.5">
                <span class="truncate text-[12px] font-medium">{card.title.clone()}</span>
                {card.started.then(|| view! { <span class=Tone::Accent.chip()>"started"</span> })}
            </span>
            {date}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeSummary, OrderStep, TaskStatus, TeamMember};
    use time::macros::date;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn card(n: u128) -> OrderCard {
        let d = date!(2027 - 03 - 01);
        OrderCard {
            id: id(n),
            title: format!("T{n}"),
            project: None,
            status: TaskStatus::Todo,
            priority: 3,
            task_type: None,
            estimate_days: Some(1.0),
            start: Some(d),
            finish: Some(d),
            due: None,
            past_due: false,
            started: false,
            allocation_pct: 100,
        }
    }

    fn member(p: u128, name: &str, steps: Vec<Vec<u128>>, tray: Vec<u128>) -> TeamMember {
        TeamMember {
            order: WorkOrder {
                person: NodeSummary {
                    node: NodeRef::new(NodeType::Person, id(p)),
                    label: name.into(),
                    archived: false,
                },
                steps: steps
                    .into_iter()
                    .enumerate()
                    .map(|(i, ids)| OrderStep {
                        step: i as u32 + 1,
                        start: Some(date!(2027 - 03 - 01)),
                        finish: Some(date!(2027 - 03 - 01) + time::Duration::days(i as i64)),
                        load_pct: 100,
                        cards: ids.into_iter().map(card).collect(),
                    })
                    .collect(),
                unordered: tray.into_iter().map(card).collect(),
                conflicts: Vec::new(),
                ruler: Vec::new(),
                today: None,
            },
            is_self: p == 100,
            load_pct: None,
        }
    }

    fn team() -> TeamOrders {
        TeamOrders {
            members: vec![
                member(100, "Me", vec![vec![1], vec![2]], vec![3]),
                member(200, "Raj", vec![vec![4]], vec![]),
            ],
        }
    }

    #[test]
    fn a_drop_reorders_within_a_column_and_gives_across() {
        let t = team();
        assert_eq!(
            team_drop(&t, id(2), id(100), Target::NewBefore(0)),
            Action::Reorder {
                person: id(100),
                steps: vec![vec![id(2)], vec![id(1)]]
            }
        );
        // Dropped where it already is: nothing.
        assert_eq!(
            team_drop(&t, id(1), id(100), Target::Join(0)),
            Action::Nothing
        );
        // To Raj, into his step 1, or into his tray.
        assert_eq!(
            team_drop(&t, id(2), id(200), Target::Join(0)),
            Action::Give {
                person: id(200),
                steps: vec![vec![id(4), id(2)]]
            }
        );
        assert_eq!(
            team_drop(&t, id(3), id(200), Target::Out),
            Action::Give {
                person: id(200),
                steps: vec![vec![id(4)]]
            }
        );
        assert_eq!(team_drop(&t, id(1), id(999), Target::Out), Action::Nothing);
    }

    #[test]
    fn a_given_task_moves_columns_at_once() {
        let t = team();
        let action = team_drop(&t, id(1), id(200), Target::NewBefore(0));
        let a = after(&t, id(1), &action);
        let mine = &a.members[0].order;
        assert_eq!(step_ids(mine), [vec![id(2)]]);
        let his = &a.members[1].order;
        assert_eq!(step_ids(his), [vec![id(1)], vec![id(4)]]);
        if let Action::Give { steps, .. } = &action {
            assert_eq!(
                given_text("T1", "Raj", steps, id(1)),
                "\"T1\" is now Raj's, step 1."
            );
        }
        assert_eq!(
            given_text("T3", "Raj", &[vec![id(4)]], id(3)),
            "\"T3\" is now Raj's, not in their order yet."
        );
    }

    #[test]
    fn a_column_says_how_long_the_order_runs() {
        let t = team();
        assert_eq!(column_sub(&t.members[0].order), "2 steps · ends Tue 2 Mar");
        assert_eq!(column_sub(&t.members[1].order), "1 step · ends Mon 1 Mar");
        assert_eq!(
            column_sub(&member(5, "A", vec![], vec![1, 2]).order),
            "No order yet · 2 open tasks"
        );
        assert_eq!(
            column_sub(&member(5, "A", vec![], vec![]).order),
            "No open tasks"
        );
    }
}
