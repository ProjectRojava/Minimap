//! The Gantt screen (spec 57): the plan on a calendar, objectives → projects → tasks, with the
//! links between tasks drawn as arrows, a bar coloured by where its task stands, and bars you can
//! drag. The tree, the dates and the drag rules come from `get_gantt` / `preview_task_drag` /
//! `apply_task_drag` (core `gantt`); the arithmetic of pixels and days is `crate::gantt`.

use std::collections::{HashMap, HashSet};

use leptos::{
    ev, prelude::*, tachys::html::attribute::custom::custom_attribute, task::spawn_local,
};
use minimap_types::{
    AppError, Date, DragEdge, DragPreview, GanttData, GanttFilter, GanttLinkKind, GanttTask,
    NodeRef, NodeType, Patch, TaskDrag, UpdateObjective, UpdateProject, Uuid,
};

use crate::{
    api,
    calendar::{format_ymd, parse_ymd},
    components::date_field::DateField,
    components::{
        form::{BUTTON, BUTTON_ON},
        objective_colour::use_objective_colours,
        page::{Hints, PageHeader, Tone, FILTER_BAR},
    },
    gantt::{
        add_project_text, add_task_line, add_task_text, arrow_head, axis, band_dates,
        band_dropped_text, band_handles, band_step, band_tip, bar_class, bar_name, body_hit,
        can_open, can_resize, cells, chain_set, clamp_label_w, count_parts, counts_w,
        critical_pairs, drag_label, dropped_text, elbow, empty_text, fade_span, filtered,
        find_group, find_matches, find_project, find_task, fit_label_w, fold_text, group_progress,
        group_title, is_open, link_tone, marks, over_weeks, progress_w, project_counts, room_chip,
        room_class, row_label, row_open, rows, shift_band, shows_names, snap_days, sorted_tasks,
        stop_reason, strip_box, strip_scroll, strip_x, task_candidate, task_hits,
        task_step_refusal, task_tip, tasks_shown, today_scroll, tree, unfold_for,
        with_all_projects, working_day_on_or_after, zoom, zoom_scroll, BandPart, Count, Density,
        Dims, Geo, LinkMode, LinkTone, Marks, Row, RowKind, Scale, SortKey, TasksShown, ViewFilter,
        CLICK_SLOP, HEAD_H, HIT, LABEL_W, MIN_BAR_W, NO_OBJECTIVE, UNASSIGNED,
    },
    state::{DataVersion, PaletteOpen, PaletteText, Selection, Toasts},
};

/// Where the label column's width is remembered on this device (spec 58).
const LABEL_W_KEY: &str = "minimap.gantt.label-width";
/// Where the row height (compact or comfortable) is remembered on this device (spec 58).
const DENSITY_KEY: &str = "minimap.gantt.density";

fn load_density() -> Density {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(DENSITY_KEY).ok().flatten())
        .map_or(Density::Compact, |v| Density::from_id(&v))
}

fn save_density(d: Density) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(DENSITY_KEY, d.id());
    }
}

/// Whether the system asks for less motion: then scrolling jumps instead of gliding.
fn reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| {
            w.match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .is_some_and(|m| m.matches())
}

/// The DOM id of a row, for `aria-activedescendant` (the chart) and scrolling it into view.
fn row_dom_id(key: Uuid) -> String {
    format!("gr-{key}")
}

/// Chart or table (spec 58): the same rows, drawn or listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Chart,
    Table,
}

fn load_label_w() -> f64 {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(LABEL_W_KEY).ok().flatten())
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(LABEL_W, clamp_label_w)
}

fn save_label_w(w: f64) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(LABEL_W_KEY, &format!("{}", w.round()));
    }
}

/// Panning the chart by dragging empty space: where the pointer and the scroll were.
#[derive(Clone, Copy)]
struct Pan {
    x0: f64,
    y0: f64,
    left0: i32,
    top0: i32,
}

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
    scale: RwSignal<Scale>,
    /// The scroller's inner width, measured: what *Fit* fits into, with the label column.
    scroller_w: RwSignal<f64>,
    label_w: RwSignal<f64>,
    /// The row and working day under the pointer, lit across both sides and in the header.
    hover_row: RwSignal<Option<usize>>,
    hover_day: RwSignal<Option<i64>>,
    pan: RwSignal<Option<Pan>>,
    /// Dragging the label column's edge: where the pointer and the width were.
    resizing: RwSignal<Option<(f64, f64)>>,
    show_done: RwSignal<bool>,
    mode: RwSignal<LinkMode>,
    toggled: RwSignal<HashSet<Uuid>>,
    hover: RwSignal<Option<Uuid>>,
    drag: RwSignal<Option<DragState>>,
    /// Tasks whose due date a drop pushed, and by how many days: marked until the next drag.
    pushed: RwSignal<HashMap<Uuid, u32>>,
    shown: RwSignal<Option<GanttData>>,
    selection: Selection,
    palette: (PaletteText, PaletteOpen),
    toasts: Toasts,
    version: DataVersion,
    density: RwSignal<Density>,
    /// The chart has the keyboard: the open row wears a focus ring.
    focused: RwSignal<bool>,
    /// What the live region says (folds and refusals), for screen readers.
    said: RwSignal<String>,
    /// The item whose date box is open.
    dates: RwSignal<Option<NodeRef>>,
    view: RwSignal<ViewMode>,
    /// The table's sort; none keeps the tree.
    sort: RwSignal<Option<(SortKey, bool)>>,
    scroller: leptos::prelude::NodeRef<leptos::html::Div>,
    /// The chart body, for turning the pointer into a working day.
    body: leptos::prelude::NodeRef<leptos::html::Div>,
    /// The plan as the backend sent it; `shown` is it narrowed by `filter` (spec 58).
    raw: RwSignal<Option<GanttData>>,
    filter: RwSignal<ViewFilter>,
    /// *Rows: by person*: read-only rows of people and their tasks.
    by_person: RwSignal<bool>,
    /// Weeks people are over capacity, as spans on the axis (shown by person).
    overload: RwSignal<crate::gantt::OverWeeks>,
    /// The find box: its text (none: closed), what matches, and which match is current.
    find: RwSignal<Option<String>>,
    found: RwSignal<Vec<Uuid>>,
    found_at: RwSignal<usize>,
    /// How far the chart is scrolled sideways, for the overview strip.
    scroll_left: RwSignal<f64>,
    /// Dragging the overview strip's box.
    strip_drag: RwSignal<bool>,
    /// Drawing a new task on a project's add row.
    adding: RwSignal<Option<Adding>>,
}

/// A task being drawn on a project's add row (spec 58).
#[derive(Clone, Debug, PartialEq)]
struct Adding {
    project: Uuid,
    slug: String,
    /// Where the pointer went down, and the working day there.
    x0: f64,
    from: i64,
    to: i64,
}

impl Ctx {
    /// The chart's visible width, right of the label column.
    fn view_w(self) -> f64 {
        (self.scroller_w.get() - self.label_w.get()).max(0.0)
    }

    /// Pixels per working day at the current scale.
    fn ppd(self) -> f64 {
        let days = self.shown.with(|d| d.as_ref().map_or(0, |d| d.days.len()));
        self.scale.get().ppd(days, self.view_w())
    }

    fn ppd_untracked(self) -> f64 {
        untrack(|| self.ppd())
    }

    fn dims(self) -> Dims {
        self.density.get().dims()
    }

    /// Says `text` in the live region (cleared first, so the same words are said again).
    fn say(self, text: String) {
        self.said.set(String::new());
        let said = self.said;
        set_timeout(move || said.set(text), std::time::Duration::from_millis(40));
    }

    /// A move that can't be made: a message on screen (the toasts are a live region too).
    fn refuse(self, text: String) {
        self.toasts.info(text);
    }

    fn set_hover(self, row: Option<usize>, day: Option<i64>) {
        if self.hover_row.get_untracked() != row {
            self.hover_row.set(row);
        }
        if self.hover_day.get_untracked() != day {
            self.hover_day.set(day);
        }
    }
}

/// Scrolls the chart so today sits a sixth of the way in.
fn scroll_to_today(ctx: Ctx, el: &web_sys::HtmlDivElement) {
    let Some(x) = ctx
        .shown
        .with_untracked(|d| d.as_ref().map(|d| Geo::new(d, ctx.ppd_untracked()).x(0.0)))
    else {
        return;
    };
    let view_w = untrack(|| ctx.view_w());
    let left = today_scroll(x, view_w);
    if reduced_motion() {
        el.set_scroll_left(left as i32);
    } else {
        let opts = web_sys::ScrollToOptions::new();
        opts.set_left(left);
        opts.set_behavior(web_sys::ScrollBehavior::Smooth);
        el.scroll_to_with_scroll_to_options(&opts);
    }
}

/// Scrolls the scroller just enough to show the row of `key` under the sticky header.
fn scroll_row_into_view(ctx: Ctx, el: &web_sys::HtmlDivElement, key: Uuid) {
    let Some(row) = document().get_element_by_id(&row_dom_id(key)) else {
        return;
    };
    let header = match ctx.view.get_untracked() {
        ViewMode::Chart => HEAD_H,
        ViewMode::Table => TABLE_HEAD_H,
    };
    let r = row.get_bounding_client_rect();
    let v = el.get_bounding_client_rect();
    let top_free = v.top() + header;
    let bottom_free = v.top() + f64::from(el.client_height());
    if r.top() < top_free {
        el.set_scroll_top(el.scroll_top() - (top_free - r.top()).ceil() as i32);
    } else if r.bottom() > bottom_free {
        el.set_scroll_top(el.scroll_top() + (r.bottom() - bottom_free).ceil() as i32);
    }
}

/// The table's header row is about this tall.
const TABLE_HEAD_H: f64 = 30.0;

/// A press on a project's add row: the new task's first day is the one under the pointer.
fn start_adding(ctx: Ctx, project: Uuid, slug: String, e: ev::MouseEvent) {
    if e.button() != 0 {
        return;
    }
    e.prevent_default();
    e.stop_propagation();
    let Some(el) = ctx.body.get_untracked() else {
        return;
    };
    let x = f64::from(e.client_x()) - el.get_bounding_client_rect().left();
    let Some(day) = ctx.shown.with_untracked(|d| {
        d.as_ref()
            .map(|d| Geo::new(d, ctx.ppd_untracked()).day_under(x))
    }) else {
        return;
    };
    // Like a dragged bar, a new task can't start before today (offset 0).
    let day = day.max(0);
    ctx.adding.set(Some(Adding {
        project,
        slug,
        x0: f64::from(e.client_x()),
        from: day,
        to: day,
    }));
}

/// The find box's DOM id, so `Ctrl/Cmd+F` can put the cursor in it.
const FIND_ID: &str = "gantt-find";

/// The overview strip followed the pointer: scroll so its box is centred there.
fn strip_follow(ctx: Ctx, client_x: f64) {
    let Some(el) = ctx.scroller.get_untracked() else {
        return;
    };
    let r = el.get_bounding_client_rect();
    let strip_w = f64::from(el.client_width());
    let chart_w = untrack(|| {
        let ppd = ctx.ppd();
        ctx.shown.with(|d| {
            d.as_ref()
                .map_or(0.0, |d| crate::gantt::chart_width(d, ppd))
        })
    });
    let view_w = untrack(|| ctx.view_w());
    let left = strip_scroll(client_x - r.left(), strip_w, chart_w, view_w);
    el.set_scroll_left(left as i32);
    // The box follows at once, without waiting for the scroll event.
    ctx.scroll_left.set(f64::from(el.scroll_left()));
}

/// The item an id found on the chart stands for.
fn node_for(d: &GanttData, id: Uuid) -> Option<NodeRef> {
    if find_task(d, id).is_some() {
        Some(NodeRef::new(NodeType::Task, id))
    } else if find_project(d, id).is_some() {
        Some(NodeRef::new(NodeType::Project, id))
    } else if id != NO_OBJECTIVE && find_group(d, id).is_some() {
        Some(NodeRef::new(NodeType::Objective, id))
    } else {
        None
    }
}

/// Finds `query` on the chart: unfolds what hides the matches and goes to the first.
fn run_find(ctx: Ctx, query: String) {
    let show_done = ctx.show_done.get_untracked();
    let found = ctx
        .shown
        .with_untracked(|d| d.as_ref().map(|d| find_matches(d, &query, show_done)))
        .unwrap_or_default();
    if !found.is_empty() && !ctx.by_person.get_untracked() {
        let toggled = ctx.shown.with_untracked(|d| {
            d.as_ref()
                .map(|d| unfold_for(d, &ctx.toggled.get_untracked(), &found))
        });
        if let Some(t) = toggled {
            ctx.toggled.set(t);
        }
    }
    ctx.find.set(Some(query));
    ctx.found.set(found);
    ctx.found_at.set(0);
    go_to_found(ctx, 0);
}

