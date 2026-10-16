//! The Gantt screen (spec 57): the plan on a calendar, objectives → projects → tasks, with the
//! links between tasks drawn as arrows, a bar coloured by where its task stands, and bars you can
//! drag. The tree, the dates and the drag rules come from `get_gantt` / `preview_task_drag` /
//! `apply_task_drag` (core `gantt`); the arithmetic of pixels and days is `crate::gantt`.

use std::collections::{HashMap, HashSet};

use leptos::{ev, prelude::*, task::spawn_local};
use minimap_types::{
    AppError, Date, DragEdge, DragPreview, GanttData, GanttFilter, GanttLinkKind, GanttProject,
    GanttTask, NodeRef, NodeType, Patch, TaskDrag, UpdateObjective, UpdateProject, Uuid,
};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_ON},
        objective_colour::use_objective_colours,
        page::{Hints, PageHeader, Tone, FILTER_BAR},
    },
    gantt::{
        arrow_head, band_dates, band_dropped_text, band_tip, bar_class, can_open, can_resize,
        chain_set, critical_pairs, drag_label, dropped_text, elbow, find_group, find_project,
        find_task, group_title, is_open, link_tone, marks, project_counts, room_chip, room_class,
        rows, shift_band, snap_days, stop_reason, task_candidate, task_tip, tasks_shown,
        with_all_projects, BandPart, Geo, LinkMode, LinkTone, Marks, Row, RowKind, TasksShown,
        BAR_H, CLICK_SLOP, HANDLE_W, HEAD_H, LABEL_W, MIN_BAR_W, NO_OBJECTIVE, ROW_H,
    },
    state::{DataVersion, Selection, Toasts},
    timeline::{axis_ticks, Granularity, TickKind},
};

/// What is being dragged.
#[derive(Clone, Copy, PartialEq)]
enum What {
    Task { id: Uuid, edge: DragEdge },
    Project { id: Uuid, part: BandPart },
    Objective { id: Uuid },
}

#[derive(Clone)]
struct DragState {
    what: What,
    /// Where the pointer was when it went down.
    x0: f64,
    /// Working days along.
    delta: i64,
    /// The pointer has gone far enough for this to be a drag and not a click.
    moved: bool,
    /// Which question to the backend the preview answers; stale answers are dropped.
    seq: u64,
    preview: Option<DragPreview>,
}

#[derive(Clone, Copy)]
struct Ctx {
    scale: RwSignal<Granularity>,
    show_done: RwSignal<bool>,
    mode: RwSignal<LinkMode>,
    toggled: RwSignal<HashSet<Uuid>>,
    hover: RwSignal<Option<Uuid>>,
    drag: RwSignal<Option<DragState>>,
    /// Tasks whose due date a drop pushed, and by how many days: marked until the next drag.
    pushed: RwSignal<HashMap<Uuid, u32>>,
    shown: RwSignal<Option<GanttData>>,
    selection: Selection,
}

fn toggle(toggled: RwSignal<HashSet<Uuid>>, key: Uuid) {
    toggled.update(|t| {
        if !t.remove(&key) {
            t.insert(key);
        }
    });
}

/// Asks the backend what this drag would do, and keeps the answer if it is still the question.
fn ask(drag: RwSignal<Option<DragState>>, td: TaskDrag, seq: u64) {
    spawn_local(async move {
        let answer = api::preview_task_drag(td).await.ok();
        drag.update(|d| {
            if let Some(d) = d.as_mut().filter(|d| d.seq == seq) {
                d.preview = answer;
            }
        });
    });
}

/// The day a candidate asks the backend about: the new first day, or for the right edge the last.
fn drag_target(edge: DragEdge, c: &crate::gantt::Candidate) -> Date {
    if edge == DragEdge::End {
        c.end
    } else {
        c.start
    }
}

#[component]
pub fn Gantt() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let orientation = expect_context::<crate::components::orientation::Orientation>();

    let ctx = Ctx {
        scale: RwSignal::new(Granularity::Days),
        show_done: RwSignal::new(false),
        mode: RwSignal::new(LinkMode::Focus),
        toggled: RwSignal::new(HashSet::new()),
        hover: RwSignal::new(None),
        drag: RwSignal::new(None),
        pushed: RwSignal::new(HashMap::new()),
        shown: RwSignal::new(None),
        selection,
    };
    let loaded = LocalResource::new(move || {
        version.track();
        api::get_gantt(GanttFilter {
            mine: orientation.is_mine(),
        })
    });
    let error = RwSignal::new(None::<AppError>);
    let scroller = leptos::prelude::NodeRef::<leptos::html::Div>::new();
    let scrolled = StoredValue::new(false);
    // The last answer stays on screen while the next one is worked out.
    Effect::new(move |_| match loaded.get() {
        Some(Ok(d)) => {
            error.set(None);
            ctx.shown.set(Some(d));
        }
        Some(Err(e)) => error.set(Some(e)),
        None => {}
    });
    // The first time, bring today to the left of the view.
    Effect::new(move |_| {
        let Some(el) = scroller.get() else { return };
        if scrolled.get_value() {
            return;
        }
        if let Some(x) = ctx.shown.with(|d| {
            d.as_ref()
                .map(|d| Geo::new(d, ctx.scale.get_untracked()).x(0.0))
        }) {
            scrolled.set_value(true);
            el.set_scroll_left((x - 40.0).max(0.0) as i32);
        }
    });

    // ------------------------------------------------------------------ dragging
    let mouse_move = window_event_listener(ev::mousemove, move |e: ev::MouseEvent| {
        let Some(cur) = ctx.drag.get_untracked() else {
            return;
        };
        let dx = f64::from(e.client_x()) - cur.x0;
        if !cur.moved && dx.abs() < CLICK_SLOP {
            return;
        }
        let delta = snap_days(dx, ctx.scale.get_untracked().px_per_day());
        if cur.moved && delta == cur.delta {
            return;
        }
        let seq = cur.seq + 1;
        ctx.drag.update(|d| {
            if let Some(d) = d {
                d.moved = true;
                d.delta = delta;
                d.seq = seq;
                d.preview = None;
            }
        });
        if let What::Task { id, edge } = cur.what {
            let asked = ctx.shown.with_untracked(|d| {
                let d = d.as_ref()?;
                let c = task_candidate(d, find_task(d, id)?, edge, delta);
                c.changed.then(|| drag_target(edge, &c))
            });
            if let Some(to) = asked {
                ask(
                    ctx.drag,
                    TaskDrag {
                        task_id: id,
                        edge,
                        to,
                    },
                    seq,
                );
            }
        }
    });
    on_cleanup(move || mouse_move.remove());
    let mouse_up = window_event_listener(ev::mouseup, move |_| {
        let Some(cur) = ctx.drag.get_untracked() else {
            return;
        };
        ctx.drag.set(None);
        if !cur.moved {
            let node = match cur.what {
                What::Task { id, .. } => NodeRef::new(NodeType::Task, id),
                What::Project { id, .. } => NodeRef::new(NodeType::Project, id),
                What::Objective { id } => NodeRef::new(NodeType::Objective, id),
            };
            if node.id != NO_OBJECTIVE {
                selection.open(node);
            }
            return;
        }
        drop_it(ctx, cur, toasts, version);
    });
    on_cleanup(move || mouse_up.remove());
    let keys = window_event_listener(ev::keydown, move |e: ev::KeyboardEvent| {
        if e.key() == "Escape" && ctx.drag.get_untracked().is_some() {
            ctx.drag.set(None);
        }
    });
    on_cleanup(move || keys.remove());

    // ------------------------------------------------------------------- the pieces
    let scale_button = move |g: Granularity| {
        view! {
            <button class=move || format!("{BUTTON} {}", if ctx.scale.get() == g { BUTTON_ON } else { "" })
                    on:click=move |_| ctx.scale.set(g)>{g.label()}</button>
        }
    };
    let mode_buttons = LinkMode::ALL
        .into_iter()
        .map(|m| {
            view! {
                <button class=move || format!("{BUTTON} {}", if ctx.mode.get() == m { BUTTON_ON } else { "" })
                        title=m.hint()
                        on:click=move |_| ctx.mode.set(m)>{m.label()}</button>
            }
        })
        .collect_view();
    let set_projects = move |open: bool| {
        ctx.shown.with_untracked(|d| {
            if let Some(d) = d {
                ctx.toggled.update(|t| *t = with_all_projects(d, t, open));
            }
        })
    };
    let shown_now = move || {
        ctx.shown.with(|d| {
            d.as_ref()
                .map_or(TasksShown::None, |d| tasks_shown(d, &ctx.toggled.get()))
        })
    };
    let summary = move || {
        ctx.shown.with(|d| {
            d.as_ref().map_or(String::new(), |d| {
                let projects: usize = d.groups.iter().map(|g| g.projects.len()).sum();
                let tasks: usize = d
                    .groups
                    .iter()
                    .flat_map(|g| g.projects.iter())
                    .map(|p| p.tasks.iter().filter(|t| !t.schedule.done).count())
                    .sum();
                format!("{projects} projects · {tasks} open tasks")
            })
        })
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="gantt" title="Gantt"
                        subtitle="The plan on a calendar: objectives, projects and tasks">
                <Hints keys=&[("←/→", "fold"), ("↑/↓", "move"), ("Alt+←/→", "shift a task")] />
            </PageHeader>
            <crate::components::orientation::MineBanner />
            <div class=FILTER_BAR>
                <span class="flex items-center gap-1" role="group" aria-label="Scale">
                    {scale_button(Granularity::Days)}
                    {scale_button(Granularity::Weeks)}
                </span>
                <span class="flex items-center gap-1" role="group" aria-label="Rows">
                    <button class=move || format!("{BUTTON} {}", if shown_now() == TasksShown::None { BUTTON_ON } else { "" })
                            title="Fold every project to one row"
                            on:click=move |_| set_projects(false)>"Projects only"</button>
                    <button class=move || format!("{BUTTON} {}", if shown_now() == TasksShown::All { BUTTON_ON } else { "" })
                            title="Open every project to its tasks"
                            on:click=move |_| set_projects(true)>"All tasks"</button>
                </span>
                <label class="flex items-center gap-1 text-[11px] text-muted">
                    <input type="checkbox" prop:checked=move || ctx.show_done.get()
                           on:change=move |ev| ctx.show_done.set(event_target_checked(&ev)) />
                    "Show finished"
                </label>
                <span class="flex items-center gap-1" role="group" aria-label="Arrows">
                    <span class="mr-1 text-[10px] uppercase tracking-wider text-muted">"Arrows"</span>
                    {mode_buttons}
                </span>
                <span class="ml-auto text-[11px] text-muted">{summary}</span>
            </div>
            <Legend />
            {move || error.get().map(|e| view! {
                <p class="px-4 py-2 text-danger">{e.message}</p>
            })}
            <div node_ref=scroller class="relative min-h-0 flex-1 overflow-auto" tabindex="0"
                 on:keydown=move |e| on_key(ctx, e, toasts, version)>
                {move || ctx.shown.with(|d| d.is_none()).then(|| view! {
                    <p class="px-4 py-3 text-muted">"Loading…"</p>
                })}
                {move || ctx.shown.with(|d| d.as_ref().is_some_and(|d| d.groups.is_empty())).then(|| view! {
                    <p class="px-4 py-3 text-muted">
                        "Nothing to draw yet. Add a project with tasks that have estimates, and they appear here."
                    </p>
                })}
                <Chart ctx=ctx />
            </div>
        </div>
    }
}