/// Opens match `i` of the find box and brings its row into view.
fn go_to_found(ctx: Ctx, i: usize) {
    let Some(id) = ctx.found.with_untracked(|f| f.get(i).copied()) else {
        return;
    };
    ctx.found_at.set(i);
    if let Some(node) = ctx
        .shown
        .with_untracked(|d| d.as_ref().and_then(|d| node_for(d, id)))
    {
        ctx.selection.open(node);
    }
    set_timeout(
        move || {
            if let Some(el) = ctx.scroller.get_untracked() {
                scroll_row_into_view(ctx, &el, id);
            }
        },
        std::time::Duration::from_millis(40),
    );
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

    let scroller = leptos::prelude::NodeRef::<leptos::html::Div>::new();
    let ctx = Ctx {
        scale: RwSignal::new(Scale::Days),
        scroller_w: RwSignal::new(0.0),
        label_w: RwSignal::new(load_label_w()),
        hover_row: RwSignal::new(None),
        hover_day: RwSignal::new(None),
        pan: RwSignal::new(None),
        resizing: RwSignal::new(None),
        show_done: RwSignal::new(false),
        mode: RwSignal::new(LinkMode::Focus),
        toggled: RwSignal::new(HashSet::new()),
        hover: RwSignal::new(None),
        drag: RwSignal::new(None),
        pushed: RwSignal::new(HashMap::new()),
        shown: RwSignal::new(None),
        selection,
        palette: (
            expect_context::<PaletteText>(),
            expect_context::<PaletteOpen>(),
        ),
        toasts,
        version,
        density: RwSignal::new(load_density()),
        focused: RwSignal::new(false),
        said: RwSignal::new(String::new()),
        dates: RwSignal::new(None),
        view: RwSignal::new(ViewMode::Chart),
        sort: RwSignal::new(None),
        scroller,
        body: leptos::prelude::NodeRef::new(),
        raw: RwSignal::new(None),
        filter: RwSignal::new(ViewFilter::default()),
        by_person: RwSignal::new(false),
        overload: RwSignal::new(Default::default()),
        find: RwSignal::new(None),
        found: RwSignal::new(Vec::new()),
        found_at: RwSignal::new(0),
        scroll_left: RwSignal::new(0.0),
        strip_drag: RwSignal::new(false),
        adding: RwSignal::new(None),
    };
    let loaded = LocalResource::new(move || {
        version.track();
        api::get_gantt(GanttFilter {
            mine: orientation.is_mine(),
        })
    });
    let error = RwSignal::new(None::<AppError>);
    let scrolled = StoredValue::new(false);
    // The last answer stays on screen while the next one is worked out.
    Effect::new(move |_| match loaded.get() {
        Some(Ok(d)) => {
            error.set(None);
            ctx.raw.set(Some(d));
        }
        Some(Err(e)) => error.set(Some(e)),
        None => {}
    });
    // What is drawn: the plan narrowed by the filters.
    Effect::new(move |_| {
        let f = ctx.filter.get();
        let shown = ctx.raw.with(|d| d.as_ref().map(|d| filtered(d, &f)));
        if shown.is_some() {
            ctx.shown.set(shown);
        }
    });
    // By person: who is over capacity, and when (the Capacity screen's numbers, spec 17).
    Effect::new(move |_| {
        if !ctx.by_person.get() {
            return;
        }
        let Some((from, to)) = ctx.raw.with(|d| {
            d.as_ref()
                .and_then(|d| Some((*d.days.first()?, *d.days.last()?)))
        }) else {
            return;
        };
        let view = orientation.view();
        spawn_local(async move {
            match api::get_capacity(Some(from), Some(to), None, view).await {
                Ok(cap) => {
                    let spans = ctx
                        .raw
                        .with_untracked(|d| d.as_ref().map(|d| over_weeks(d, &cap)))
                        .unwrap_or_default();
                    ctx.overload.set(spans);
                }
                Err(e) => ctx.toasts.error(&e),
            }
        });
    });
    // The chart's width, for *Fit* and for bringing today into view: measured when the window
    // changes size, and after the detail pane opens or closes (it slides).
    let measure = move || {
        if let Some(el) = scroller.get_untracked() {
            let w = f64::from(el.client_width());
            if (w - ctx.scroller_w.get_untracked()).abs() > 0.5 {
                ctx.scroller_w.set(w);
            }
        }
    };
    let resized = window_event_listener(ev::resize, move |_| measure());
    on_cleanup(move || resized.remove());
    Effect::new(move |_| {
        selection.0.track();
        ctx.shown.with(|d| d.is_some());
        scroller.track();
        measure();
        set_timeout(measure, std::time::Duration::from_millis(60));
        set_timeout(measure, std::time::Duration::from_millis(350));
    });
    // The first time, bring today about a sixth of the way into the view.
    Effect::new(move |_| {
        let Some(el) = scroller.get() else { return };
        if scrolled.get_value() || ctx.scroller_w.get() <= 0.0 {
            return;
        }
        if ctx.shown.with(|d| d.is_some()) {
            scrolled.set_value(true);
            let el = el.clone();
            request_animation_frame(move || scroll_to_today(ctx, &el));
        }
    });

    // ------------------------------------------------------------------ dragging
    let mouse_move = window_event_listener(ev::mousemove, move |e: ev::MouseEvent| {
        if let Some((x0, w0)) = ctx.resizing.get_untracked() {
            ctx.label_w
                .set(clamp_label_w(w0 + f64::from(e.client_x()) - x0));
            return;
        }
        if let Some(a) = ctx.adding.get_untracked() {
            // Like a dragged bar, a new task can't start before today (offset 0).
            let to =
                (a.from + snap_days(f64::from(e.client_x()) - a.x0, ctx.ppd_untracked())).max(0);
            if to != a.to {
                ctx.adding.set(Some(Adding { to, ..a }));
            }
            return;
        }
        if ctx.strip_drag.get_untracked() {
            strip_follow(ctx, f64::from(e.client_x()));
            return;
        }
        if let Some(p) = ctx.pan.get_untracked() {
            if let Some(el) = scroller.get_untracked() {
                el.set_scroll_left(p.left0 - (f64::from(e.client_x()) - p.x0) as i32);
                el.set_scroll_top(p.top0 - (f64::from(e.client_y()) - p.y0) as i32);
            }
            return;
        }
        let Some(cur) = ctx.drag.get_untracked() else {
            return;
        };
        let dx = f64::from(e.client_x()) - cur.x0;
        // Rows by person only read: a press stays a click.
        if (!cur.moved && dx.abs() < CLICK_SLOP) || ctx.by_person.get_untracked() {
            return;
        }
        let delta = snap_days(dx, ctx.ppd_untracked());
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
        if ctx.resizing.get_untracked().is_some() {
            ctx.resizing.set(None);
            save_label_w(ctx.label_w.get_untracked());
            return;
        }
        if ctx.pan.get_untracked().is_some() {
            ctx.pan.set(None);
            return;
        }
        if ctx.strip_drag.get_untracked() {
            ctx.strip_drag.set(false);
            return;
        }
        if let Some(a) = ctx.adding.get_untracked() {
            ctx.adding.set(None);
            // Nothing is made here: quick-add opens with the project and the dates filled in.
            if let Some(line) = ctx
                .shown
                .with_untracked(|d| d.as_ref().map(|d| add_task_line(d, &a.slug, a.from, a.to)))
            {
                let (prefill, palette) = ctx.palette;
                prefill.open_with(palette, line);
            }
            return;
        }
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
                // A click on a bar also opens its date box: the way to change dates without
                // dragging (spec 58). Rows by person only read.
                if !ctx.by_person.get_untracked() {
                    ctx.dates.set(Some(node));
                }
            }
            return;
        }
        drop_it(ctx, cur);
    });
    on_cleanup(move || mouse_up.remove());
    let legend_open = RwSignal::new(false);
    let keys = window_event_listener(ev::keydown, move |e: ev::KeyboardEvent| {
        // Ctrl/Cmd+F finds on the chart (spec 58); `/` stays the global search.
        if (e.ctrl_key() || e.meta_key()) && !e.alt_key() && e.key().eq_ignore_ascii_case("f") {
            e.prevent_default();
            if ctx.find.get_untracked().is_none() {
                ctx.find.set(Some(String::new()));
            }
            set_timeout(
                || {
                    if let Some(el) = document().get_element_by_id(FIND_ID).and_then(|b| {
                        wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlInputElement>(b).ok()
                    }) {
                        let _ = el.focus();
                        el.select();
                    }
                },
                std::time::Duration::from_millis(30),
            );
            return;
        }
        if e.key() == "Escape" && ctx.adding.get_untracked().is_some() {
            ctx.adding.set(None);
        } else if e.key() == "Escape" && ctx.drag.get_untracked().is_some() {
            ctx.drag.set(None);
        } else if e.key() == "Escape" && legend_open.get_untracked() {
            legend_open.set(false);
        } else if e.key() == "Escape" && ctx.dates.get_untracked().is_some() {
            ctx.dates.set(None);
            if let Some(el) = scroller.get_untracked() {
                let _ = el.focus();
            }
        }
    });
    on_cleanup(move || keys.remove());

    // ------------------------------------------------------------------- the pieces
    // A new scale keeps the day at the left edge of the view where it is.
    let set_scale = move |new: Scale| {
        let old_ppd = ctx.ppd_untracked();
        ctx.scale.set(new);
        let Some(el) = scroller.get_untracked() else {
            return;
        };
        let day = f64::from(el.scroll_left()) / old_ppd;
        let new_ppd = ctx.ppd_untracked();
        request_animation_frame(move || el.set_scroll_left((day * new_ppd) as i32));
    };
    let scale_buttons = Scale::ALL
        .into_iter()
        .map(|g| {
            view! {
                <button class=move || format!("{SEGMENT} {}", if ctx.scale.get() == g { SEGMENT_ON } else { "" })
                        title=g.hint() aria-pressed=move || (ctx.scale.get() == g).to_string()
                        on:click=move |_| set_scale(g)>{g.label()}</button>
            }
        })
        .collect_view();
    let mode_buttons = LinkMode::ALL
        .into_iter()
        .map(|m| {
            view! {
                <button class=move || format!("{SEGMENT} {}", if ctx.mode.get() == m { SEGMENT_ON } else { "" })
                        title=m.hint() aria-pressed=move || (ctx.mode.get() == m).to_string()
                        on:click=move |_| ctx.mode.set(m)>{m.label()}</button>
            }
        })
        .collect_view();
    let view_buttons = [(ViewMode::Chart, "Chart"), (ViewMode::Table, "Table")]
        .into_iter()
        .map(|(v, label)| {
            view! {
                <button class=move || format!("{SEGMENT} {}", if ctx.view.get() == v { SEGMENT_ON } else { "" })
                        aria-pressed=move || (ctx.view.get() == v).to_string()
                        title=if v == ViewMode::Chart { "The plan on a calendar" } else { "The same rows as a table of dates" }
                        on:click=move |_| ctx.view.set(v)>{label}</button>
            }
        })
        .collect_view();
    let toggle_density = move |_| {
        let next = match ctx.density.get_untracked() {
            Density::Compact => Density::Comfortable,
            Density::Comfortable => Density::Compact,
        };
        ctx.density.set(next);
        save_density(next);
    };
    let chart_only = move || {
        if ctx.view.get() == ViewMode::Chart {
            "flex items-center gap-1.5"
        } else {
            "hidden"
        }
    };
    // The open row, for screen readers following the keyboard through the tree.
    let active_row = move || {
        let open = ctx.selection.0.get()?;
        let shown = ctx.shown.with(|d| {
            d.as_ref().is_some_and(|d| {
                tree(
                    d,
                    &ctx.toggled.get(),
                    ctx.show_done.get(),
                    ctx.by_person.get(),
                )
                .iter()
                .any(|r| r.key == open.id)
            })
        });
        shown.then(|| row_dom_id(open.id))
    };
    // Filters (spec 58): one person, one objective, what needs a look.
    let person_options = move || {
        ctx.raw.with(|d| {
            d.as_ref()
                .map(|d| {
                    d.people
                        .iter()
                        .map(|p| (p.id, p.title.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let objective_options = move || {
        ctx.raw.with(|d| {
            d.as_ref()
                .map(|d| {
                    d.groups
                        .iter()
                        .map(|g| (crate::gantt::group_key(g), group_title(g)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let pick = |value: String| Uuid::parse_str(&value).ok();
    let find_count = move || {
        let n = ctx.found.with(|f| f.len());
        let q = ctx.find.with(|f| f.clone().unwrap_or_default());
        if q.trim().is_empty() {
            String::new()
        } else if n == 0 {
            "No match".to_owned()
        } else {
            format!("{} of {n}", ctx.found_at.get() + 1)
        }
    };
    // Only opening or closing the find box redraws it, not typing in it.
    let find_open = Memo::new(move |_| ctx.find.with(Option::is_some));
    let step_found = move |dir: isize| {
        let n = ctx.found.with_untracked(|f| f.len());
        if n == 0 {
            return;
        }
        let at = ctx.found_at.get_untracked() as isize;
        go_to_found(ctx, (at + dir).rem_euclid(n as isize) as usize);
    };
    let go_today = move |_| {
        if let Some(el) = scroller.get_untracked() {
            scroll_to_today(ctx, &el);
        }
    };
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
                <Hints keys=&[("←/→", "fold"), ("↑/↓", "move"), ("t", "today"), ("Alt+←/→", "shift a task")] />
            </PageHeader>
            <crate::components::orientation::MineBanner />
            <div class=format!("{FILTER_BAR} relative gap-x-4")>
                <span class="flex items-center gap-1.5" role="group" aria-label="View">
                    <span class=GROUP_LABEL>"View"</span>
                    <span class=SEGMENTS>{view_buttons}</span>
                </span>
                <span class=chart_only role="group" aria-label="Scale">
                    <span class=GROUP_LABEL>"Scale"</span>
                    <span class=SEGMENTS>{scale_buttons}</span>
                    <button class=BUTTON title="Bring today into view (t)" on:click=go_today>"Today"</button>
                </span>
                <span class="flex items-center gap-1.5" role="group" aria-label="Rows">
                    <span class=GROUP_LABEL>"Rows"</span>
                    <span class=SEGMENTS>
                        <button class=move || format!("{SEGMENT} {}", if shown_now() == TasksShown::None { SEGMENT_ON } else { "" })
                                title="Fold every project to one row"
                                on:click=move |_| set_projects(false)>"Projects only"</button>
                        <button class=move || format!("{SEGMENT} {}", if shown_now() == TasksShown::All { SEGMENT_ON } else { "" })
                                title="Open every project to its tasks"
                                on:click=move |_| set_projects(true)>"All tasks"</button>
                    </span>
                    <label class="ml-1 flex items-center gap-1 text-[11px] text-muted">
                        <input type="checkbox" prop:checked=move || ctx.show_done.get()
                               on:change=move |ev| ctx.show_done.set(event_target_checked(&ev)) />
                        "Show finished"
                    </label>
                    <button class=move || format!("{BUTTON} {}", if ctx.by_person.get() { BUTTON_ON } else { "" })
                            aria-pressed=move || ctx.by_person.get().to_string()
                            title="One row per person with their tasks under them, weeks over their capacity shaded. Read-only."
                            on:click=move |_| {
                                ctx.by_person.update(|b| *b = !*b);
                                ctx.dates.set(None);
                            }>"By person"</button>
                    <button class=move || format!("{BUTTON} {}", if ctx.density.get() == Density::Comfortable { BUTTON_ON } else { "" })
                            aria-pressed=move || (ctx.density.get() == Density::Comfortable).to_string()
                            title="Taller rows and bars, easier to point at and to read"
                            on:click=toggle_density>"Comfortable"</button>
                </span>
                <span class=chart_only role="group" aria-label="Arrows">
                    <span class=GROUP_LABEL>"Arrows"</span>
                    <span class=SEGMENTS>{mode_buttons}</span>
                </span>
                <span class="flex items-center gap-1.5" role="group" aria-label="Filters">
                    <span class=GROUP_LABEL>"Show"</span>
                    <select class=FILTER_SELECT aria-label="Only one person's tasks"
                            on:change=move |e| { let v = pick(event_target_value(&e)); ctx.filter.update(|f| f.person = v); }>
                        <option value="" selected=move || ctx.filter.with(|f| f.person.is_none())>"Everyone"</option>
                        {move || person_options().into_iter().map(|(id, name)| view! {
                            <option value=id.to_string() selected=move || ctx.filter.with(|f| f.person == Some(id))>{name}</option>
                        }).collect_view()}
                    </select>
                    <select class=FILTER_SELECT aria-label="Only one objective"
                            on:change=move |e| { let v = pick(event_target_value(&e)); ctx.filter.update(|f| f.objective = v); }>
                        <option value="" selected=move || ctx.filter.with(|f| f.objective.is_none())>"Every objective"</option>
                        {move || objective_options().into_iter().map(|(id, name)| view! {
                            <option value=id.to_string() selected=move || ctx.filter.with(|f| f.objective == Some(id))>{name}</option>
                        }).collect_view()}
                    </select>
                    <button class=move || format!("{BUTTON} {}", if ctx.filter.with(|f| f.needs_a_look) { BUTTON_ON } else { "" })
                            aria-pressed=move || ctx.filter.with(|f| f.needs_a_look).to_string()
                            title="Only projects that are late, out of room, or have blocked work"
                            on:click=move |_| ctx.filter.update(|f| f.needs_a_look = !f.needs_a_look)>"Needs a look"</button>
                </span>
                {move || match find_open.get() {
                    false => view! {
                        <button class=BUTTON title="Find a row on the chart (Ctrl/Cmd+F)"
                                on:click=move |_| {
                                    ctx.find.set(Some(String::new()));
                                    set_timeout(|| {
                                        if let Some(el) = document().get_element_by_id(FIND_ID)
                                            .and_then(|b| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlInputElement>(b).ok()) {
                                            let _ = el.focus();
                                        }
                                    }, std::time::Duration::from_millis(30));
                                }>"Find"</button>
                    }.into_any(),
                    true => view! {
                        <span class="flex items-center gap-1" role="search">
                            <input id=FIND_ID class=FILTER_SELECT type="search" placeholder="Find a name"
                                   aria-label="Find a name on the chart"
                                   prop:value=move || ctx.find.get().unwrap_or_default()
                                   on:input=move |e| run_find(ctx, event_target_value(&e))
                                   on:keydown=move |e: ev::KeyboardEvent| {
                                       match e.key().as_str() {
                                           "Enter" => { e.prevent_default(); step_found(if e.shift_key() { -1 } else { 1 }); }
                                           "Escape" => {
                                               e.stop_propagation();
                                               ctx.find.set(None);
                                               ctx.found.set(Vec::new());
                                               if let Some(el) = ctx.scroller.get_untracked() { let _ = el.focus(); }
                                           }
                                           _ => {}
                                       }
                                   } />
                            <span class="min-w-[4.5rem] text-[11px] text-muted" aria-live="polite">{find_count}</span>
                            <button class=BUTTON aria-label="Previous match" title="Previous (Shift+Enter)" on:click=move |_| step_found(-1)>"↑"</button>
                            <button class=BUTTON aria-label="Next match" title="Next (Enter)" on:click=move |_| step_found(1)>"↓"</button>
                            <button class="text-faint hover:text-fg" aria-label="Close find"
                                    on:click=move |_| { ctx.find.set(None); ctx.found.set(Vec::new()); }>"✕"</button>
                        </span>
                    }.into_any(),
                }}
                <span class="ml-auto flex items-center gap-3">
                    <span class="text-[11px] text-muted">{summary}</span>
                    <button class=move || format!("{BUTTON} {}", if legend_open.get() { BUTTON_ON } else { "" })
                            title="What the bars and marks mean"
                            aria-expanded=move || legend_open.get().to_string()
                            on:click=move |_| legend_open.update(|o| *o = !*o)>
                        "?"
                    </button>
                </span>
                {move || legend_open.get().then(|| view! { <Legend close=legend_open /> })}
            </div>
            {move || error.get().map(|e| view! {
                <p class="px-4 py-2 text-danger">{e.message}</p>
            })}
            <p class="sr-only" role="status" aria-live="polite">{move || ctx.said.get()}</p>
            {move || (ctx.view.get() == ViewMode::Chart).then(|| view! { <Strip ctx=ctx /> })}
            <div node_ref=scroller
                 class=move || format!("relative min-h-0 overflow-auto{}", if ctx.pan.get().is_some() { " gc-panning" } else { "" })
                 tabindex="0"
                 role=move || (ctx.view.get() == ViewMode::Chart).then_some("treegrid")
                 aria-label="The plan: objectives, projects and tasks. Arrow keys move and fold, Enter opens the dates."
                 aria-activedescendant=move || (ctx.view.get() == ViewMode::Chart).then(active_row).flatten()
                 on:focus=move |_| ctx.focused.set(true)
                 on:blur=move |_| ctx.focused.set(false)
                 on:keydown=move |e| on_key(ctx, e, scroller)
                 on:scroll=move |_| {
                     if let Some(el) = scroller.get_untracked() {
                         ctx.scroll_left.set(f64::from(el.scroll_left()));
                     }
                 }
                 on:wheel=move |e: ev::WheelEvent| {
                     // Ctrl+wheel zooms around the pointer, between Weeks and Days (spec 58).
                     if (e.ctrl_key() || e.meta_key()) && e.delta_y() != 0.0 && ctx.view.get_untracked() == ViewMode::Chart {
                         e.prevent_default();
                         let Some(el) = scroller.get_untracked() else { return };
                         let old = ctx.ppd_untracked();
                         let next = zoom(old, if e.delta_y() < 0.0 { 1 } else { -1 });
                         if next == ctx.scale.get_untracked() {
                             return;
                         }
                         let at = f64::from(e.client_x()) - el.get_bounding_client_rect().left()
                             - ctx.label_w.get_untracked();
                         let left = f64::from(el.scroll_left());
                         ctx.scale.set(next);
                         let new_ppd = ctx.ppd_untracked();
                         let to = zoom_scroll(at.max(0.0), left, old, new_ppd);
                         request_animation_frame(move || el.set_scroll_left(to as i32));
                         return;
                     }
                     // Shift+wheel scrolls sideways, on every system.
                     if e.shift_key() && e.delta_x() == 0.0 && e.delta_y() != 0.0 {
                         if let Some(el) = scroller.get_untracked() {
                             e.prevent_default();
                             el.set_scroll_left(el.scroll_left() + e.delta_y() as i32);
                         }
                     }
                 }>
                {move || ctx.shown.with(|d| d.is_none()).then(|| view! {
                    <p class="px-4 py-3 text-muted">"Loading…"</p>
                })}
                {move || ctx.shown.with(|d| d.as_ref().is_some_and(|d| d.groups.is_empty())).then(|| view! {
                    <p class="px-4 py-3 text-muted">
                        "Nothing to draw yet. Add a project with tasks that have estimates, and they appear here."
                    </p>
                })}
                {move || match ctx.view.get() {
                    ViewMode::Chart => view! { <Chart ctx=ctx scroller=scroller /> }.into_any(),
                    ViewMode::Table => view! { <Table ctx=ctx /> }.into_any(),
                }}
            </div>
        </div>
    }
}

/// The toolbar's segmented controls (spec 58): one box, buttons side by side.
const SEGMENTS: &str = "inline-flex overflow-hidden rounded-sm border border-line";
const SEGMENT: &str = "border-l border-line px-2 py-0.5 text-[12px] text-fg first:border-l-0 \
    hover:bg-accent/10 hover:text-accent";
const SEGMENT_ON: &str = "bg-accent/15 text-accent";
const GROUP_LABEL: &str = "text-[11px] text-muted";
const FILTER_SELECT: &str =
    "rounded-sm border border-line bg-canvas px-1.5 py-0.5 text-[12px] text-fg \
    focus:border-accent focus:outline-none";

/// The overview strip (spec 58): the whole axis in miniature, the work of every project, today
/// and the targets, and a box for what the chart shows. Click or drag it to move there.
#[component]
fn Strip(ctx: Ctx) -> impl IntoView {
    const H: f64 = 22.0;
    let drawn = move || {
        let strip_w = ctx.scroller_w.get();
        ctx.shown.with(|d| {
            let d = d.as_ref()?;
            let mut marks: Vec<AnyView> = Vec::new();
            for (lane, p) in d.groups.iter().flat_map(|g| g.projects.iter()).enumerate() {
                let y = 3.0 + (lane % 4) as f64 * 4.0;
                if let (Some(a), Some(b)) = (p.band.work_from, p.band.work_to) {
                    let (x1, x2) = (strip_x(d, a, strip_w), strip_x(d, b, strip_w));
                    marks.push(view! {
                        <rect class=if p.needs_a_look { "strip-work look" } else { "strip-work" }
                              x=x1 y=y width=(x2 - x1).max(1.0) height=3 />
                    }.into_any());
                }
                if let Some(t) = p.band.target_offset {
                    let x = strip_x(d, t, strip_w);
                    marks.push(
                        view! { <line class="strip-target" x1=x x2=x y1=y - 1.0 y2=y + 4.0 /> }
                            .into_any(),
                    );
                }
            }
            let today = strip_x(d, 0.0, strip_w);
            Some(view! {
                {marks}
                <line class="today" x1=today x2=today y1=0 y2=H />
            })
        })
    };
    let frame = move || {
        let strip_w = ctx.scroller_w.get();
        let ppd = ctx.ppd();
        let chart_w = ctx.shown.with(|d| {
            d.as_ref()
                .map_or(0.0, |d| crate::gantt::chart_width(d, ppd))
        });
        let (x, w) = strip_box(ctx.scroll_left.get(), ctx.view_w(), chart_w, strip_w);
        view! { <rect class="strip-box" x=x y=0.5 width=w height=H - 1.0 rx="2" /> }
    };
    view! {
        <div class="shrink-0 border-b border-line bg-panel" aria-hidden="true"
             title="The whole plan: click or drag to move the chart there">
            <svg class="gc gc-strip" width=move || ctx.scroller_w.get() height=H
                 on:mousedown=move |e: ev::MouseEvent| {
                     if e.button() != 0 { return; }
                     e.prevent_default();
                     ctx.strip_drag.set(true);
                     strip_follow(ctx, f64::from(e.client_x()));
                 }>
                {drawn}
                {frame}
            </svg>
        </div>
    }
}

/// What the colours and marks mean: a popover from the toolbar's **?** (spec 58).
#[component]
fn Legend(close: RwSignal<bool>) -> impl IntoView {
    let swatch = |class: &'static str, text: &'static str| {
        view! {
            <li class="flex items-center gap-2">
                <svg class="gc" width="24" height="12" aria-hidden="true">
                    <rect class=class x="1" y="1" width="22" height="10" rx="2" />
                </svg>
                {text}
            </li>
        }
    };
    view! {
        <div class="absolute right-4 top-full z-40 mt-1 w-[22rem] rounded-sm border border-line-strong bg-panel p-3 text-[12px] text-fg shadow-lg"
             role="dialog" aria-label="What the bars and marks mean">
            <div class="mb-2 flex items-start gap-2">
                <p class="flex-1 text-muted">
                    "On a bar, colour says where the work stands: a task's status, or a project's room. "
                    "An objective's own colour is the dot by its name."
                </p>
                <button class="text-faint hover:text-fg" aria-label="Close"
                        on:click=move |_| close.set(false)>"✕"</button>
            </div>
            <ul class="grid grid-cols-2 gap-x-4 gap-y-1.5">
                {swatch("gb todo", "To do")}
                {swatch("gb progress", "In progress")}
                {swatch("gb blocked", "Blocked")}
                {swatch("gb done", "Done")}
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="gb todo" x="1" y="1" width="22" height="8" rx="2" />
                        <rect class="crit" x="1" y="9" width="22" height="2" />
                    </svg>
                    "Critical"
                </li>
                {swatch("gb todo unestimated", "No estimate")}
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="gb todo" x="1" y="1" width="13" height="10" rx="2" />
                        <rect class="late-tail" x="14" y="1" width="9" height="10" />
                    </svg>
                    "Past what the target allows"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <line class="due" x1="12" x2="12" y1="1" y2="11" />
                    </svg>
                    "Due date"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="summary" x="1" y="1" width="16" height="10" rx="2" />
                        <rect class="progress" x="1" y="1" width="6" height="10" rx="2" />
                    </svg>
                    "Project: work, the done part filled"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="room wide" x="1" y="6" width="16" height="5" />
                        <path class="flag" d="M17 1h6l-2 2.5 2 2.5h-6z" />
                    </svg>
                    "Room before the target"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="room-key" x="1" y="6" width="22" height="5" />
                        <path class="hatch-line" d="M3 11l4-5M8 11l4-5M13 11l4-5M18 11l4-5" />
                    </svg>
                    "Narrow room (hatched)"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="room-key" x="1" y="6" width="22" height="5" />
                        <circle class="dot" cx="5" cy="8.5" r="1.2" />
                        <circle class="dot" cx="10" cy="8.5" r="1.2" />
                        <circle class="dot" cx="15" cy="8.5" r="1.2" />
                        <circle class="dot" cx="20" cy="8.5" r="1.2" />
                    </svg>
                    "Room closing or none (dotted)"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <rect class="over" x="1" y="1" width="22" height="10" />
                    </svg>
                    "Work past the target"
                </li>
                <li class="flex items-center gap-2">
                    <svg class="gc" width="24" height="12" aria-hidden="true">
                        <line class="today" x1="12" x2="12" y1="0" y2="12" />
                    </svg>
                    "Today"
                </li>
            </ul>
        </div>
    }
}

// ------------------------------------------------------------------------------- keyboard

/// The items in the order the screen shows them: the tree's rows, or the table's sorted tasks.
fn order(ctx: Ctx, d: &GanttData) -> Vec<(Uuid, Option<NodeRef>)> {
    let show_done = ctx.show_done.get_untracked();
    if let (ViewMode::Table, Some((key, desc))) =
        (ctx.view.get_untracked(), ctx.sort.get_untracked())
    {
        return sorted_tasks(d, show_done, key, desc)
            .into_iter()
            .map(|t| {
                (
                    t.schedule.id,
                    Some(NodeRef::new(NodeType::Task, t.schedule.id)),
                )
            })
            .collect();
    }
    // Rows that stand for no item (*No objective*, add rows, *Not assigned*) are passed over.
    let by_person = ctx.view.get_untracked() == ViewMode::Chart && ctx.by_person.get_untracked();
    tree(d, &ctx.toggled.get_untracked(), show_done, by_person)
        .iter()
        .filter_map(|r| node_of(r).map(|n| (r.key, Some(n))))
        .collect()
}

/// The item a row stands for (none for *No objective*).
fn node_of(r: &Row) -> Option<NodeRef> {
    match r.kind {
        RowKind::Group(g) => g
            .objective
            .as_ref()
            .map(|o| NodeRef::new(NodeType::Objective, o.id)),
        RowKind::Project(_) => Some(NodeRef::new(NodeType::Project, r.key)),
        RowKind::Task(_) => Some(NodeRef::new(NodeType::Task, r.key)),
        RowKind::Person(p) if p.id != UNASSIGNED.id => Some(NodeRef::new(NodeType::Person, p.id)),
        RowKind::Person(_) | RowKind::Add(_) => None,
    }
}

/// Moves a task a few working days from the keyboard or the date box, or says why it can't.
fn step_task(ctx: Ctx, id: Uuid, edge: DragEdge, delta: i64) {
    let refusal = ctx.shown.with_untracked(|d| {
        let d = d.as_ref()?;
        let t = find_task(d, id)?;
        task_step_refusal(d, t, &task_candidate(d, t, edge, delta))
    });
    if let Some(text) = refusal {
        ctx.refuse(text);
        return;
    }
    drop_it(
        ctx,
        DragState {
            what: What::Task { id, edge },
            x0: 0.0,
            delta,
            moved: true,
            seq: 0,
            preview: None,
        },
    );
}

/// Moves a project's or objective's date a few working days, or says why it can't.
fn step_band(ctx: Ctx, what: What, delta: i64) {
    let checked = ctx.shown.with_untracked(|d| {
        let d = d.as_ref()?;
        match what {
            What::Project { id, part } => {
                let p = find_project(d, id)?;
                Some(band_step(d, &p.band, part, delta, &p.title, false))
            }
            What::Objective { id } => {
                let g = find_group(d, id)?;
                let ongoing = g.objective.as_ref().is_some_and(|o| o.ongoing);
                Some(band_step(
                    d,
                    &g.band,
                    BandPart::Target,
                    delta,
                    &group_title(g),
                    ongoing,
                ))
            }
            What::Task { .. } => None,
        }
    });
    match checked {
        Some(Err(text)) => ctx.refuse(text),
        Some(Ok(_)) => drop_it(
            ctx,
            DragState {
                what,
                x0: 0.0,
                delta,
                moved: true,
                seq: 0,
                preview: None,
            },
        ),
        None => {}
    }
}

fn on_key(ctx: Ctx, e: ev::KeyboardEvent, scroller: leptos::prelude::NodeRef<leptos::html::Div>) {
    let key = e.key();
    if e.ctrl_key() || e.meta_key() {
        return;
    }
    if key == "t" && !e.alt_key() && !e.shift_key() {
        e.prevent_default();
        if let Some(el) = scroller.get_untracked() {
            scroll_to_today(ctx, &el);
        }
        return;
    }
    let current = ctx.selection.0.get_untracked();
    // Alt+arrows move the open item a working day: a task (Shift: its end), a project's target
    // (Shift: its start), an objective's target. Ctrl is left to the app.
    if e.alt_key() && (key == "ArrowLeft" || key == "ArrowRight") {
        e.prevent_default();
        let Some(node) = current else { return };
        if ctx.by_person.get_untracked() && ctx.view.get_untracked() == ViewMode::Chart {
            ctx.refuse("Rows by person only read: switch By person off to move work.".to_owned());
            return;
        }
        let step = if key == "ArrowLeft" { -1 } else { 1 };
        match node.node_type {
            NodeType::Task => {
                let edge = if e.shift_key() {
                    DragEdge::End
                } else {
                    DragEdge::Move
                };
                step_task(ctx, node.id, edge, step);
            }
            NodeType::Project => {
                let part = if e.shift_key() {
                    BandPart::Start
                } else {
                    BandPart::Target
                };
                step_band(ctx, What::Project { id: node.id, part }, step);
            }
            NodeType::Objective if e.shift_key() => {
                ctx.refuse(
                    "An objective has no start date to move; Alt+arrows move its target."
                        .to_owned(),
                );
            }
            NodeType::Objective => step_band(ctx, What::Objective { id: node.id }, step),
            _ => {}
        }
        return;
    }
    if e.alt_key() || e.shift_key() {
        return;
    }
    ctx.shown.with_untracked(|d| {
        let Some(d) = d else { return };
        let list = order(ctx, d);
        if list.is_empty() {
            return;
        }
        let at = current.and_then(|n| list.iter().position(|(k, _)| *k == n.id));
        let last = list.len() - 1;
        let page = scroller.get_untracked().map_or(10, |el| {
            ((f64::from(el.client_height()) - HEAD_H) / untrack(|| ctx.dims().row_h)).max(1.0)
                as usize
        });
        let target = match key.as_str() {
            "ArrowDown" | "j" => Some(at.map_or(0, |i| (i + 1).min(last))),
            "ArrowUp" | "k" => Some(at.map_or(0, |i| i.saturating_sub(1))),
            "Home" => Some(0),
            "End" => Some(last),
            "PageDown" => Some(at.map_or(0, |i| (i + page).min(last))),
            "PageUp" => Some(at.map_or(0, |i| i.saturating_sub(page))),
            _ => None,
        };
        if let Some(next) = target {
            e.prevent_default();
            if let Some(node) = list.get(next).and_then(|(_, n)| *n) {
                ctx.selection.open(node);
                if ctx.dates.get_untracked().is_some() {
                    ctx.dates.set(Some(node));
                }
            }
            if let (Some(el), Some((key, _))) = (scroller.get_untracked(), list.get(next)) {
                scroll_row_into_view(ctx, &el, *key);
            }
            return;
        }
        if key == "Enter"
            && ctx.by_person.get_untracked()
            && ctx.view.get_untracked() == ViewMode::Chart
        {
            return;
        }
        if key == "Enter" {
            if let Some(node) = current {
                e.prevent_default();
                ctx.dates.set(Some(node));
                // The keyboard goes into the box, to its first step, once it is drawn.
                set_timeout(
                    || {
                        if let Some(b) = document()
                            .query_selector(".gc-datebox button[aria-label$='earlier']")
                            .ok()
                            .flatten()
                            .and_then(|b| {
                                wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(b).ok()
                            })
                        {
                            let _ = b.focus();
                        }
                    },
                    std::time::Duration::from_millis(60),
                );
            }
            return;
        }
        if key == "*" {
            ctx.toggled.set(crate::gantt::all_open(d));
            ctx.say("Every project opened".to_owned());
            return;
        }
        // Folding is a tree's business; a sorted table has none.
        if ctx.view.get_untracked() == ViewMode::Table && ctx.sort.get_untracked().is_some() {
            return;
        }
        let toggled = ctx.toggled.get_untracked();
        let by_person =
            ctx.view.get_untracked() == ViewMode::Chart && ctx.by_person.get_untracked();
        let list = tree(d, &toggled, ctx.show_done.get_untracked(), by_person);
        let Some(row) = current.and_then(|n| list.iter().find(|r| r.key == n.id)) else {
            return;
        };
        let open = row_open(row, &toggled);
        let flip = match key.as_str() {
            "ArrowRight" => can_open(row) && !open,
            "ArrowLeft" => can_open(row) && open,
            _ => false,
        };
        if flip {
            e.prevent_default();
            toggle(ctx.toggled, row.key);
            ctx.say(fold_text(row, !open, ctx.show_done.get_untracked()));
        }
    });
}

// ------------------------------------------------------------------------------- dropping

fn drop_it(ctx: Ctx, cur: DragState) {
    let (toasts, version) = (ctx.toasts, ctx.version);
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
                        toasts.info_with_undo(dropped_text(&title, &c, done.pushed, most));
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
                    Ok(_) => toasts.info_with_undo(band_dropped_text(&title, start, target)),
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
                    Ok(_) => toasts.info_with_undo(band_dropped_text(&title, None, Some(target))),
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
fn Chart(ctx: Ctx, scroller: leptos::prelude::NodeRef<leptos::html::Div>) -> impl IntoView {
    let colours = use_objective_colours();
    let selection = ctx.selection;
    let width = move || {
        let ppd = ctx.ppd();
        ctx.shown
            .with(|d| d.as_ref().map_or(0.0, |d| chart_width(d, ppd)))
    };
    let body_ref = ctx.body;

    // The left column: one row per row of the chart.
    let labels = move || {
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            let toggled = ctx.toggled.get();
            let selected = selection.0.get().map(|n| n.id);
            let pushed = ctx.pushed.get();
            let side = counts_w(ctx.label_w.get());
            let list = tree(data, &toggled, ctx.show_done.get(), ctx.by_person.get());
            let found: HashSet<Uuid> = ctx.found.get().into_iter().collect();
            let look = RowLook {
                selected,
                focused: ctx.focused.get(),
                side,
                row_h: ctx.dims().row_h,
            };
            Some(
                list.iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let hit = found.contains(&r.key);
                        label_row(ctx, data, i, r, &toggled, &pushed, look, hit, colours)
                    })
                    .collect_view(),
            )
        })
    };

    let header = move || {
        let ppd = ctx.ppd();
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            let geo = Geo::new(data, ppd);
            let w = chart_width(data, ppd);
            let a = axis(&data.days, ppd);
            let mid = HEAD_H / 2.0;
            let months = a
                .months
                .iter()
                .map(|m| {
                    view! {
                        <line class="month-line" x1=m.x x2=m.x y1=0 y2=HEAD_H />
                        <text class="month" x=m.x + 4.0 y=13>{m.label.clone()}</text>
                    }
                })
                .collect_view();
            let numbers = !a.days.is_empty();
            let weeks = a
                .weeks
                .iter()
                .map(|t| {
                    view! {
                        <line class="week-line" x1=t.x x2=t.x y1=mid y2=HEAD_H />
                        {(!numbers && !t.label.is_empty()).then(|| view! {
                            <text x=t.x + 3.0 y=HEAD_H - 6.0>{t.label.clone()}</text>
                        })}
                    }
                })
                .collect_view();
            let today = geo.x(0.0);
            let days = a
                .days
                .iter()
                .map(|t| {
                    let is_today = (t.x - ppd / 2.0 - today).abs() < 0.5;
                    view! {
                        <text class=if is_today { "day today-num" } else { "day" }
                              x=t.x y=HEAD_H - 6.0 text-anchor="middle">{t.label.clone()}</text>
                    }
                })
                .collect_view();
            Some(view! {
                <svg class="gc" width=w height=HEAD_H aria-hidden="true">
                    <line class="head-line" x1=0 x2=w y1=HEAD_H y2=HEAD_H />
                    <line class="head-line" x1=0 x2=w y1=mid y2=mid />
                    <rect class="today-col" x=today y=mid width=ppd.max(2.0) height=mid>
                        <title>"Today"</title>
                    </rect>
                    {months}
                    {weeks}
                    {days}
                    <path class="today-marker" d=marker(today + ppd / 2.0, HEAD_H)>
                        <title>"Today"</title>
                    </path>
                </svg>
            })
        })
    };
    // The working day under the pointer: its column in the header, and the day in words.
    let day_hover = move || {
        let day = ctx.hover_day.get()?;
        let ppd = ctx.ppd();
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            let geo = Geo::new(data, ppd);
            let x = geo.x(day as f64);
            let text = minimap_types::short_day(crate::gantt::day_at(data, day));
            Some(view! {
                <div class="gc-day-col" aria-hidden="true" style=format!("left:{x}px;width:{}px", ppd.max(2.0))></div>
                <div class="gc-day-tag" aria-hidden="true" style=format!("left:{}px", x + ppd / 2.0)>{text}</div>
            })
        })
    };
    let row_hover = move || {
        let row_h = ctx.dims().row_h;
        ctx.hover_row.get().map(|i| {
            view! { <div class="gc-row-hover" style=format!("top:{}px;height:{row_h}px", i as f64 * row_h)></div> }
        })
    };

    let body = move || {
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            Some(body_svg(ctx, data))
        })
    };
    // Where the pointer is over the chart, in rows and working days.
    let track = move |e: ev::MouseEvent| {
        let Some(el) = body_ref.get_untracked() else {
            return;
        };
        let r = el.get_bounding_client_rect();
        let x = f64::from(e.client_x()) - r.left();
        let y = f64::from(e.client_y()) - r.top();
        let row_h = ctx.dims().row_h;
        let rows_n = ctx.shown.with_untracked(|d| {
            d.as_ref().map_or(0, |d| {
                tree(
                    d,
                    &ctx.toggled.get_untracked(),
                    ctx.show_done.get_untracked(),
                    ctx.by_person.get_untracked(),
                )
                .len()
            })
        });
        let row = (y >= 0.0)
            .then(|| (y / row_h) as usize)
            .filter(|i| *i < rows_n);
        let day = ctx.shown.with_untracked(|d| {
            d.as_ref()
                .map(|d| Geo::new(d, ctx.ppd_untracked()).day_under(x))
        });
        ctx.set_hover(row, day);
    };
    // A press on empty chart (bars stop it on their own) pans the view.
    let start_pan = move |e: ev::MouseEvent| {
        if e.button() != 0 {
            return;
        }
        let Some(el) = scroller.get_untracked() else {
            return;
        };
        e.prevent_default();
        let _ = el.focus();
        ctx.pan.set(Some(Pan {
            x0: f64::from(e.client_x()),
            y0: f64::from(e.client_y()),
            left0: el.scroll_left(),
            top0: el.scroll_top(),
        }));
    };
    // The edge of the label column: drag to resize, double-click to fit the longest name.
    let start_resize = move |e: ev::MouseEvent| {
        if e.button() != 0 {
            return;
        }
        e.prevent_default();
        e.stop_propagation();
        ctx.resizing
            .set(Some((f64::from(e.client_x()), ctx.label_w.get_untracked())));
    };
    let fit_labels = move |_| {
        let w = ctx.shown.with_untracked(|d| {
            d.as_ref().map(|d| {
                let list = tree(
                    d,
                    &ctx.toggled.get_untracked(),
                    ctx.show_done.get_untracked(),
                    ctx.by_person.get_untracked(),
                );
                fit_label_w(list.iter().map(|r| (r.depth, row_title(r).chars().count())))
            })
        });
        if let Some(w) = w {
            ctx.label_w.set(w);
            save_label_w(w);
        }
    };
    let edge = move || {
        view! {
            <div class="gc-resize" role="separator" aria-orientation="vertical"
                 aria-label="Width of the names. Drag to resize, double-click to fit."
                 title="Drag to resize, double-click to fit the longest name"
                 on:mousedown=start_resize on:dblclick=fit_labels></div>
        }
    };

    view! {
        <div style=move || format!("width:{}px;min-width:100%", ctx.label_w.get() + width())>
            <div class="sticky top-0 z-20 flex bg-panel" style=format!("height:{HEAD_H}px")>
                <div class="sticky left-0 z-30 flex shrink-0 items-end border-b border-r border-line bg-panel px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted"
                     aria-hidden="true"
                     style=move || format!("width:{}px", ctx.label_w.get())>
                    "Objective · project · task"
                    {edge}
                </div>
                <div class="relative border-b border-line" aria-hidden="true">{header}{day_hover}</div>
            </div>
            <div class="flex">
                <div class="sticky left-0 z-10 shrink-0 border-r border-line bg-canvas" role="rowgroup"
                     style=move || format!("width:{}px", ctx.label_w.get())
                     on:mouseleave=move |_| ctx.set_hover(None, None)>
                    {labels}
                    {row_hover}
                    {edge}
                </div>
                <div node_ref=body_ref class="relative"
                     on:mousemove=track
                     on:mouseleave=move |_| ctx.set_hover(None, None)
                     on:mousedown=start_pan>
                    {body}
                    {row_hover}
                    {move || date_box_here(ctx)}
                </div>
            </div>
        </div>
    }
}

fn chart_width(data: &GanttData, ppd: f64) -> f64 {
    crate::gantt::chart_width(data, ppd)
}

/// A small triangle pointing down at `x`, sitting on the line under the date axis.
fn marker(x: f64, y: f64) -> String {
    format!("M{} {}L{} {}L{x} {y}Z", x - 4.0, y - 7.0, x + 4.0, y - 7.0)
}

/// The name a row shows.
fn row_title(r: &Row) -> String {
    match r.kind {
        RowKind::Group(g) => group_title(g),
        RowKind::Project(p) => p.title.clone(),
        RowKind::Task(t) => t.schedule.title.clone(),
        RowKind::Add(_) => "+ Add a task".to_owned(),
        RowKind::Person(p) => p.title.clone(),
    }
}

/// A link in the counts column that opens quick-add, filled in.
fn add_link(ctx: Ctx, label: &'static str, tip: &'static str, text: String) -> AnyView {
    view! {
        <button class="text-[11px] text-accent hover:underline" title=tip
                on:click=move |e| {
                    e.stop_propagation();
                    let (prefill, palette) = ctx.palette;
                    prefill.open_with(palette, text.clone());
                }>{label}</button>
    }
    .into_any()
}

/// A row's treegrid attributes.
struct RowAria {
    id: String,
    level: u8,
    expanded: Option<String>,
    selected: String,
    label: String,
}

/// What every label row shares: which row is open, whether the chart has the keyboard, the
/// counts column's width and the row height.
#[derive(Clone, Copy)]
struct RowLook {
    selected: Option<Uuid>,
    focused: bool,
    side: f64,
    row_h: f64,
}

#[allow(clippy::too_many_arguments)]
fn label_row(
    ctx: Ctx,
    data: &GanttData,
    index: usize,
    row: &Row,
    toggled: &HashSet<Uuid>,
    pushed: &HashMap<Uuid, u32>,
    look: RowLook,
    is_found: bool,
    colours: crate::components::objective_colour::ObjectiveColours,
) -> AnyView {
    let RowLook { side, row_h, .. } = look;
    let key = row.key;
    let is_selected = look.selected == Some(key);
    let base = format!(
        "flex items-center gap-1 overflow-hidden whitespace-nowrap pr-2 border-b border-line/60 {}",
        match (is_selected, look.focused, is_found) {
            (true, true, _) => "bg-active gc-focus-row",
            (true, false, _) => "bg-active",
            (false, _, true) => "gc-found",
            _ => "",
        }
    );
    // A treegrid row (spec 58): its level, whether it is open, whether it is the open item, and
    // everything its bar says, in words.
    let aria = RowAria {
        id: row_dom_id(key),
        level: row.depth + 1,
        expanded: can_open(row).then(|| row_open(row, toggled).to_string()),
        selected: is_selected.to_string(),
        label: row_label(data, row),
    };
    let indent = format!(
        "padding-left:{}px;height:{row_h}px",
        6.0 + f64::from(row.depth) * 14.0
    );
    // A project's counts and chips sit right-aligned in a column of their own, so they line up
    // and the names get the rest; other rows take only the room their chips need.
    let column = move |items: Vec<AnyView>, fixed: bool| {
        (side > 0.0 && !items.is_empty()).then(|| {
            view! {
                <span class="flex shrink-0 items-center justify-end gap-1 overflow-hidden"
                      style=if fixed { format!("width:{side}px") } else { String::new() }>{items}</span>
            }
        })
    };
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
    let enter = move |_| ctx.set_hover(Some(index), ctx.hover_day.get_untracked());
    match &row.kind {
        RowKind::Group(g) => {
            let open = is_open(true, key, toggled);
            let id = g.objective.as_ref().map(|o| o.id);
            let hue = id.and_then(|i| colours.hue(i));
            let title = group_title(g);
            let tip = title.clone();
            let mut chips: Vec<AnyView> = Vec::new();
            if g.objective.as_ref().is_some_and(|o| o.ongoing) {
                chips.push(view! { <span class=Tone::Neutral.chip()>"ongoing"</span> }.into_any());
            }
            if let Some(n) = g.band.late_by_days {
                chips.push(
                    view! { <span class=Tone::Danger.chip()>{format!("{n}d over")}</span> }
                        .into_any(),
                );
            }
            if empty_text(row).is_some() {
                chips.push(add_link(
                    ctx,
                    "Add a project",
                    "No projects yet. Opens quick-add with this objective filled in.",
                    add_project_text(&title),
                ));
            }
            let can = !g.projects.is_empty();
            view! {
                <div class=base style=indent on:mouseenter=enter
                     id=aria.id.clone() role="row" {..custom_attribute("aria-level", aria.level.to_string())} aria-expanded=aria.expanded.clone()
                     aria-selected=aria.selected.clone() aria-label=aria.label.clone()>
                    <div role="gridcell" class="contents">
                    {chevron(open, can)}
                    <span class="obj-dot" style=hue.map(|h| format!("--obj-h: {h}")).unwrap_or_default()></span>
                    <button class="min-w-0 flex-1 truncate text-left font-semibold"
                            title=tip
                            on:click=move |_| {
                                if let Some(i) = id {
                                    ctx.selection.open(NodeRef::new(NodeType::Objective, i));
                                }
                            }>{title}</button>
                    {column(chips, false)}
                    </div>
                </div>
            }
            .into_any()
        }
        RowKind::Project(p) => {
            let open = is_open(p.needs_a_look, key, toggled);
            let can = !p.tasks.is_empty();
            let room = room_chip(&p.zone, p.band.late_by_days);
            let also = p
                .also_under
                .first()
                .map(|o| (o.title.clone(), p.also_under.len()));
            let mut chips: Vec<AnyView> = Vec::new();
            if empty_text(row).is_some() {
                chips.push(add_link(
                    ctx,
                    "Add a task",
                    "No open tasks. Opens quick-add with this project filled in.",
                    add_task_text(&p.slug),
                ));
            } else {
                let counts = count_parts(p)
                    .into_iter()
                    .map(|c| {
                        let class = match c {
                            Count::Open(_) => "text-muted",
                            Count::Blocked(_) => "text-warning",
                            Count::Done(_) => "text-faint",
                        };
                        view! { <span class=class title=c.words()>{c.short()}</span> }
                    })
                    .collect_view();
                chips.push(
                    view! {
                        <span class="flex shrink-0 items-center gap-1 text-[10px] tabular-nums"
                              aria-label=project_counts(p)>{counts}</span>
                    }
                    .into_any(),
                );
            }
            if let Some((text, bad)) = room {
                chips.push(
                    view! {
                        <span class=if bad { Tone::Danger.chip() } else { Tone::Neutral.chip() }>{text}</span>
                    }
                    .into_any(),
                );
            }
            if let Some((first, n)) = also {
                chips.push(
                    view! {
                        <span class=Tone::Neutral.chip()
                              title=format!("Also serves {first}{}", if n > 1 { format!(" and {} more", n - 1) } else { String::new() })>
                            "+" {n}
                        </span>
                    }
                    .into_any(),
                );
            }
            let tip = match room_chip(&p.zone, p.band.late_by_days) {
                Some((r, _)) => format!("{}\n{} · {r}", p.title, project_counts(p)),
                None => format!("{}\n{}", p.title, project_counts(p)),
            };
            view! {
                <div class=base style=indent on:mouseenter=enter
                     id=aria.id.clone() role="row" {..custom_attribute("aria-level", aria.level.to_string())} aria-expanded=aria.expanded.clone()
                     aria-selected=aria.selected.clone() aria-label=aria.label.clone()>
                    <div role="gridcell" class="contents">
                    {chevron(open, can)}
                    <button class="min-w-0 flex-1 truncate text-left font-medium"
                            title=tip
                            on:click=move |_| ctx.selection.open(NodeRef::new(NodeType::Project, key))>
                        {p.title.clone()}
                    </button>
                    {column(chips, true)}
                    </div>
                </div>
            }
            .into_any()
        }
        RowKind::Add(p) => {
            let text = add_task_text(&p.slug);
            view! {
                <div class=base style=indent on:mouseenter=enter
                     id=aria.id.clone() role="row" {..custom_attribute("aria-level", aria.level.to_string())}
                     aria-selected=aria.selected.clone() aria-label=aria.label.clone()>
                    <div role="gridcell" class="contents">
                    <span class="w-4 shrink-0"></span>
                    <button class="min-w-0 flex-1 truncate text-left text-[11px] text-muted hover:text-accent"
                            title="Opens quick-add with this project filled in. Or drag across the chart on this row to give it dates."
                            on:click=move |_| {
                                let (prefill, palette) = ctx.palette;
                                prefill.open_with(palette, text.clone());
                            }>"+ Add a task"</button>
                    </div>
                </div>
            }
            .into_any()
        }
        RowKind::Person(p) => {
            let open = is_open(true, key, toggled);
            let id = p.id;
            let over = ctx.overload.with(|o| o.get(&id).map_or(0, Vec::len));
            let mut chips: Vec<AnyView> = Vec::new();
            if over > 0 {
                chips.push(
                    view! {
                        <span class=Tone::Danger.chip() title="Weeks on the chart where their work is more than their time">
                            {format!("over {over} wk")}
                        </span>
                    }
                    .into_any(),
                );
            }
            view! {
                <div class=base style=indent on:mouseenter=enter
                     id=aria.id.clone() role="row" {..custom_attribute("aria-level", aria.level.to_string())}
                     aria-expanded=aria.expanded.clone()
                     aria-selected=aria.selected.clone() aria-label=aria.label.clone()>
                    <div role="gridcell" class="contents">
                    {chevron(open, true)}
                    <button class="min-w-0 flex-1 truncate text-left font-semibold" title=p.title.clone()
                            on:click=move |_| {
                                if id != UNASSIGNED.id {
                                    ctx.selection.open(NodeRef::new(NodeType::Person, id));
                                }
                            }>{p.title.clone()}</button>
                    {column(chips, false)}
                    </div>
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
            let mut chips: Vec<AnyView> = Vec::new();
            if let Some(d) = flash {
                chips.push(view! {
                    <span class=Tone::Danger.chip() title="Its due date was pushed later by a drop">{format!("+{d}d")}</span>
                }.into_any());
            }
            if let Some(n) = s.late_by_days {
                chips.push(
                    view! { <span class=Tone::Danger.chip()>{format!("late {n}d")}</span> }
                        .into_any(),
                );
            }
            if t.overdue {
                chips.push(view! { <span class=Tone::Danger.chip()>"overdue"</span> }.into_any());
            }
            if s.status == minimap_types::TaskStatus::Blocked {
                chips.push(view! { <span class=Tone::Warning.chip()>"blocked"</span> }.into_any());
            }
            view! {
                <div class=base style=indent
                     id=aria.id.clone() role="row" {..custom_attribute("aria-level", aria.level.to_string())}
                     aria-selected=aria.selected.clone() aria-label=aria.label.clone()
                     on:mouseenter=move |e| { enter(e); ctx.hover.set(Some(key)); }
                     on:mouseleave=move |_| ctx.hover.set(None)>
                    <div role="gridcell" class="contents">
                    <span class="w-4 shrink-0"></span>
                    <button class=class title=task_tip(t)
                            on:click=move |_| ctx.selection.open(NodeRef::new(NodeType::Task, key))>
                        {s.title.clone()}
                    </button>
                    {column(chips, false)}
                    </div>
                </div>
            }
            .into_any()
        }
    }
}

/// The chart body: gridlines, then the rows' bars, then the arrows on top.
fn body_svg(ctx: Ctx, data: &GanttData) -> impl IntoView {
    let dims = ctx.dims();
    let row_h = dims.row_h;
    let focused = ctx.focused.get();
    let ppd = ctx.ppd();
    let geo = Geo::new(data, ppd);
    let toggled = ctx.toggled.get();
    let by_person = ctx.by_person.get();
    let list = tree(data, &toggled, ctx.show_done.get(), by_person);
    let found: HashSet<Uuid> = ctx.found.get().into_iter().collect();
    let overload = ctx.overload.get();
    let adding = ctx.adding.get();
    let w = chart_width(data, ppd);
    let names = shows_names(ppd);
    let today = geo.x(0.0);
    let h = (list.len().max(1)) as f64 * row_h;
    let drag = ctx.drag.get();
    let pushed = ctx.pushed.get();
    let selected = ctx.selection.0.get();
    let resizable = can_resize(ppd);

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

    // ---- grid: a line for each day when there is room, each week, and each month
    let a = axis(&data.days, ppd);
    let ticks = a
        .days
        .iter()
        .map(|d| (d.x - ppd / 2.0, "day-line"))
        .chain(a.weeks.iter().map(|wk| (wk.x, "week-line")))
        .chain(a.months.iter().map(|m| (m.x, "month-line")))
        .map(|(x, class)| view! { <line class=class x1=x x2=x y1=0 y2=h /> })
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
        let y = i as f64 * row_h;
        let selected_row = selected.is_some_and(|n| n.id == r.key);
        let row_class = match (selected_row, focused, found.contains(&r.key)) {
            (true, true, _) => "row-sel row-focus",
            (true, false, _) => "row-sel",
            (false, _, true) => "row-found",
            _ => "row-bg",
        };
        shapes.push(
            view! {
                <rect class=row_class x=0 y=y width=w height=row_h />
                <line class="row-line" x1=0 x2=w y1=y + row_h y2=y + row_h />
            }
            .into_any(),
        );
        if let Some(text) = empty_text(r) {
            shapes.push(
                view! { <text class="empty" x=today + 8.0 y=y + row_h / 2.0 + 3.5>{text}</text> }
                    .into_any(),
            );
        }
        match &r.kind {
            RowKind::Group(g) => {
                let id = g.objective.as_ref().map(|o| o.id);
                let mut band = g.band;
                if let (Some(d), Some(oid)) = (drag.as_ref().filter(|d| d.moved), id) {
                    if d.what == (What::Objective { id: oid }) {
                        band = shift_band(&band, BandPart::Target, d.delta);
                    }
                }
                let ongoing = g.objective.as_ref().is_some_and(|o| o.ongoing);
                let can_drag = id.is_some() && !ongoing && band.target_offset.is_some();
                let title = group_title(g);
                shapes.push(band_shapes(
                    ctx,
                    BandLook {
                        y,
                        dims,
                        focus: selected_row && focused,
                        m: marks(&band, geo),
                        room: None,
                        ongoing,
                        chart_w: w,
                        ppd,
                        today,
                        progress: group_progress(g),
                        name: names.then_some(title.clone()),
                    },
                    band_tip(&title, &band, None),
                    id.map(|i| (What::Objective { id: i }, can_drag, false)),
                ));
            }
            RowKind::Project(p) => {
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
                    BandLook {
                        y,
                        dims,
                        focus: selected_row && focused,
                        m: marks(&band, geo),
                        room: Some(room_class(&p.zone)),
                        ongoing: false,
                        chart_w: w,
                        ppd,
                        today,
                        progress: (p.done_tasks, p.open_tasks),
                        name: (names && empty_text(r).is_none()).then(|| p.title.clone()),
                    },
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
            }
            RowKind::Add(p) => {
                let (pid, slug) = (p.id, p.slug.clone());
                shapes.push(
                    view! {
                        <rect class="add-row" x=0 y=y width=w height=row_h
                              on:mousedown=move |e: ev::MouseEvent| start_adding(ctx, pid, slug.clone(), e)>
                            <title>"Drag across this row to add a task with those dates"</title>
                        </rect>
                    }
                    .into_any(),
                );
                if let Some(a) = adding.as_ref().filter(|a| a.project == pid) {
                    let (from, to) = (a.from.min(a.to), a.from.max(a.to));
                    let (x1, x2) = (geo.x(from as f64), geo.x((to + 1) as f64));
                    let by = y + (row_h - dims.bar_h) / 2.0;
                    let days = (to - from + 1) as u32;
                    let text = format!(
                        "{} → {} · {}",
                        minimap_types::short_day(crate::gantt::day_at(data, from)),
                        minimap_types::short_day(crate::gantt::day_at(data, to)),
                        if days == 1 {
                            "1 day".to_owned()
                        } else {
                            format!("{days} days")
                        }
                    );
                    shapes.push(
                        view! {
                            <rect class="new-bar" x=x1 y=by width=(x2 - x1).max(MIN_BAR_W) height=dims.bar_h rx="2" />
                            <text class="name" x=x2 + 6.0 y=by + dims.bar_h - 2.0>{text}</text>
                        }
                        .into_any(),
                    );
                } else if names {
                    shapes.push(
                        view! { <text class="empty" x=today + 8.0 y=y + row_h / 2.0 + 3.5>"Drag here to add a task"</text> }
                            .into_any(),
                    );
                }
            }
            RowKind::Person(p) => {
                for (from, to, pct) in overload.get(&p.id).into_iter().flatten() {
                    let (x1, x2) = (geo.x(*from as f64), geo.x(*to as f64));
                    shapes.push(
                        view! {
                            <rect class="overload" x=x1 y=y + 2.0 width=(x2 - x1).max(1.0) height=row_h - 4.0>
                                <title>{format!("Over capacity that week: {pct:.0}% of their time")}</title>
                            </rect>
                        }
                        .into_any(),
                    );
                }
            }
            RowKind::Task(t) => {
                let dim = focus.as_ref().is_some_and(|f| !f.contains(&t.schedule.id));
                shapes.push(task_shapes(
                    ctx,
                    geo,
                    t,
                    TaskLook {
                        y,
                        dims,
                        focus: selected_row && focused,
                        read_only: by_person,
                    },
                    names.then_some(w),
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
            return Some((x, i as f64 * row_h + row_h / 2.0, false));
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
            i as f64 * row_h + row_h / 2.0,
            true,
        ))
    };
    let mut arrows: Vec<(bool, AnyView)> = Vec::new();
    let mut seen: HashSet<(i64, i64, i64, i64, u8)> = HashSet::new();
    // By person a task may sit far from what it waits for; the arrows are left out there.
    for l in data.links.iter().filter(|_| !by_person) {
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
                        i as f64 * row_h + row_h / 2.0,
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
                    <path class="line" d=elbow(x1, y1, x2, y2, row_h) />
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
        let y = (i as f64 * row_h - 20.0).max(2.0);
        let x = x.min((w - width).max(0.0));
        Some(view! {
            <g class=if stopped { "tag stopped" } else { "tag" } pointer-events="none">
                <rect x=x y=y width=width height=18 rx="2" />
                <text x=x + 6.0 y=y + 12.5>{text}</text>
            </g>
        })
    });

    let dragging = drag.as_ref().is_some_and(|d| d.moved);
    view! {
        <svg class=if dragging { "gc dragging" } else { "gc" } width=w height=h aria-hidden="true">
            <defs>
                // Room that is narrow is hatched and room that is closing is dotted, so the
                // level never rests on colour alone (spec 58).
                <pattern id="gc-hatch" width="4" height="4" patternUnits="userSpaceOnUse"
                         patternTransform="rotate(45)">
                    <line class="hatch-line" x1="0" y1="0" x2="0" y2="4" />
                </pattern>
                <pattern id="gc-over" width="6" height="6" patternUnits="userSpaceOnUse"
                         patternTransform="rotate(45)">
                    <line class="over-line" x1="0" y1="0" x2="0" y2="6" />
                </pattern>
                <pattern id="gc-dots" width="4" height="4" patternUnits="userSpaceOnUse">
                    <circle class="dot" cx="2" cy="2" r="1.2" />
                </pattern>
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

/// How a project's or objective's row is drawn.
struct BandLook {
    y: f64,
    dims: Dims,
    /// The open row while the chart has the keyboard: the bar is outlined.
    focus: bool,
    m: Marks,
    /// The class that shades the room (a project's zone); objectives use the plain one.
    room: Option<&'static str>,
    ongoing: bool,
    chart_w: f64,
    ppd: f64,
    today: f64,
    /// Tasks done and open, for the filled part of the bar.
    progress: (u32, u32),
    /// The name to write after the bar, when the scale has room for names.
    name: Option<String>,
}

/// A project's or objective's row (spec 58): the work as a bar as tall as a task's, filled for
/// what is done; the room to the target as a thin strip along its foot; the overrun red and as
/// tall as the bar; the flag; and handles.
fn band_shapes(ctx: Ctx, b: BandLook, tip: String, drag: Option<(What, bool, bool)>) -> AnyView {
    let BandLook { y, m, dims, .. } = b;
    let Dims { row_h, bar_h } = dims;
    let by = y + (row_h - bar_h) / 2.0;
    // Tall enough for its pattern to read (hatched narrow, dotted closing).
    let strip = (bar_h / 2.4).round();
    let room_rect = m.room.map(|(x, wd)| {
        view! { <rect class=b.room.unwrap_or("room wide") x=x y=by + bar_h - strip width=wd height=strip /> }
    });
    let over_rect = m.over.map(|(x, wd)| {
        view! { <rect class="over" x=x y=by width=wd height=bar_h /> }
    });
    let fade = b.ongoing.then(|| {
        let from = m.work.map_or(b.today, |(x, wd)| x + wd);
        let (fx, fw) = fade_span(from, b.ppd, b.chart_w);
        view! { <rect class="fade" x=fx y=by + bar_h / 2.0 - 1.5 width=fw height=3 /> }
    });
    // Dragging the whole bar needs both ends to be dates.
    let (what, can_target, can_move) = match drag {
        Some((w, a, c)) => (Some(w), a, c),
        None => (None, false, false),
    };
    let (done, open) = b.progress;
    let hit_y = y + (row_h - HIT) / 2.0;
    let focus = b.focus;
    let work = m.work.map(|(x, wd)| {
        let filled = progress_w(wd, done, open);
        let body = view! {
            <rect class=if focus { "summary focus" } else { "summary" } x=x y=by width=wd height=bar_h rx="2" />
            {(filled > 0.5).then(|| view! {
                <rect class="progress" x=x y=by width=filled height=bar_h rx="2" />
            })}
        };
        match (what, can_move) {
            (Some(What::Project { id, .. }), true) => {
                let (gx, gw) = body_hit(x, wd);
                view! {
                    {body}
                    <rect class="grip" x=gx y=hit_y width=gw height=HIT
                          on:mousedown=press(ctx, What::Project { id, part: BandPart::Both }) />
                }
                .into_any()
            }
            _ => body.into_any(),
        }
    });
    // The press areas of the start mark and the flag: wide enough to hit, never on each other.
    let (start_hit, target_hit) = band_handles(m.start_x, m.target_x);
    let flag = m.target_x.map(|x| {
        let handle = what
            .filter(|_| can_target)
            .zip(target_hit)
            .map(|(w, (hx, hw))| {
                view! {
                    <rect class="handle" x=hx y=hit_y width=hw height=HIT
                          on:mousedown=press(ctx, w) />
                }
            });
        let fy = y + (row_h - 12.0) / 2.0 - 1.0;
        view! {
            <line class="target" x1=x x2=x y1=y + 2.0 y2=y + row_h - 2.0 />
            <path class="flag" d=format!("M{x} {fy}h7l-2 3 2 3h-7z") />
            {handle}
        }
    });
    let start = m.start_x.map(|x| {
        let handle = match (what, start_hit) {
            (Some(What::Project { id, .. }), Some((hx, hw))) => Some(view! {
                <rect class="handle" x=hx y=hit_y width=hw height=HIT
                      on:mousedown=press(ctx, What::Project { id, part: BandPart::Start }) />
            }),
            _ => None,
        };
        view! {
            <path class="pin" d=format!("M{x} {}v{}", y + 3.0, row_h - 6.0) />
            {handle}
        }
    });
    // The name goes after everything the row draws: the work, the room, the overrun, the flag.
    let end = [
        m.work.map(|(x, wd)| x + wd),
        m.room.map(|(x, wd)| x + wd),
        m.over.map(|(x, wd)| x + wd),
        m.target_x.map(|x| x + 8.0),
    ]
    .into_iter()
    .flatten()
    .fold(f64::NEG_INFINITY, f64::max);
    let name = b
        .name
        .filter(|_| end.is_finite())
        .and_then(|n| bar_name(&n, end, false, b.chart_w))
        .map(|(x, text)| {
            view! { <text class="name band-name" x=x y=by + bar_h - 2.0>{text}</text> }
        });
    view! {
        <g>
            {room_rect}{fade}{work}{over_rect}{start}{flag}{name}
            <title>{tip}</title>
        </g>
    }
    .into_any()
}

/// One task's row: its bar, the slack after it, the due date, and the handles to drag it.
/// Where a task's row is and how it is drawn.
struct TaskLook {
    y: f64,
    dims: Dims,
    /// The open row while the chart has the keyboard: the bar is outlined.
    focus: bool,
    /// Rows by person only read: a press opens the task, nothing drags.
    read_only: bool,
}

#[allow(clippy::too_many_arguments)]
fn task_shapes(
    ctx: Ctx,
    geo: Geo,
    t: &GanttTask,
    look: TaskLook,
    // The chart's width when names are written next to the bars.
    names: Option<f64>,
    moved: Option<(f64, f64)>,
    ghost: Option<(Uuid, f64, f64)>,
    pushed: Option<u32>,
    limit: Option<f64>,
    dim: bool,
    resizable: bool,
) -> AnyView {
    let TaskLook {
        y,
        dims,
        focus,
        read_only,
    } = look;
    let Dims { row_h, bar_h } = dims;
    let s = &t.schedule;
    let id = s.id;
    let (es, ef) = moved.unwrap_or((s.es, s.ef));
    let x = geo.x(es);
    let wd = (geo.x(ef) - x).max(MIN_BAR_W);
    let by = y + (row_h - bar_h) / 2.0;
    let class = format!(
        "{}{}{}{}",
        bar_class(t),
        if moved.is_some() { " moving" } else { "" },
        if pushed.is_some() { " pushed" } else { "" },
        if focus { " focus" } else { "" }
    );
    let movable = !s.done && !read_only;
    let late = (s.late_by_days.is_some() && s.ef > s.lf + 1e-9 && moved.is_none()).then(|| {
        let from = geo.x(s.lf).max(x);
        view! { <rect class="late-tail" x=from y=by width=(x + wd - from).max(0.0) height=bar_h pointer-events="none" /> }
    });
    let slack = (!s.done && !s.critical && s.slack_days > 1e-9).then(|| {
        view! { <line class="slack" x1=x + wd x2=geo.x(ef + s.slack_days) y1=by + bar_h / 2.0 y2=by + bar_h / 2.0 /> }
    });
    let critical = (s.critical && !s.done)
        .then(|| view! { <rect class="crit" x=x y=by + bar_h + 1.0 width=wd height=2 pointer-events="none" /> });
    let due = t.due_offset.map(|o| {
        let dx = geo.x(o);
        view! {
            <line class=if t.overdue { "due overdue" } else { "due" } x1=dx x2=dx y1=y + 3.0 y2=y + row_h - 3.0>
            </line>
        }
    });
    let ghost = ghost.map(|(_, ges, gef)| {
        let gx = geo.x(ges);
        view! { <rect class="ghost" x=gx y=by width=(geo.x(gef) - gx).max(MIN_BAR_W) height=bar_h rx="2" /> }
    });
    let limit = limit.map(|lx| view! { <line class="limit" x1=lx x2=lx y1=y y2=y + row_h /> });
    let chip = pushed.map(|d| {
        view! { <text class="pushed" x=x + wd + 6.0 y=by + bar_h - 2.0>{format!("+{d}d")}</text> }
    });
    let name = names
        .and_then(|chart_w| bar_name(&s.title, x + wd, pushed.is_some(), chart_w))
        .map(|(nx, text)| {
            view! {
                <text class=if s.critical && !s.done { "name critical" } else { "name" }
                      x=nx y=by + bar_h - 2.0>{text}</text>
            }
        });
    // What is drawn stays its size; what can be pressed is at least HIT square (WCAG 2.5.8).
    let hit_y = y + (row_h - HIT) / 2.0;
    let body = view! { <rect class=class x=x y=by width=wd height=bar_h rx="2" /> };
    let hits = if movable && resizable {
        let h = task_hits(x, wd);
        view! {
            <rect class="hit" x=h.body.0 y=hit_y width=h.body.1 height=HIT
                  on:mousedown=press(ctx, What::Task { id, edge: DragEdge::Move }) />
            <rect class="handle" x=h.start.0 y=hit_y width=h.start.1 height=HIT
                  on:mousedown=press(ctx, What::Task { id, edge: DragEdge::Start }) />
            <rect class="handle" x=h.end.0 y=hit_y width=h.end.1 height=HIT
                  on:mousedown=press(ctx, What::Task { id, edge: DragEdge::End }) />
        }
        .into_any()
    } else {
        let (hx, hw) = body_hit(x, wd);
        view! {
            <rect class=if movable { "hit" } else { "hit still" } x=hx y=hit_y width=hw height=HIT
                  on:mousedown=press(ctx, What::Task { id, edge: DragEdge::Move }) />
        }
        .into_any()
    };
    view! {
        <g class=if dim { "task dim" } else { "task" }
           on:mouseenter=move |_| ctx.hover.set(Some(id))
           on:mouseleave=move |_| ctx.hover.set(None)>
            {limit}{ghost}{slack}{body}{late}{critical}{due}{chip}{name}{hits}
            <title>{task_tip(t)}</title>
        </g>
    }
    .into_any()
}

// ------------------------------------------------------------------------------ the date box

/// A date as the date field writes it.
fn ymd(d: Date) -> String {
    format_ymd(d.year(), d.month() as u32, u32::from(d.day()))
}

/// What a date field committed, as a date.
fn date_of(text: &str) -> Option<Date> {
    let (y, m, d) = parse_ymd(text)?;
    Date::from_calendar_date(y, time::Month::try_from(m as u8).ok()?, d as u8).ok()
}

/// Working days from `from` to the working day on or after `to`.
fn days_between(data: &GanttData, from: Date, to: Date) -> Option<i64> {
    Some(working_day_on_or_after(data, to)? - working_day_on_or_after(data, from)?)
}

/// One line of the date box: a name, a working day earlier or later, and a date to type or pick.
fn step_row(
    label: &'static str,
    what: String,
    step: impl Fn(i64) + Copy + Send + Sync + 'static,
    date: Option<(Date, Callback<String>)>,
) -> AnyView {
    let field_id = format!("gc-date-{}", label.to_lowercase());
    view! {
        <div class="flex items-center gap-1.5">
            {if date.is_some() {
                view! { <label class="w-14 shrink-0 text-muted" for=field_id.clone()>{label}</label> }.into_any()
            } else {
                view! { <span class="w-14 shrink-0 text-muted">{label}</span> }.into_any()
            }}
            <button class=BUTTON aria-label=format!("{what} one working day earlier")
                    title="One working day earlier" on:click=move |_| step(-1)>"−1d"</button>
            <button class=BUTTON aria-label=format!("{what} one working day later")
                    title="One working day later" on:click=move |_| step(1)>"+1d"</button>
            {date.map(|(d, on_commit)| view! {
                <DateField current=ymd(d) on_commit=on_commit compact=true id=field_id.clone() />
            })}
        </div>
    }
    .into_any()
}

/// The date box of an item (spec 58): the way to change its dates without dragging, with the
/// same rules as the drag and one undo step a change.
fn date_box(ctx: Ctx, node: NodeRef) -> Option<AnyView> {
    ctx.shown.with(|d| {
        let d = d.as_ref()?;
        let (title, summary, lines): (String, String, Vec<AnyView>) = match node.node_type {
            NodeType::Task => {
                let t = find_task(d, node.id)?;
                let s = &t.schedule;
                let id = s.id;
                let mut summary = format!(
                    "{} → {}",
                    minimap_types::short_day(s.start),
                    minimap_types::short_day(s.finish)
                );
                if let Some(e) = t.estimate_days {
                    summary.push_str(&format!(" · estimate {e}d"));
                }
                if let Some(due) = t.due_date {
                    summary.push_str(&format!(" · due {}", minimap_types::short_day(due)));
                }
                let (start, finish) = (s.start, s.finish);
                let at = move |edge: DragEdge, from: Date| {
                    Callback::new(move |text: String| {
                        let delta = date_of(&text).and_then(|to| {
                            ctx.shown
                                .with_untracked(|d| d.as_ref().and_then(|d| days_between(d, from, to)))
                        });
                        match delta {
                            Some(delta) => step_task(ctx, id, edge, delta),
                            None => ctx.refuse("That date isn't on the plan's calendar.".to_owned()),
                        }
                    })
                };
                let lines = if s.done {
                    vec![view! { <p class="text-muted">"Finished: reopen it to move it."</p> }.into_any()]
                } else {
                    vec![
                        step_row("Move", "Move the task".to_owned(), move |n| step_task(ctx, id, DragEdge::Move, n), None),
                        step_row("Start", "Start".to_owned(), move |n| step_task(ctx, id, DragEdge::Start, n), Some((start, at(DragEdge::Start, start)))),
                        step_row("End", "End".to_owned(), move |n| step_task(ctx, id, DragEdge::End, n), Some((finish, at(DragEdge::End, finish)))),
                    ]
                };
                (s.title.clone(), summary, lines)
            }
            NodeType::Project => {
                let p = find_project(d, node.id)?;
                let id = p.id;
                let b = p.band;
                let summary = band_tip("", &b, Some(&p.zone)).trim().replace('\n', " · ");
                let at = move |part: BandPart, from: Date| {
                    Callback::new(move |text: String| {
                        let delta = date_of(&text).and_then(|to| {
                            ctx.shown
                                .with_untracked(|d| d.as_ref().and_then(|d| days_between(d, from, to)))
                        });
                        match delta {
                            Some(delta) => step_band(ctx, What::Project { id, part }, delta),
                            None => ctx.refuse("That date isn't on the plan's calendar.".to_owned()),
                        }
                    })
                };
                let mut lines = Vec::new();
                if let Some(start) = b.start_date {
                    lines.push(step_row("Start", "Start".to_owned(), move |n| step_band(ctx, What::Project { id, part: BandPart::Start }, n), Some((start, at(BandPart::Start, start)))));
                }
                if let Some(target) = b.target_date {
                    lines.push(step_row("Target", "Target".to_owned(), move |n| step_band(ctx, What::Project { id, part: BandPart::Target }, n), Some((target, at(BandPart::Target, target)))));
                }
                if b.start_date.is_some() && b.target_date.is_some() {
                    lines.push(step_row("Both", "Start and target".to_owned(), move |n| step_band(ctx, What::Project { id, part: BandPart::Both }, n), None));
                }
                if lines.is_empty() {
                    lines.push(view! { <p class="text-muted">"No start or target date yet. Set them in the detail pane."</p> }.into_any());
                }
                (p.title.clone(), summary, lines)
            }
            NodeType::Objective => {
                let g = find_group(d, node.id)?;
                let id = node.id;
                let ongoing = g.objective.as_ref().is_some_and(|o| o.ongoing);
                let summary = band_tip("", &g.band, None).trim().replace('\n', " · ");
                let lines = match g.band.target_date {
                    _ if ongoing => vec![view! { <p class="text-muted">"Ongoing: it has no target to move."</p> }.into_any()],
                    Some(target) => {
                        let on_commit = Callback::new(move |text: String| {
                            let delta = date_of(&text).and_then(|to| {
                                ctx.shown.with_untracked(|d| {
                                    d.as_ref().and_then(|d| days_between(d, target, to))
                                })
                            });
                            match delta {
                                Some(delta) => step_band(ctx, What::Objective { id }, delta),
                                None => ctx.refuse("That date isn't on the plan's calendar.".to_owned()),
                            }
                        });
                        vec![step_row("Target", "Target".to_owned(), move |n| step_band(ctx, What::Objective { id }, n), Some((target, on_commit)))]
                    }
                    None => vec![view! { <p class="text-muted">"No target date yet. Set one in the detail pane."</p> }.into_any()],
                };
                (group_title(g), summary, lines)
            }
            _ => return None,
        };
        let close = move |_| {
            ctx.dates.set(None);
            if let Some(el) = ctx.scroller.get_untracked() {
                let _ = el.focus();
            }
        };
        Some(
            view! {
                <div class="gc-datebox w-[19rem] rounded-sm border border-line-strong bg-panel p-2.5 text-[12px] text-fg shadow-lg"
                     role="dialog" aria-label=format!("Dates of {title}")
                     on:mousedown=|e| e.stop_propagation()
                     on:keydown=move |e: ev::KeyboardEvent| {
                         if e.key() == "Escape" {
                             e.stop_propagation();
                             ctx.dates.set(None);
                             if let Some(el) = ctx.scroller.get_untracked() {
                                 let _ = el.focus();
                             }
                         } else {
                             // Keys typed here are the box's, not the chart's.
                             e.stop_propagation();
                         }
                     }>
                    <div class="mb-1 flex items-start gap-2">
                        <p class="min-w-0 flex-1 truncate font-semibold" title=title.clone()>{title.clone()}</p>
                        <button class="text-faint hover:text-fg" aria-label="Close the dates" on:click=close>"✕"</button>
                    </div>
                    <p class="mb-2 text-[11px] text-muted">{summary}</p>
                    <div class="flex flex-col gap-1.5">{lines}</div>
                    <p class="mt-2 text-[11px] text-muted">
                        "Same rules as dragging; each change is one undo step."
                    </p>
                </div>
            }
            .into_any(),
        )
    })
}

/// The date box in the chart, under the row and just after its bar.
fn date_box_here(ctx: Ctx) -> Option<AnyView> {
    let node = ctx.dates.get()?;
    let row_h = ctx.dims().row_h;
    let ppd = ctx.ppd();
    let (left, top) = ctx.shown.with(|d| {
        let d = d.as_ref()?;
        let list = tree(
            d,
            &ctx.toggled.get(),
            ctx.show_done.get(),
            ctx.by_person.get(),
        );
        let i = list.iter().position(|r| r.key == node.id)?;
        let geo = Geo::new(d, ppd);
        let end_x = match list[i].kind {
            RowKind::Task(t) => Some(geo.x(t.schedule.ef)),
            RowKind::Project(p) => {
                let m = marks(&p.band, geo);
                m.target_x.or(m.work.map(|(x, w)| x + w))
            }
            RowKind::Group(g) => {
                let m = marks(&g.band, geo);
                m.target_x.or(m.work.map(|(x, w)| x + w))
            }
            RowKind::Add(_) | RowKind::Person(_) => None,
        }
        .unwrap_or_else(|| geo.x(0.0));
        let w = chart_width(d, ppd);
        Some((
            (end_x + 12.0).min(w - 310.0).max(0.0),
            (i + 1) as f64 * row_h + 2.0,
        ))
    })?;
    let inner = date_box(ctx, node)?;
    Some(
        view! { <div class="absolute z-30" style=format!("left:{left}px;top:{top}px")>{inner}</div> }
            .into_any(),
    )
}

// ---------------------------------------------------------------------------------- the table

/// The same rows as a plain table (spec 58): dates to scan, for screen readers and for print.
/// Folding works as on the chart; a sort lists every task flat.
#[component]
fn Table(ctx: Ctx) -> impl IntoView {
    let colours = use_objective_colours();
    let header = move |key: Option<SortKey>, label: &'static str| {
        let state = move || match (key, ctx.sort.get()) {
            (Some(k), Some((s, desc))) if k == s => {
                if desc {
                    "descending"
                } else {
                    "ascending"
                }
            }
            _ => "none",
        };
        let content = match key {
            Some(k) => view! {
                <button class="font-semibold hover:text-accent"
                        title="Sort the tasks by this; a third click goes back to the tree"
                        on:click=move |_| ctx.sort.update(|s| {
                            *s = match *s {
                                Some((cur, false)) if cur == k => Some((k, true)),
                                Some((cur, true)) if cur == k => None,
                                _ => Some((k, false)),
                            }
                        })>
                    {label}
                    {move || match state() {
                        "ascending" => " ↑",
                        "descending" => " ↓",
                        _ => "",
                    }}
                </button>
            }
            .into_any(),
            None => view! { <span class="font-semibold">{label}</span> }.into_any(),
        };
        view! {
            <th scope="col" aria-sort=move || key.map(|_| state())
                class="sticky top-0 z-10 border-b border-line bg-panel px-2 text-left text-[11px] text-muted"
                style=format!("height:{TABLE_HEAD_H}px")>
                {content}
            </th>
        }
    };
    let day = |d: Option<Date>| d.map_or("—".to_owned(), minimap_types::short_day);
    let body = move || {
        let toggled = ctx.toggled.get();
        let show_done = ctx.show_done.get();
        let selected = ctx.selection.0.get().map(|n| n.id);
        let dates = ctx.dates.get();
        let sort = ctx.sort.get();
        ctx.shown.with(|data| {
            let data = data.as_ref()?;
            let line = |key: Uuid,
                        node: Option<NodeRef>,
                        name: AnyView,
                        c: crate::gantt::Cells|
             -> AnyView {
                let is_sel = selected == Some(key);
                let room = c.room.map(|(text, bad)| {
                    view! { <span class=if bad { "text-danger" } else { "text-muted" }>{text}</span> }
                });
                let date_line = dates.filter(|n| Some(*n) == node).and_then(|n| date_box(ctx, n)).map(|b| {
                    view! { <tr><td colspan="8" class="px-2 py-1.5">{b}</td></tr> }
                });
                view! {
                    <tr id=row_dom_id(key) aria-selected=is_sel.to_string()
                        class=if is_sel { "border-b border-line/60 bg-active" } else { "border-b border-line/60 hover:bg-hover" }
                        on:click=move |_| {
                            if let Some(n) = node {
                                ctx.selection.open(n);
                            }
                        }>
                        <td class="max-w-[28rem] px-2 py-1">{name}</td>
                        <td class="px-2 text-muted">{c.kind}</td>
                        <td class="px-2">{c.status}</td>
                        <td class="px-2 tabular-nums">{day(c.start)}</td>
                        <td class="px-2 tabular-nums">{day(c.finish)}</td>
                        <td class="px-2 tabular-nums">{day(c.due)}</td>
                        <td class="px-2">{room}</td>
                        <td class="px-2">{c.critical.then_some("Critical")}</td>
                    </tr>
                    {date_line}
                }
                .into_any()
            };
            let out: Vec<AnyView> = match sort {
                Some((key, desc)) => sorted_tasks(data, show_done, key, desc)
                    .into_iter()
                    .map(|t| {
                        let row = Row {
                            kind: RowKind::Task(t),
                            key: t.schedule.id,
                            depth: 2,
                        };
                        let name = view! {
                            <span class="flex min-w-0 items-baseline gap-2">
                                <span class="truncate">{t.schedule.title.clone()}</span>
                                <span class="truncate text-[11px] text-muted">
                                    {t.schedule.project_title.clone().unwrap_or_default()}
                                </span>
                            </span>
                        }
                        .into_any();
                        line(row.key, node_of(&row), name, cells(data, &row))
                    })
                    .collect(),
                None => rows(data, &toggled, show_done)
                    .iter()
                    .filter(|r| !matches!(r.kind, RowKind::Add(_)))
                    .map(|r| {
                        let key = r.key;
                        let can = can_open(r);
                        let open = row_open(r, &toggled);
                        let hue = match r.kind {
                            RowKind::Group(g) => g.objective.as_ref().and_then(|o| colours.hue(o.id)),
                            _ => None,
                        };
                        let weight = match r.kind {
                            RowKind::Group(_) => "truncate font-semibold",
                            RowKind::Project(_) => "truncate font-medium",
                            _ => "truncate",
                        };
                        let name = view! {
                            <span class="flex min-w-0 items-center gap-1"
                                  style=format!("padding-left:{}px", f64::from(r.depth) * 14.0)>
                                {if can {
                                    view! {
                                        <button class="w-4 shrink-0 text-muted hover:text-fg"
                                                aria-label=if open { "Fold" } else { "Open" }
                                                aria-expanded=open.to_string()
                                                on:click=move |e| { e.stop_propagation(); toggle(ctx.toggled, key); }>
                                            <span class=if open { "inline-block rotate-90" } else { "inline-block" }>"▸"</span>
                                        </button>
                                    }
                                    .into_any()
                                } else {
                                    view! { <span class="w-4 shrink-0"></span> }.into_any()
                                }}
                                {hue.map(|h| view! { <span class="obj-dot" style=format!("--obj-h: {h}")></span> })}
                                <span class=weight>{row_title(r)}</span>
                            </span>
                        }
                        .into_any();
                        line(key, node_of(r), name, cells(data, r))
                    })
                    .collect(),
            };
            Some(out)
        })
    };
    view! {
        <table class="w-full border-collapse text-[12px]">
            <caption class="sr-only">"The plan as a table: objectives, projects and tasks with their dates"</caption>
            <thead>
                <tr>
                    {header(Some(SortKey::Name), "Name")}
                    {header(None, "Kind")}
                    {header(Some(SortKey::Status), "Status")}
                    {header(Some(SortKey::Start), "Start")}
                    {header(Some(SortKey::Finish), "Finish")}
                    {header(Some(SortKey::Due), "Due")}
                    {header(Some(SortKey::Late), "Room or late")}
                    {header(None, "Critical")}
                </tr>
            </thead>
            <tbody>{body}</tbody>
        </table>
    }
}