/// What the colours and marks mean.
#[component]
fn Legend() -> impl IntoView {
    let swatch = |class: &'static str, text: &'static str| {
        view! {
            <span class="flex items-center gap-1">
                <svg class="gc" width="22" height="12" aria-hidden="true">
                    <rect class=class x="1" y="1" width="20" height="10" rx="2" />
                </svg>
                {text}
            </span>
        }
    };
    view! {
        <div class="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-line px-4 py-1 text-[11px] text-muted">
            {swatch("gb todo", "To do")}
            {swatch("gb progress", "In progress")}
            {swatch("gb blocked", "Blocked")}
            {swatch("gb done", "Done")}
            <span class="flex items-center gap-1">
                <svg class="gc" width="22" height="12" aria-hidden="true">
                    <rect class="gb todo" x="1" y="1" width="20" height="8" rx="2" />
                    <rect class="crit" x="1" y="9" width="20" height="2" />
                </svg>
                "Critical"
            </span>
            <span class="flex items-center gap-1">
                <svg class="gc" width="22" height="12" aria-hidden="true">
                    <rect class="gb todo" x="1" y="1" width="12" height="10" rx="2" />
                    <rect class="late-tail" x="13" y="1" width="8" height="10" />
                </svg>
                "Past what the target allows"
            </span>
            <span class="flex items-center gap-1">
                <svg class="gc" width="10" height="12" aria-hidden="true">
                    <line class="due" x1="5" x2="5" y1="1" y2="11" />
                </svg>
                "Due date"
            </span>
            <span class="flex items-center gap-1">
                <svg class="gc" width="22" height="12" aria-hidden="true">
                    <rect class="room wide" x="1" y="2" width="20" height="8" />
                </svg>
                "Room before the target"
            </span>
        </div>
    }
}

// ------------------------------------------------------------------------------- keyboard

fn on_key(ctx: Ctx, e: ev::KeyboardEvent, toasts: Toasts, version: DataVersion) {
    let key = e.key();
    if e.ctrl_key() || e.meta_key() {
        return;
    }
    let current = ctx.selection.0.get_untracked();
    let toggled = ctx.toggled.get_untracked();
    let show_done = ctx.show_done.get_untracked();
    // Alt+arrows shift the open task by a working day (Shift: its end; Ctrl is left to the app).
    if e.alt_key() && (key == "ArrowLeft" || key == "ArrowRight") {
        e.prevent_default();
        let Some(node) = current.filter(|n| n.node_type == NodeType::Task) else {
            return;
        };
        let step = if key == "ArrowLeft" { -1 } else { 1 };
        let edge = if e.shift_key() {
            DragEdge::End
        } else {
            DragEdge::Move
        };
        let what = What::Task { id: node.id, edge };
        let cur = DragState {
            what,
            x0: 0.0,
            delta: step,
            moved: true,
            seq: 0,
            preview: None,
        };
        drop_it(ctx, cur, toasts, version);
        return;
    }
    if e.alt_key() || e.shift_key() {
        return;
    }
    let step: isize = match key.as_str() {
        "ArrowDown" | "j" => 1,
        "ArrowUp" | "k" => -1,
        _ => 0,
    };
    ctx.shown.with_untracked(|d| {
        let Some(d) = d else { return };
        let list = rows(d, &toggled, show_done);
        let at = current.and_then(|n| list.iter().position(|r| r.key == n.id));
        let node_of = |r: &Row| match r.kind {
            RowKind::Group(g) => g
                .objective
                .as_ref()
                .map(|o| NodeRef::new(NodeType::Objective, o.id)),
            RowKind::Project(_) => Some(NodeRef::new(NodeType::Project, r.key)),
            RowKind::Task(_) => Some(NodeRef::new(NodeType::Task, r.key)),
        };
        if step != 0 {
            e.prevent_default();
            let next = match at {
                Some(i) => (i as isize + step).clamp(0, list.len() as isize - 1) as usize,
                None => 0,
            };
            if let Some(node) = list.get(next).and_then(node_of) {
                ctx.selection.open(node);
            }
            return;
        }
        let Some(row) = at.and_then(|i| list.get(i)) else {
            if key == "*" {
                ctx.toggled.set(crate::gantt::all_open(d));
            }
            return;
        };
        let default_open = matches!(row.kind, RowKind::Group(_));
        let open = is_open(default_open, row.key, &toggled);
        match key.as_str() {
            "ArrowRight" if can_open(row) && !open => {
                e.prevent_default();
                toggle(ctx.toggled, row.key);
            }
            "ArrowLeft" if can_open(row) && open => {
                e.prevent_default();
                toggle(ctx.toggled, row.key);
            }
            "*" => ctx.toggled.set(crate::gantt::all_open(d)),
            _ => {}
        }
    });
}

// ------------------------------------------------------------------------------- dropping

fn drop_it(ctx: Ctx, cur: DragState, toasts: Toasts, version: DataVersion) {
    match cur.what {
        What::Task { id, edge } => {
            let Some((c, title)) = ctx.shown.with_untracked(|d| {
                let d = d.as_ref()?;
                let t = find_task(d, id)?;
                Some((
                    task_candidate(d, t, edge, cur.delta),
                    t.schedule.title.clone(),
                ))
            }) else {
                return;
            };
            if !c.changed {
                return;
            }
            let td = TaskDrag {
                task_id: id,
                edge,
                to: drag_target(edge, &c),
            };
            spawn_local(async move {
                match api::apply_task_drag(td).await {
                    Ok(done) => {
                        let most = done.preview.pushes.iter().map(|p| p.days).max();
                        toasts.info(dropped_text(&title, &c, done.pushed, most));
                        ctx.pushed.set(
                            done.preview
                                .pushes
                                .iter()
                                .map(|p| (p.task_id, p.days))
                                .collect(),
                        );
                    }
                    Err(e) => toasts.error(&e),
                }
                version.bump();
            });
        }
        What::Project { id, part } => {
            let Some((dates, title)) = ctx.shown.with_untracked(|d| {
                let d = d.as_ref()?;
                let p = find_project(d, id)?;
                Some((band_dates(d, &p.band, part, cur.delta), p.title.clone()))
            }) else {
                return;
            };
            let (start, target) = dates;
            if start.is_none() && target.is_none() {
                return;
            }
            let patch = UpdateProject {
                start_date: start.map_or(Patch::Keep, Patch::Set),
                target_date: target.map_or(Patch::Keep, Patch::Set),
                ..Default::default()
            };
            spawn_local(async move {
                match api::update_project(id, patch).await {
                    Ok(_) => toasts.info(band_dropped_text(&title, start, target)),
                    Err(e) => toasts.error(&e),
                }
                version.bump();
            });
        }
        What::Objective { id } => {
            let Some((dates, title)) = ctx.shown.with_untracked(|d| {
                let d = d.as_ref()?;
                let g = find_group(d, id)?;
                Some((
                    band_dates(d, &g.band, BandPart::Target, cur.delta),
                    group_title(g),
                ))
            }) else {
                return;
            };
            let Some(target) = dates.1 else { return };
            let patch = UpdateObjective {
                target_date: Patch::Set(target),
                ..Default::default()
            };
            spawn_local(async move {
                match api::update_objective(id, patch).await {
                    Ok(_) => toasts.info(band_dropped_text(&title, None, Some(target))),
                    Err(e) => toasts.error(&e),
                }
                version.bump();
            });
        }
    }
}

// ---------------------------------------------------------------------------------- chart

/// Presses on a part of a bar: starts a drag (or a click).
fn press(ctx: Ctx, what: What) -> impl Fn(ev::MouseEvent) {
    move |e: ev::MouseEvent| {
        if e.button() != 0 {
            return;
        }
        e.prevent_default();
        e.stop_propagation();
        ctx.pushed.set(HashMap::new());
        ctx.drag.set(Some(DragState {
            what,
            x0: f64::from(e.client_x()),
            delta: 0,
            moved: false,
            seq: 0,
            preview: None,
        }));
    }
}

#[component]
fn Chart(ctx: Ctx) -> impl IntoView {
    let colours = use_objective_colours();
    let selection = ctx.selection;
    let width = move || {
        ctx.shown.with(|d| {
            d.as_ref()
                .map_or(0.0, |d| crate::gantt::chart_width(d, ctx.scale.get()))
        })
    };

    // The left column: one button row per row of the chart.
    let labels = move || {
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            let toggled = ctx.toggled.get();
            let selected = selection.0.get().map(|n| n.id);
            let pushed = ctx.pushed.get();
            let list = rows(data, &toggled, ctx.show_done.get());
            Some(
                list.iter()
                    .map(|r| label_row(ctx, r, &toggled, selected, &pushed, colours))
                    .collect_view(),
            )
        })
    };

    let header = move || {
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            let scale = ctx.scale.get();
            let geo = Geo::new(data, scale);
            let weeks = scale == Granularity::Weeks;
            let ticks = axis_ticks(&data.days, scale)
                .into_iter()
                .filter(|t| !(t.kind == TickKind::Day && t.label.is_empty()))
                .map(|t| match t.kind {
                    TickKind::Week => view! {
                        <line class="week-line" x1=t.x x2=t.x y1=0 y2=HEAD_H />
                        <text x=t.x + 2.0 y=if weeks { 20.0 } else { 13.0 }>{t.label}</text>
                    }
                    .into_any(),
                    TickKind::Day => view! {
                        <text x=t.x y=HEAD_H - 8.0 text-anchor="middle">{t.label}</text>
                    }
                    .into_any(),
                })
                .collect_view();
            let today = geo.x(0.0);
            Some(view! {
                <svg class="gc" width=chart_width(data, scale) height=HEAD_H>
                    <line class="head-line" x1=0 x2=chart_width(data, scale) y1=HEAD_H y2=HEAD_H />
                    {ticks}
                    <path class="today-marker" d=marker(today, HEAD_H)>
                        <title>"Today"</title>
                    </path>
                </svg>
            })
        })
    };

    let body = move || {
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            Some(body_svg(ctx, data, colours))
        })
    };

    view! {
        <div style=move || format!("width:{}px;min-width:100%", LABEL_W + width())>
            <div class="sticky top-0 z-20 flex bg-panel" style=format!("height:{HEAD_H}px")>
                <div class="sticky left-0 z-30 flex shrink-0 items-end border-b border-r border-line bg-panel px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted"
                     style=format!("width:{LABEL_W}px")>
                    "Objective · project · task"
                </div>
                <div class="border-b border-line">{header}</div>
            </div>
            <div class="flex">
                <div class="sticky left-0 z-10 shrink-0 border-r border-line bg-canvas"
                     style=format!("width:{LABEL_W}px")>
                    {labels}
                </div>
                <div>{body}</div>
            </div>
        </div>
    }
}

fn chart_width(data: &GanttData, scale: Granularity) -> f64 {
    crate::gantt::chart_width(data, scale)
}

/// A small triangle pointing down at `x`, sitting on the line under the date axis.
fn marker(x: f64, y: f64) -> String {
    format!("M{} {}L{} {}L{x} {y}Z", x - 4.0, y - 7.0, x + 4.0, y - 7.0)
}

fn label_row(
    ctx: Ctx,
    row: &Row,
    toggled: &HashSet<Uuid>,
    selected: Option<Uuid>,
    pushed: &HashMap<Uuid, u32>,
    colours: crate::components::objective_colour::ObjectiveColours,
) -> AnyView {
    let key = row.key;
    let is_selected = selected == Some(key);
    let base = format!(
        "flex items-center gap-1 overflow-hidden whitespace-nowrap pr-2 border-b border-line/60 [&>span]:shrink-0 {}",
        if is_selected {
            "bg-active"
        } else {
            "hover:bg-hover"
        }
    );
    let indent = format!(
        "padding-left:{}px;height:{ROW_H}px",
        6.0 + f64::from(row.depth) * 14.0
    );
    let chevron = |open: bool, can: bool| {
        if can {
            view! {
                <button class="flex h-5 w-4 shrink-0 items-center justify-center text-muted hover:text-fg"
                        aria-label=if open { "Fold" } else { "Open" }
                        on:click=move |e| { e.stop_propagation(); toggle(ctx.toggled, key); }>
                    <span class=if open { "inline-block rotate-90" } else { "inline-block" }>"▸"</span>
                </button>
            }
            .into_any()
        } else {
            view! { <span class="w-4 shrink-0"></span> }.into_any()
        }
    };
    match &row.kind {
        RowKind::Group(g) => {
            let open = is_open(true, key, toggled);
            let id = g.objective.as_ref().map(|o| o.id);
            let hue = id.and_then(|i| colours.hue(i));
            let title = group_title(g);
            let tip = title.clone();
            let chips = {
                let mut v: Vec<AnyView> = Vec::new();
                if g.objective.as_ref().is_some_and(|o| o.ongoing) {
                    v.push(view! { <span class=Tone::Neutral.chip()>"ongoing"</span> }.into_any());
                }
                if let Some(n) = g.band.late_by_days {
                    v.push(
                        view! { <span class=Tone::Danger.chip()>{format!("{n}d over")}</span> }
                            .into_any(),
                    );
                }
                v
            };
            let can = !g.projects.is_empty();
            view! {
                <div class=base style=indent>
                    {chevron(open, can)}
                    <span class="obj-dot" style=hue.map(|h| format!("--obj-h: {h}")).unwrap_or_default()></span>
                    <button class="min-w-0 flex-1 truncate text-left font-semibold"
                            title=tip
                            on:click=move |_| {
                                if let Some(i) = id {
                                    ctx.selection.open(NodeRef::new(NodeType::Objective, i));
                                }
                            }>{title}</button>
                    {chips}
                </div>
            }
            .into_any()
        }
        RowKind::Project(p) => {
            let open = is_open(false, key, toggled);
            let can = !p.tasks.is_empty();
            let room = room_chip(&p.zone, p.band.late_by_days);
            let also = p
                .also_under
                .first()
                .map(|o| (o.title.clone(), p.also_under.len()));
            view! {
                <div class=base style=indent>
                    {chevron(open, can)}
                    <button class="min-w-0 flex-1 truncate text-left font-medium"
                            title=format!("{}\n{}", p.title, project_counts(p))
                            on:click=move |_| ctx.selection.open(NodeRef::new(NodeType::Project, key))>
                        {p.title.clone()}
                    </button>
                    <span class="shrink-0 text-[10px] text-muted">{project_counts(p)}</span>
                    {room.map(|(text, bad)| view! {
                        <span class=if bad { Tone::Danger.chip() } else { Tone::Neutral.chip() }>{text}</span>
                    })}
                    {also.map(|(first, n)| view! {
                        <span class=Tone::Neutral.chip()
                              title=format!("Also serves {first}{}", if n > 1 { format!(" and {} more", n - 1) } else { String::new() })>
                            "+" {n}
                        </span>
                    })}
                </div>
            }
            .into_any()
        }
        RowKind::Task(t) => {
            let s = &t.schedule;
            let flash = pushed.get(&key).copied();
            let class = if s.done {
                "min-w-0 flex-1 truncate text-left text-faint"
            } else if s.critical {
                "min-w-0 flex-1 truncate text-left font-semibold"
            } else {
                "min-w-0 flex-1 truncate text-left"
            };
            view! {
                <div class=base style=indent
                     on:mouseenter=move |_| ctx.hover.set(Some(key))
                     on:mouseleave=move |_| ctx.hover.set(None)>
                    <span class="w-4 shrink-0"></span>
                    <button class=class title=task_tip(t)
                            on:click=move |_| ctx.selection.open(NodeRef::new(NodeType::Task, key))>
                        {s.title.clone()}
                    </button>
                    {flash.map(|d| view! {
                        <span class=Tone::Danger.chip() title="Its due date was pushed later by a drop">{format!("+{d}d")}</span>
                    })}
                    {(s.status == minimap_types::TaskStatus::Blocked)
                        .then(|| view! { <span class=Tone::Warning.chip()>"blocked"</span> })}
                    {s.late_by_days.map(|n| view! {
                        <span class=Tone::Danger.chip()>{format!("late {n}d")}</span>
                    })}
                    {t.overdue.then(|| view! { <span class=Tone::Danger.chip()>"overdue"</span> })}
                </div>
            }
            .into_any()
        }
    }
}

/// The chart body: gridlines, then the rows' bars, then the arrows on top.
fn body_svg(
    ctx: Ctx,
    data: &GanttData,
    colours: crate::components::objective_colour::ObjectiveColours,
) -> impl IntoView {
    let scale = ctx.scale.get();
    let geo = Geo::new(data, scale);
    let toggled = ctx.toggled.get();
    let list = rows(data, &toggled, ctx.show_done.get());
    let w = chart_width(data, scale);
    let h = (list.len().max(1)) as f64 * ROW_H;
    let drag = ctx.drag.get();
    let pushed = ctx.pushed.get();
    let selected = ctx.selection.0.get();
    let resizable = can_resize(scale);

    // Bars as the drag leaves them: the dragged one at the pointer, the ones it moves where the
    // backend says, and the due dates it pushes.
    let mut at: HashMap<Uuid, (f64, f64)> = HashMap::new();
    let mut ghost: Option<(Uuid, f64, f64)> = None;
    let mut pushes: HashMap<Uuid, u32> = pushed;
    let mut label: Option<(Uuid, String, bool)> = None;
    let mut limit: Option<(Uuid, f64)> = None;
    if let Some(d) = drag.as_ref().filter(|d| d.moved) {
        if let What::Task { id, edge } = d.what {
            if let Some(t) = find_task(data, id) {
                let c = task_candidate(data, t, edge, d.delta);
                if let Some(p) = &d.preview {
                    for m in &p.moved {
                        at.insert(m.task_id, (m.es, m.ef));
                    }
                    pushes = p.pushes.iter().map(|x| (x.task_id, x.days)).collect();
                }
                at.insert(id, (c.es, c.ef));
                ghost = Some((id, t.schedule.es, t.schedule.ef));
                let mut text = drag_label(&c);
                if let Some(p) = d.preview.as_ref().filter(|p| !p.pushes.is_empty()) {
                    text.push_str(&format!(
                        " · pushes {} due date{}",
                        p.pushes.len(),
                        if p.pushes.len() == 1 { "" } else { "s" }
                    ));
                }
                if c.stopped {
                    text.push_str(&format!(" · {}", stop_reason(data, t)));
                    limit = offset_x(data, Some(t.schedule.earliest_start), geo).map(|x| (id, x));
                }
                label = Some((id, text, c.stopped));
            }
        }
    }

    // Which row each task and project is on, and where its bar ends.
    let mut row_of: HashMap<Uuid, usize> = HashMap::new();
    let mut task_project: HashMap<Uuid, Uuid> = HashMap::new();
    for (i, r) in list.iter().enumerate() {
        row_of.insert(r.key, i);
        if let RowKind::Project(p) = r.kind {
            for t in &p.tasks {
                task_project.insert(t.schedule.id, p.id);
            }
        }
    }
    for g in &data.groups {
        for p in &g.projects {
            for t in &p.tasks {
                task_project.entry(t.schedule.id).or_insert(p.id);
            }
        }
    }

    // ---- grid
    let ticks = axis_ticks(&data.days, scale)
        .into_iter()
        .filter(|t| t.label.is_empty() || t.kind == TickKind::Week)
        .map(|t| {
            let class = if t.kind == TickKind::Week {
                "week-line"
            } else {
                "day-line"
            };
            view! { <line class=class x1=t.x x2=t.x y1=0 y2=h /> }
        })
        .collect_view();

    // ---- focus: the chain of the task pointed at, or open
    let focus_id = ctx.hover.get().or_else(|| {
        selected
            .filter(|n| n.node_type == NodeType::Task)
            .map(|n| n.id)
    });
    let focus = focus_id
        .map(|id| chain_set(&data.links, id))
        .filter(|s| s.len() > 1);
    let critical = critical_pairs(data);

    // ---- rows
    let mut shapes: Vec<AnyView> = Vec::new();
    for (i, r) in list.iter().enumerate() {
        let y = i as f64 * ROW_H;
        let selected_row = selected.is_some_and(|n| n.id == r.key);
        shapes.push(
            view! {
                <rect class=if selected_row { "row-sel" } else { "row-bg" } x=0 y=y width=w height=ROW_H />
                <line class="row-line" x1=0 x2=w y1=y + ROW_H y2=y + ROW_H />
            }
            .into_any(),
        );
        match &r.kind {
            RowKind::Group(g) => {
                let id = g.objective.as_ref().map(|o| o.id);
                let hue = id.and_then(|i| colours.hue(i));
                let mut band = g.band;
                if let (Some(d), Some(oid)) = (drag.as_ref().filter(|d| d.moved), id) {
                    if d.what == (What::Objective { id: oid }) {
                        band = shift_band(&band, BandPart::Target, d.delta);
                    }
                }
                let ongoing = g.objective.as_ref().is_some_and(|o| o.ongoing);
                let can_drag = id.is_some() && !ongoing && band.target_offset.is_some();
                shapes.push(band_shapes(
                    ctx,
                    y,
                    marks(&band, geo),
                    hue,
                    None,
                    ongoing,
                    w,
                    band_tip(&group_title(g), &band, None),
                    id.map(|i| (What::Objective { id: i }, can_drag, false)),
                ));
            }
            RowKind::Project(p) => {
                let hue = None;
                let mut band = p.band;
                if let Some(d) = drag.as_ref().filter(|d| d.moved) {
                    if let What::Project { id, part } = d.what {
                        if id == p.id {
                            band = shift_band(&band, part, d.delta);
                        }
                    }
                }
                let both = p.band.start_offset.is_some() && p.band.target_offset.is_some();
                shapes.push(band_shapes(
                    ctx,
                    y,
                    marks(&band, geo),
                    hue,
                    Some(room_class(&p.zone)),
                    false,
                    w,
                    band_tip(&p.title, &band, Some(&p.zone)),
                    Some((
                        What::Project {
                            id: p.id,
                            part: BandPart::Target,
                        },
                        p.band.target_offset.is_some(),
                        both,
                    )),
                ));
                if let Some(extra) = project_pin(ctx, p, y, marks(&band, geo)) {
                    shapes.push(extra);
                }
            }
            RowKind::Task(t) => {
                let dim = focus.as_ref().is_some_and(|f| !f.contains(&t.schedule.id));
                shapes.push(task_shapes(
                    ctx,
                    data,
                    geo,
                    t,
                    y,
                    at.get(&t.schedule.id).copied(),
                    ghost.filter(|g| g.0 == t.schedule.id),
                    pushes.get(&t.schedule.id).copied(),
                    limit.filter(|l| l.0 == t.schedule.id).map(|l| l.1),
                    dim,
                    resizable,
                ));
            }
        }
    }

    // ---- arrows
    let bar_end = |id: Uuid, want_end: bool| -> Option<(f64, f64, bool)> {
        // (x, y, dashed): the task's own bar, or its project's when the task is folded away.
        if let Some(&i) = row_of.get(&id) {
            let t = list.get(i).and_then(|r| match r.kind {
                RowKind::Task(t) => Some(t),
                _ => None,
            })?;
            let (es, ef) = at
                .get(&id)
                .copied()
                .unwrap_or((t.schedule.es, t.schedule.ef));
            let x = geo.x(if want_end { ef } else { es });
            return Some((x, i as f64 * ROW_H + ROW_H / 2.0, false));
        }
        let pid = task_project.get(&id)?;
        let i = *row_of.get(pid)?;
        let m = match list.get(i)?.kind {
            RowKind::Project(p) => marks(&p.band, geo),
            _ => return None,
        };
        let (x, wd) = m.work?;
        Some((
            if want_end { x + wd } else { x },
            i as f64 * ROW_H + ROW_H / 2.0,
            true,
        ))
    };
    let mut arrows: Vec<(bool, AnyView)> = Vec::new();
    let mut seen: HashSet<(i64, i64, i64, i64, u8)> = HashSet::new();
    for l in &data.links {
        let tone = link_tone(ctx.mode.get(), l, focus.as_ref(), &critical);
        if tone == LinkTone::Hidden {
            continue;
        }
        let ends = match l.kind {
            GanttLinkKind::Blocks => bar_end(l.from, true).zip(bar_end(l.to, false)),
            GanttLinkKind::DependsOn => {
                let end = |pid: Uuid, want_end: bool| {
                    let i = *row_of.get(&pid)?;
                    let RowKind::Project(p) = list.get(i)?.kind else {
                        return None;
                    };
                    let (x, wd) = marks(&p.band, geo).work?;
                    Some((
                        if want_end { x + wd } else { x },
                        i as f64 * ROW_H + ROW_H / 2.0,
                        true,
                    ))
                };
                end(l.from, true).zip(end(l.to, false))
            }
        };
        let Some(((x1, y1, d1), (x2, y2, d2))) = ends else {
            continue;
        };
        // Both ends folded into one project's row: nothing between them to draw.
        if (y1 - y2).abs() < 1.0 && (d1 || d2) {
            continue;
        }
        // Several links between the same two folded rows are one line.
        let at = (x1 as i64, y1 as i64, x2 as i64, y2 as i64, tone as u8);
        if !seen.insert(at) {
            continue;
        }
        let class = format!(
            "link {}{}{}",
            match tone {
                LinkTone::Dim => "dim",
                LinkTone::Quiet => "quiet",
                LinkTone::Normal => "normal",
                _ => "lit",
            },
            if d1 || d2 || l.kind == GanttLinkKind::DependsOn {
                " dashed"
            } else {
                ""
            },
            if l.lag_days > 0 { " lagged" } else { "" },
        );
        let tip = if l.lag_days > 0 {
            format!(
                "Blocks, with {} working day{} lag",
                l.lag_days,
                if l.lag_days == 1 { "" } else { "s" }
            )
        } else if l.kind == GanttLinkKind::DependsOn {
            "The project needs this one".to_owned()
        } else {
            "Must finish before the next one starts".to_owned()
        };
        arrows.push((
            tone == LinkTone::Lit,
            view! {
                <g class=class>
                    <path class="line" d=elbow(x1, y1, x2, y2) />
                    <path class="head" d=arrow_head(x2, y2) />
                    <title>{tip}</title>
                </g>
            }
            .into_any(),
        ));
    }
    // The lit ones on top.
    arrows.sort_by_key(|(lit, _)| *lit);
    let arrows: Vec<AnyView> = arrows.into_iter().map(|(_, v)| v).collect();

    // ---- the drag's label, near the row it is on
    let tag = label.and_then(|(id, text, stopped)| {
        let i = *row_of.get(&id)?;
        let x = at.get(&id).map_or(0.0, |(es, _)| geo.x(*es));
        let width = text.chars().count() as f64 * 5.6 + 12.0;
        let y = (i as f64 * ROW_H - 20.0).max(2.0);
        let x = x.min((w - width).max(0.0));
        Some(view! {
            <g class=if stopped { "tag stopped" } else { "tag" } pointer-events="none">
                <rect x=x y=y width=width height=18 rx="2" />
                <text x=x + 6.0 y=y + 12.5>{text}</text>
            </g>
        })
    });

    let today = geo.x(0.0);
    let dragging = drag.as_ref().is_some_and(|d| d.moved);
    view! {
        <svg class=if dragging { "gc dragging" } else { "gc" } width=w height=h>
            <defs>
                <pattern id="gc-stripes" width="6" height="6" patternUnits="userSpaceOnUse"
                         patternTransform="rotate(45)">
                    <rect class="stripe-bg" width="6" height="6" />
                    <line class="stripe-line" x1="0" y1="0" x2="0" y2="6" />
                </pattern>
                <linearGradient id="gc-fade" x1="0" x2="1" y1="0" y2="0">
                    <stop offset="0" class="fade-from" />
                    <stop offset="1" class="fade-to" />
                </linearGradient>
            </defs>
            {ticks}
            {shapes}
            <line class="today" x1=today x2=today y1=0 y2=h />
            {arrows}
            {tag}
        </svg>
    }
}

fn offset_x(data: &GanttData, d: Option<Date>, geo: Geo) -> Option<f64> {
    let o = crate::gantt::offset_of(data, d?)?;
    Some(geo.x(o as f64))
}

/// A project's or objective's row: the room, the overrun, the work and the flag, with handles.
#[allow(clippy::too_many_arguments)]
fn band_shapes(
    ctx: Ctx,
    y: f64,
    m: Marks,
    hue: Option<u16>,
    room: Option<&'static str>,
    ongoing: bool,
    chart_w: f64,
    tip: String,
    drag: Option<(What, bool, bool)>,
) -> AnyView {
    let mid = y + ROW_H / 2.0;
    let style = hue.map(|h| format!("--obj-h: {h}")).unwrap_or_default();
    let room_rect = m.room.map(|(x, wd)| {
        view! { <rect class=room.unwrap_or("room wide") x=x y=y + 4.0 width=wd height=ROW_H - 8.0 /> }
    });
    let over_rect = m.over.map(|(x, wd)| {
        view! { <rect class="over" x=x y=y + 4.0 width=wd height=ROW_H - 8.0 /> }
    });
    let fade = ongoing.then(|| {
        let from = m.work.map_or(0.0, |(x, wd)| x + wd);
        view! { <rect class="fade" x=from y=mid - 3.0 width=(chart_w - from).max(0.0) height=6 /> }
    });
    // Dragging the whole bar needs both ends to be dates.
    let (what, can_target, can_move) = match drag {
        Some((w, a, b)) => (Some(w), a, b),
        None => (None, false, false),
    };
    let work = m.work.map(|(x, wd)| {
        let body = view! {
            <rect class=if hue.is_some() { "summary objc" } else { "summary" }
                  x=x y=mid - 3.0 width=wd height=6 />
            <path class="cap" d=format!("M{x} {}v8M{} {}v8", mid - 7.0, x + wd, mid - 7.0) />
        };
        match (what, can_move) {
            (Some(What::Project { id, .. }), true) => view! {
                {body}
                <rect class="grip" x=x y=y + 3.0 width=wd height=ROW_H - 6.0
                      on:mousedown=press(ctx, What::Project { id, part: BandPart::Both }) />
            }
            .into_any(),
            _ => body.into_any(),
        }
    });
    let flag = m.target_x.map(|x| {
        let handle = what.filter(|_| can_target).map(|w| {
            view! {
                <rect class="handle" x=x - 6.0 y=y width=12 height=ROW_H
                      on:mousedown=press(ctx, w) />
            }
        });
        view! {
            <line class="target" x1=x x2=x y1=y + 2.0 y2=y + ROW_H - 2.0 />
            <path class="flag" d=format!("M{x} {}h7l-2 3 2 3h-7z", y + 3.0) />
            {handle}
        }
    });
    let start = m.start_x.map(|x| {
        view! { <path class="pin" d=format!("M{x} {}v{}", y + 3.0, ROW_H - 6.0) /> }
    });
    view! {
        <g style=style>
            {room_rect}{over_rect}{fade}{work}{start}{flag}
            <title>{tip}</title>
        </g>
    }
    .into_any()
}

/// The handle on a project's start pin.
fn project_pin(ctx: Ctx, p: &GanttProject, y: f64, m: Marks) -> Option<AnyView> {
    let x = m.start_x?;
    Some(
        view! {
            <rect class="handle" x=x - 6.0 y=y width=12 height=ROW_H
                  on:mousedown=press(ctx, What::Project { id: p.id, part: BandPart::Start }) />
        }
        .into_any(),
    )
}

/// One task's row: its bar, the slack after it, the due date, and the handles to drag it.
#[allow(clippy::too_many_arguments)]
fn task_shapes(
    ctx: Ctx,
    data: &GanttData,
    geo: Geo,
    t: &GanttTask,
    y: f64,
    moved: Option<(f64, f64)>,
    ghost: Option<(Uuid, f64, f64)>,
    pushed: Option<u32>,
    limit: Option<f64>,
    dim: bool,
    resizable: bool,
) -> AnyView {
    let _ = data;
    let s = &t.schedule;
    let id = s.id;
    let (es, ef) = moved.unwrap_or((s.es, s.ef));
    let x = geo.x(es);
    let wd = (geo.x(ef) - x).max(MIN_BAR_W);
    let by = y + (ROW_H - BAR_H) / 2.0;
    let class = format!(
        "{}{}{}",
        bar_class(t),
        if moved.is_some() { " moving" } else { "" },
        if pushed.is_some() { " pushed" } else { "" }
    );
    let movable = !s.done;
    let late = (s.late_by_days.is_some() && s.ef > s.lf + 1e-9 && moved.is_none()).then(|| {
        let from = geo.x(s.lf).max(x);
        view! { <rect class="late-tail" x=from y=by width=(x + wd - from).max(0.0) height=BAR_H pointer-events="none" /> }
    });
    let slack = (!s.done && !s.critical && s.slack_days > 1e-9).then(|| {
        view! { <line class="slack" x1=x + wd x2=geo.x(ef + s.slack_days) y1=by + BAR_H / 2.0 y2=by + BAR_H / 2.0 /> }
    });
    let critical = (s.critical && !s.done)
        .then(|| view! { <rect class="crit" x=x y=by + BAR_H + 1.0 width=wd height=2 pointer-events="none" /> });
    let due = t.due_offset.map(|o| {
        let dx = geo.x(o);
        view! {
            <line class=if t.overdue { "due overdue" } else { "due" } x1=dx x2=dx y1=y + 3.0 y2=y + ROW_H - 3.0>
            </line>
        }
    });
    let ghost = ghost.map(|(_, ges, gef)| {
        let gx = geo.x(ges);
        view! { <rect class="ghost" x=gx y=by width=(geo.x(gef) - gx).max(MIN_BAR_W) height=BAR_H rx="2" /> }
    });
    let limit = limit.map(|lx| view! { <line class="limit" x1=lx x2=lx y1=y y2=y + ROW_H /> });
    let chip = pushed.map(|d| {
        view! { <text class="pushed" x=x + wd + 6.0 y=by + BAR_H - 2.0>{format!("+{d}d")}</text> }
    });
    let handle_w = HANDLE_W.min(wd / 3.0);
    let handles = (movable && resizable).then(|| {
        view! {
            <rect class="handle" x=x - 1.0 y=by - 2.0 width=handle_w + 1.0 height=BAR_H + 4.0
                  on:mousedown=press(ctx, What::Task { id, edge: DragEdge::Start }) />
            <rect class="handle" x=x + wd - handle_w y=by - 2.0 width=handle_w + 1.0 height=BAR_H + 4.0
                  on:mousedown=press(ctx, What::Task { id, edge: DragEdge::End }) />
        }
    });
    let body = view! {
        <rect class=class x=x y=by width=wd height=BAR_H rx="2"
              on:mousedown=press(ctx, What::Task { id, edge: DragEdge::Move }) />
    };
    view! {
        <g class=if dim { "task dim" } else { "task" }
           on:mouseenter=move |_| ctx.hover.set(Some(id))
           on:mouseleave=move |_| ctx.hover.set(None)>
            {limit}{ghost}{slack}{body}{late}{critical}{due}{handles}{chip}
            <title>{task_tip(t)}</title>
        </g>
    }
    .into_any()
}
