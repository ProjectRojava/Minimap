//! Dependencies: how work depends on other work, left to right. Tasks joined by `blocks`, or
//! projects joined by `depends_on`, narrowed by project, team or objective. The layout is
//! computed by the backend; this draws it as SVG with drag to pan and wheel / buttons to zoom.
//!
//! Spec 47 made it a place to work: boxes say who and when, a lens answers one question at a time
//! (route, risk, people), a click traces a chain, links can be drawn, given a lag and removed, and
//! the unlinked work waits in a tray. A box opens (its ⌄ icon) into a large one that holds its
//! details and quick actions: the backend lays the graph out around it, one box at a time. A
//! double-click opens the detail pane. The logic is `graph_view.rs`.

use std::collections::HashMap;

use leptos::{ev, prelude::*, task::spawn_local, web_sys};
use leptos_router::hooks::use_navigate;
use minimap_types::{
    DependencyGraph, GraphEdge, GraphFilter, GraphLevel, GraphNode, NodeType, UpdateTask, Uuid,
};

use crate::{
    api,
    components::{
        fog::{FogCtx, StillAccurate},
        form::{SelectField, BUTTON, BUTTON_DANGER, BUTTON_ON, BUTTON_SOFT, COMPACT_INPUT},
        objective_colour::{assign, use_objective_colours, ObjectiveChips},
        page::{EmptyState, PageHeader, Tone, FILTER_BAR},
        task_board::priority_pill,
        task_type::TypeChip,
        what_if_button::DEFAULT_SLIP_DAYS,
    },
    graph_view::{
        detail_level, edge_class, edge_label, edge_sentence, initials, insights, inspector_lines,
        keep_in_place, lag_attrs, legend, line_three, line_two, link_message, midpoint, neighbours,
        new_link, node_class, status_choices, status_word, trace_edge, trace_node, why_lines,
        Detail, Lens, Neighbour, Trace,
    },
    state::{finish, finish_task, DataVersion, FinishDecision, Scenario, Selection, Toasts},
};

// ------------------------------------------------------------------ pure logic

pub const MIN_ZOOM: f64 = 0.15;
pub const MAX_ZOOM: f64 = 3.0;

/// Zoom and pan that fit a `cw x ch` drawing in a `vw x vh` view, centred, never enlarging
/// beyond actual size.
pub fn fit(cw: f64, ch: f64, vw: f64, vh: f64) -> (f64, (f64, f64)) {
    if cw <= 0.0 || ch <= 0.0 || vw <= 0.0 || vh <= 0.0 {
        return (1.0, (0.0, 0.0));
    }
    let z = (vw / cw).min(vh / ch).clamp(MIN_ZOOM, 1.0);
    (z, ((vw - cw * z) / 2.0, (vh - ch * z) / 2.0))
}

/// Zooms by `factor` keeping the drawing point under `cursor` (view pixels) where it is.
pub fn zoom_at(zoom: f64, pan: (f64, f64), cursor: (f64, f64), factor: f64) -> (f64, (f64, f64)) {
    let z = (zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let k = z / zoom;
    (
        z,
        (
            cursor.0 - (cursor.0 - pan.0) * k,
            cursor.1 - (cursor.1 - pan.1) * k,
        ),
    )
}

/// The point of the drawing under a point of the view (view pixels from the view's corner).
pub fn to_drawing(view: (f64, f64), pan: (f64, f64), zoom: f64) -> (f64, f64) {
    ((view.0 - pan.0) / zoom, (view.1 - pan.1) / zoom)
}

/// The level a further zoom step switches to (spec 52): zooming out of the smallest scale of the
/// Tasks level goes to the Projects level, and zooming in past the largest scale of the Projects
/// level goes back to tasks. `delta` is the direction of the step (positive = in, negative =
/// out). Anywhere else a step just zooms.
pub fn next_level(
    scale: f64,
    min: f64,
    max: f64,
    delta: f64,
    level: GraphLevel,
) -> Option<GraphLevel> {
    const EPS: f64 = 1e-6;
    match level {
        GraphLevel::Tasks if delta < 0.0 && scale <= min + EPS => Some(GraphLevel::Projects),
        GraphLevel::Projects if delta > 0.0 && scale >= max - EPS => Some(GraphLevel::Tasks),
        _ => None,
    }
}

/// The project of the task box nearest a point of the drawing (the middle of the view when
/// zooming out of the tasks).
pub fn nearest_project(graph: &DependencyGraph, at: (f64, f64)) -> Option<Uuid> {
    graph
        .nodes
        .iter()
        .filter(|n| n.node.node_type == NodeType::Task)
        .filter_map(|n| {
            let (cx, cy) = (n.x + n.w / 2.0, n.y + n.h / 2.0);
            Some((n.project_id?, (cx - at.0).powi(2) + (cy - at.1).powi(2)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(project, _)| project)
}

/// The project box under a point of the drawing, if one is.
pub fn project_at(graph: &DependencyGraph, at: (f64, f64)) -> Option<Uuid> {
    graph
        .nodes
        .iter()
        .filter(|n| n.node.node_type == NodeType::Project)
        .find(|n| at.0 >= n.x && at.0 <= n.x + n.w && at.1 >= n.y && at.1 <= n.y + n.h)
        .map(|n| n.node.id)
}

/// The pan that puts the middle of a box in the middle of a `view`-sized view at this zoom.
pub fn centre_on(node: &GraphNode, zoom: f64, view: (f64, f64)) -> (f64, f64) {
    (
        view.0 / 2.0 - (node.x + node.w / 2.0) * zoom,
        view.1 / 2.0 - (node.y + node.h / 2.0) * zoom,
    )
}

/// The smallest scale a drawing is left at when it is framed around one project: small enough
/// to see its neighbours, never so small that the box is a speck.
pub const FOCUS_MIN_ZOOM: f64 = 0.6;

thread_local! {
    /// Which level-switch hints have been shown this session (bit 0 out, bit 1 in).
    static HINTED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// True the first time it is asked for `bit` in a session.
fn first_hint(bit: u8) -> bool {
    HINTED.with(|h| {
        let shown = h.get() & bit != 0;
        h.set(h.get() | bit);
        !shown
    })
}

/// A smooth route through the points, with horizontal tangents at each one.
pub fn edge_path(points: &[(f64, f64)]) -> String {
    let Some(first) = points.first() else {
        return String::new();
    };
    let mut d = format!("M{:.1} {:.1}", first.0, first.1);
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        let mid = (x1 - x0) / 2.0;
        d.push_str(&format!(
            " C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
            x0 + mid,
            y0,
            x1 - mid,
            y1,
            x1,
            y1
        ));
    }
    d
}

/// `text` cut with an ellipsis so it fits `max_px` at roughly `px_per_char`.
pub fn fit_text(text: &str, max_px: f64, px_per_char: f64) -> String {
    let max_chars = (max_px / px_per_char).floor().max(1.0) as usize;
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// "12 tasks · 5 on the critical path · 3 unlinked hidden".
pub fn summary_text(g: &DependencyGraph) -> String {
    let what = match g.level {
        GraphLevel::Tasks => "task",
        GraphLevel::Projects => "project",
    };
    let shown = g.nodes.iter().filter(|n| !n.context).count();
    let mut parts = vec![format!(
        "{shown} {what}{}",
        if shown == 1 { "" } else { "s" }
    )];
    let context = g.nodes.len() - shown;
    if context > 0 {
        parts.push(format!("{context} more for context"));
    }
    if g.level == GraphLevel::Tasks && g.critical_nodes > 0 {
        parts.push(format!("{} on the critical path", g.critical_nodes));
    }
    if g.hidden_unlinked > 0 {
        parts.push(format!("{} unlinked hidden", g.hidden_unlinked));
    }
    parts.join(" · ")
}

// ------------------------------------------------------------------ the screen

fn parse_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}

/// What is picked on the graph: a box or a link.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Picked {
    Node(Uuid),
    Edge(Uuid),
}

/// A link being made: from a box's handle (dragging), or from a chip in the tray or a button in an
/// expanded box (then click the other box). `reverse` makes the clicked box the one that blocks
/// (or is needed by) the source, instead of the one that is blocked.
#[derive(Clone, Copy, PartialEq)]
struct Linking {
    source: Uuid,
    cursor: Option<(f64, f64)>,
    from_tray: bool,
    reverse: bool,
}

/// Where a box was before the layout made room for it (or took the room back), so it can be kept
/// where it was on screen once the new layout arrives.
#[derive(Clone, Copy, PartialEq)]
struct Anchor {
    id: Uuid,
    at: (f64, f64),
    /// Whether the new layout has the box expanded.
    want: bool,
}

/// Opens and closes the one expanded box. Opening another closes the first: the layout only ever
/// makes room for one.
#[derive(Clone, Copy)]
struct Expander {
    expanded: RwSignal<Option<Uuid>>,
    anchor: RwSignal<Option<Anchor>>,
    picked: RwSignal<Option<Picked>>,
    /// Each box's top-left corner in the layout now on screen.
    places: RwSignal<HashMap<Uuid, (f64, f64)>>,
}

impl Expander {
    fn set(&self, id: Option<Uuid>) {
        let current = self.expanded.get_untracked();
        if current == id {
            return;
        }
        let places = self.places.get_untracked();
        let remember =
            |id: Uuid, want: bool| places.get(&id).map(|at| Anchor { id, at: *at, want });
        match (id, current) {
            (Some(new), _) => {
                self.anchor.set(remember(new, true));
                self.picked.set(Some(Picked::Node(new)));
            }
            (None, Some(old)) => self.anchor.set(remember(old, false)),
            (None, None) => {}
        }
        self.expanded.set(id);
    }

    fn toggle(&self, id: Uuid) {
        if self.expanded.get_untracked() == Some(id) {
            self.set(None);
        } else {
            self.set(Some(id));
        }
    }
}

#[component]
pub fn Graph() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();

    let level = RwSignal::new(GraphLevel::Tasks);
    let lens = RwSignal::new(Lens::Route);
    let project = RwSignal::new(String::new());
    let team = RwSignal::new(String::new());
    let objective = RwSignal::new(String::new());
    let include_done = RwSignal::new(false);
    let include_isolated = RwSignal::new(false);
    let picked = RwSignal::new(Option::<Picked>::None);
    let linking = RwSignal::new(Option::<Linking>::None);
    let tray_open = RwSignal::new(false);
    let expander = Expander {
        expanded: RwSignal::new(None),
        anchor: RwSignal::new(None),
        picked,
        places: RwSignal::new(HashMap::new()),
    };

    let zoom = RwSignal::new(1.0_f64);
    let pan = RwSignal::new((0.0_f64, 0.0_f64));
    let drag = RwSignal::new(Option::<((f64, f64), (f64, f64))>::None);
    let moved = RwSignal::new(false);
    let needs_fit = RwSignal::new(true);
    // The project to centre and pick once the Projects level has been drawn (spec 52).
    let pending_focus = RwSignal::new(Option::<Uuid>::None);
    let viewport = leptos::prelude::NodeRef::<leptos::html::Div>::new();

    let orientation = expect_context::<crate::components::orientation::Orientation>();
    let graph = LocalResource::new(move || {
        version.track();
        api::get_dependency_graph(GraphFilter {
            level: level.get(),
            project_id: parse_id(&project.get()),
            team_id: if level.get() == GraphLevel::Tasks {
                parse_id(&team.get())
            } else {
                None
            },
            objective_id: parse_id(&objective.get()),
            include_done: include_done.get(),
            include_isolated: include_isolated.get(),
            mine: orientation.is_mine(),
            expanded: expander.expanded.get(),
        })
    });
    // The last graph that arrived, so the drawing never blanks while a new layout is worked out
    // (opening a box asks for one).
    let shown = RwSignal::new(Option::<DependencyGraph>::None);
    Effect::new(move |_| {
        if let Some(Ok(g)) = graph.get() {
            if let Some(a) = expander.anchor.get_untracked() {
                match g.nodes.iter().find(|n| n.node.id == a.id) {
                    // The layout has caught up: put the box back where it was on screen.
                    Some(n) if n.expanded == a.want => {
                        let z = zoom.get_untracked();
                        pan.update(|p| *p = keep_in_place(*p, z, a.at, (n.x, n.y)));
                        expander.anchor.set(None);
                    }
                    Some(_) => {}
                    None => expander.anchor.set(None),
                }
            }
            expander
                .places
                .set(g.nodes.iter().map(|n| (n.node.id, (n.x, n.y))).collect());
            shown.set(Some(g));
        }
    });
    let projects = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Project)
    });
    let teams = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Team)
    });
    let objectives = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Objective)
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Person)
    });
    // Each person's colour, ranked by id like the board's, so an owner looks the same everywhere.
    let hues = Memo::new(
        move |last: Option<&HashMap<Uuid, u16>>| match people.get() {
            Some(Ok(list)) => assign(&list.iter().map(|p| p.node.id).collect::<Vec<_>>()),
            _ => last.cloned().unwrap_or_default(),
        },
    );
    // What the picked box or link lights.
    let trace = Memo::new(move |_| match (picked.get(), shown.get()) {
        (Some(Picked::Node(id)), Some(g)) => Some(trace_node(&g, id)),
        (Some(Picked::Edge(id)), Some(g)) => Some(trace_edge(&g, id)),
        _ => None,
    });

    // A different filter or level frames the new drawing; edits keep your place.
    Effect::new(move |first: Option<()>| {
        level.track();
        project.track();
        team.track();
        objective.track();
        include_done.track();
        include_isolated.track();
        picked.set(None);
        linking.set(None);
        expander.anchor.set(None);
        if expander.expanded.get_untracked().is_some() {
            expander.expanded.set(None);
        }
        if first.is_some() {
            needs_fit.set(true);
        }
    });
    let fit_now = move || {
        if let (Some(el), Some(g)) = (viewport.get_untracked(), shown.get_untracked()) {
            let r = el.get_bounding_client_rect();
            let (z, p) = fit(g.width, g.height, r.width(), r.height());
            zoom.set(z);
            pan.set(p);
        }
    };
    Effect::new(move |_| {
        if shown.get().is_some() && needs_fit.get_untracked() {
            needs_fit.set(false);
            fit_now();
            // Zoomed out from the tasks: the project they belonged to is centred and picked.
            if let (Some(id), Some(g), Some(el)) = (
                pending_focus.get_untracked(),
                shown.get_untracked(),
                viewport.get_untracked(),
            ) {
                if g.level == GraphLevel::Projects {
                    if let Some(n) = g.nodes.iter().find(|n| n.node.id == id) {
                        let r = el.get_bounding_client_rect();
                        let z = zoom.get_untracked().max(FOCUS_MIN_ZOOM);
                        zoom.set(z);
                        pan.set(centre_on(n, z, (r.width(), r.height())));
                        picked.set(Some(Picked::Node(id)));
                    }
                    pending_focus.set(None);
                }
            }
        }
    });

    // One zoom step by `factor` around `cursor` (view pixels). Out of the smallest scale of the
    // tasks, or in past the largest scale of the projects, it changes level instead (spec 52).
    let zoom_step =
        move |factor: f64, cursor: (f64, f64), view: (f64, f64), prefer_picked: bool| {
            let to = next_level(
                zoom.get_untracked(),
                MIN_ZOOM,
                MAX_ZOOM,
                factor.ln(),
                level.get_untracked(),
            );
            match to {
                Some(GraphLevel::Projects) => {
                    let middle = to_drawing(
                        (view.0 / 2.0, view.1 / 2.0),
                        pan.get_untracked(),
                        zoom.get_untracked(),
                    );
                    pending_focus.set(
                        shown
                            .get_untracked()
                            .and_then(|g| nearest_project(&g, middle)),
                    );
                    project.set(String::new());
                    level.set(GraphLevel::Projects);
                    if first_hint(1) {
                        toasts.info("Zoomed out to projects");
                    }
                }
                Some(_) => {
                    let graph = shown.get_untracked();
                    let under = graph.as_ref().and_then(|g| {
                        project_at(
                            g,
                            to_drawing(cursor, pan.get_untracked(), zoom.get_untracked()),
                        )
                    });
                    let chosen = match picked.get_untracked() {
                        Some(Picked::Node(id))
                            if graph.as_ref().is_some_and(|g| {
                                g.nodes.iter().any(|n| {
                                    n.node.id == id && n.node.node_type == NodeType::Project
                                })
                            }) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    };
                    let target = if prefer_picked {
                        chosen.or(under)
                    } else {
                        under.or(chosen)
                    };
                    if let Some(id) = target {
                        let name = graph
                            .and_then(|g| g.nodes.into_iter().find(|n| n.node.id == id))
                            .map(|n| n.label)
                            .unwrap_or_default();
                        level.set(GraphLevel::Tasks);
                        project.set(id.to_string());
                        if first_hint(2) {
                            toasts.info(format!("Zoomed in to the tasks of {name}"));
                        }
                    }
                }
                None => {
                    let (z, p) = zoom_at(zoom.get_untracked(), pan.get_untracked(), cursor, factor);
                    zoom.set(z);
                    pan.set(p);
                }
            }
        };
    let zoom_by = move |factor: f64| {
        if let Some(el) = viewport.get_untracked() {
            let r = el.get_bounding_client_rect();
            let view = (r.width(), r.height());
            zoom_step(factor, (view.0 / 2.0, view.1 / 2.0), view, true);
        }
    };
    let on_wheel = move |ev: ev::WheelEvent| {
        ev.prevent_default();
        if let Some(el) = viewport.get_untracked() {
            let r = el.get_bounding_client_rect();
            let cursor = (
                f64::from(ev.client_x()) - r.left(),
                f64::from(ev.client_y()) - r.top(),
            );
            zoom_step(
                (-ev.delta_y() * 0.0015).exp(),
                cursor,
                (r.width(), r.height()),
                false,
            );
        }
    };
    let on_down = move |ev: ev::MouseEvent| {
        moved.set(false);
        if linking.get_untracked().is_some() {
            return;
        }
        drag.set(Some((
            (f64::from(ev.client_x()), f64::from(ev.client_y())),
            pan.get_untracked(),
        )));
    };
    let on_move = move |ev: ev::MouseEvent| {
        if let Some((start, pan0)) = drag.get_untracked() {
            let (dx, dy) = (
                f64::from(ev.client_x()) - start.0,
                f64::from(ev.client_y()) - start.1,
            );
            if dx.abs() + dy.abs() > 3.0 {
                moved.set(true);
            }
            pan.set((pan0.0 + dx, pan0.1 + dy));
        }
        // A link being dragged follows the mouse, in the drawing's own coordinates.
        if let (Some(l), Some(el)) = (linking.get_untracked(), viewport.get_untracked()) {
            if !l.from_tray {
                let r = el.get_bounding_client_rect();
                let at = to_drawing(
                    (
                        f64::from(ev.client_x()) - r.left(),
                        f64::from(ev.client_y()) - r.top(),
                    ),
                    pan.get_untracked(),
                    zoom.get_untracked(),
                );
                linking.set(Some(Linking {
                    cursor: Some(at),
                    ..l
                }));
            }
        }
    };
    let end_drag = move |_| {
        drag.set(None);
        // Letting go anywhere but on a box drops a dragged link.
        if linking.get_untracked().is_some_and(|l| !l.from_tray) {
            linking.set(None);
        }
    };
    // A click on the background clears the pick and a link waiting for its target (not after a pan).
    // An expanded box stays open: it closes with its own button, Esc, or by opening another.
    let on_background = move |_| {
        if !moved.get_untracked() {
            picked.set(None);
            linking.set(None);
        }
    };

    // Escape clears (and closes the open box); Delete removes the picked link.
    let handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        let typing = e
            .target()
            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(t).ok())
            .map(|el| crate::nav::is_typing_target(&el.tag_name(), el.is_content_editable()))
            .unwrap_or(false);
        if typing {
            return;
        }
        let plain = !(e.ctrl_key() || e.meta_key() || e.alt_key());
        match e.key().as_str() {
            // `-` and `+` zoom like the buttons (and change level at the ends, spec 52).
            "-" | "_" if plain => zoom_by(1.0 / 1.25),
            "+" | "=" if plain => zoom_by(1.25),
            "Escape" => {
                picked.set(None);
                linking.set(None);
                expander.set(None);
            }
            "Delete" | "Backspace" => {
                if let Some(Picked::Edge(id)) = picked.get_untracked() {
                    e.prevent_default();
                    remove_link(id, picked, toasts, version);
                }
            }
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());

    let level_button = move |l: GraphLevel, text: &'static str| {
        view! {
            <button class=move || format!("{BUTTON} {}", if level.get() == l { BUTTON_ON } else { "" })
                    on:click=move |_| { project.set(String::new()); level.set(l) }>{text}</button>
        }
    };
    let lens_buttons = Lens::ALL
        .into_iter()
        .map(|l| {
            view! {
                <button class=move || format!("{BUTTON} {}", if lens.get() == l { BUTTON_ON } else { "" })
                        title=l.hint() on:click=move |_| lens.set(l)>{l.label()}</button>
            }
        })
        .collect_view();
    let option_list =
        move |any: &'static str,
              res: Option<Result<Vec<minimap_types::NodeSummary>, minimap_types::AppError>>|
              -> Vec<(String, String)> {
            std::iter::once((String::new(), any.to_owned()))
                .chain(match res {
                    Some(Ok(v)) => v
                        .into_iter()
                        .map(|n| (n.node.id.to_string(), n.label))
                        .collect(),
                    _ => Vec::new(),
                })
                .collect()
        };

    let body = move || {
        match (shown.get(), graph.get()) {
        (None, Some(Err(e))) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
        (None, _) => view! { <p class="p-6 text-muted">"Laying it out…"</p> }.into_any(),
        (Some(g), _) if g.nodes.is_empty() => view! {
            <EmptyState icon="graph" title="No dependencies to draw"
                hint="Link tasks with Links → Blocks (or projects with Depends on), then they appear here. Tasks and projects with no links are listed under Unlinked below, and you can link one from there." />
        }
        .into_any(),
        (Some(g), _) => view! {
            <Drawing graph=g zoom=zoom pan=pan lens=lens picked=picked linking=linking
                     trace=trace hues=hues expander=expander />
        }
        .into_any(),
    }
    };
    let summary = move || shown.get().map(|g| summary_text(&g)).unwrap_or_default();
    let chips = move || {
        match shown.get() {
        Some(g) => insights(&g)
            .into_iter()
            .map(|i| {
                let node = i.node;
                let class = format!("{} {}", Tone::Neutral.chip(), if node.is_some() { "cursor-pointer hover:bg-hover" } else { "" });
                view! {
                    <button class=class disabled=node.is_none()
                            on:click=move |_| { if let Some(id) = node { picked.set(Some(Picked::Node(id))); } }>
                        {i.text}
                    </button>
                }
            })
            .collect_view()
            .into_any(),
        None => ().into_any(),
    }
    };
    let key = move || {
        legend(lens.get())
            .into_iter()
            .map(|(tone, text)| view! { <span class=tone.chip()>{text}</span> })
            .collect_view()
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="graph" title="Dependencies"
                        subtitle="How work depends on other work, left to right">
                {level_button(GraphLevel::Tasks, "Tasks")}
                {level_button(GraphLevel::Projects, "Projects")}
                <span class="ml-2 flex items-center gap-1" role="group" aria-label="Lens">
                    <span class="mr-1 text-[10px] uppercase tracking-wider text-muted">"Lens"</span>
                    {lens_buttons}
                </span>
                <span class="ml-auto flex items-center gap-1">
                    <button class=BUTTON aria-label="Zoom out" on:click=move |_| zoom_by(1.0 / 1.25)>"−"</button>
                    <span class="w-12 text-center tabular-nums text-muted">{move || format!("{}%", (zoom.get() * 100.0).round() as i64)}</span>
                    <button class=BUTTON aria-label="Zoom in" on:click=move |_| zoom_by(1.25)>"+"</button>
                    <button class=BUTTON on:click=move |_| fit_now()>"Fit"</button>
                </span>
            </PageHeader>
            <crate::components::orientation::MineBanner />
            <div class=FILTER_BAR>
                {move || view! { <SelectField compact=true current=project.get_untracked()
                    options=option_list(if level.get() == GraphLevel::Tasks { "Any project" } else { "Any project (focus)" }, projects.get())
                    on_change=move |v: String| project.set(v) /> }}
                {move || (level.get() == GraphLevel::Tasks).then(|| view! {
                    <SelectField compact=true current=team.get_untracked()
                        options=option_list("Any team", teams.get())
                        on_change=move |v: String| team.set(v) />
                })}
                {move || view! { <SelectField compact=true current=objective.get_untracked()
                    options=option_list("Any objective", objectives.get())
                    on_change=move |v: String| objective.set(v) /> }}
                <label class="flex items-center gap-1 text-[11px] text-muted">
                    <input type="checkbox" prop:checked=move || include_done.get()
                           on:change=move |ev| include_done.set(event_target_checked(&ev)) />
                    "Show finished"
                </label>
                <label class="flex items-center gap-1 text-[11px] text-muted">
                    <input type="checkbox" prop:checked=move || include_isolated.get()
                           on:change=move |ev| include_isolated.set(event_target_checked(&ev)) />
                    "Draw unlinked"
                </label>
                <span class="ml-auto text-[11px] text-muted">{summary}</span>
            </div>
            <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-line px-4 py-1 empty:hidden">{chips}</div>
            <div node_ref=viewport class="relative min-h-0 flex-1 overflow-hidden"
                 on:mousedown=on_down on:mousemove=on_move on:mouseup=end_drag on:mouseleave=end_drag
                 on:click=on_background on:wheel=on_wheel>
                {body}
                <LinkBanner graph=shown linking=linking />
                <EdgeCard graph=shown picked=picked pan=pan zoom=zoom />
                <div class="pointer-events-none absolute bottom-2 left-3 flex flex-wrap items-center gap-2 text-[11px] text-muted">
                    {key}
                    <span>"drag to pan · scroll to zoom (out far enough for projects) · click to trace · ⌄ for details · double-click to open · drag from a box's handle to link"</span>
                </div>
            </div>
            <Tray graph=shown linking=linking open=tray_open />
        </div>
    }
}

/// Removes a link, and says how to get it back.
fn remove_link(id: Uuid, picked: RwSignal<Option<Picked>>, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        if finish(api::remove_edge(id).await, toasts, version).is_some() {
            picked.set(None);
            toasts.info("Removed the link. Press Ctrl/Cmd+Z to undo.");
        }
    });
}

/// Makes the link a drag or a click means, and says what it did (or why it could not).
fn make_link(
    level: GraphLevel,
    source: Uuid,
    target: Uuid,
    names: (String, String),
    toasts: Toasts,
    version: DataVersion,
) {
    spawn_local(async move {
        if finish(
            api::add_edge(new_link(level, source, target)).await,
            toasts,
            version,
        )
        .is_some()
        {
            toasts.info(link_message(level, &names.0, &names.1));
        }
    });
}

/// "Click the box X should block" while a chip or a button is waiting for its other box.
#[component]
fn LinkBanner(
    graph: RwSignal<Option<DependencyGraph>>,
    linking: RwSignal<Option<Linking>>,
) -> impl IntoView {
    move || {
        let l = linking.get().filter(|l| l.from_tray)?;
        let g = graph.get()?;
        let name = g
            .nodes
            .iter()
            .find(|n| n.node.id == l.source)
            .map(|n| n.label.clone())
            .or_else(|| {
                g.unlinked
                    .iter()
                    .find(|u| u.node.id == l.source)
                    .map(|u| u.label.clone())
            })
            .map_or_else(|| "it".to_owned(), |n| format!("“{n}”"));
        let text = match (g.level, l.reverse) {
            (GraphLevel::Tasks, false) => {
                format!("Click the task {name} should block (Esc cancels)")
            }
            (GraphLevel::Tasks, true) => {
                format!("Click the task that should block {name} (Esc cancels)")
            }
            (GraphLevel::Projects, false) => {
                format!("Click the project that needs {name} (Esc cancels)")
            }
            (GraphLevel::Projects, true) => {
                format!("Click the project {name} needs (Esc cancels)")
            }
        };
        Some(view! {
            <div class="pointer-events-none absolute left-1/2 top-3 z-10 -translate-x-1/2 rounded-sm border border-accent bg-panel px-3 py-1.5 text-[12px] text-accent">
                {text}
            </div>
        })
    }
}

/// The popover on a picked link, at the middle of the arrow: what it means, its lag, remove.
#[component]
fn EdgeCard(
    graph: RwSignal<Option<DependencyGraph>>,
    picked: RwSignal<Option<Picked>>,
    pan: RwSignal<(f64, f64)>,
    zoom: RwSignal<f64>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    move || {
        let Picked::Edge(id) = picked.get()? else {
            return None;
        };
        let g = graph.get()?;
        let e = g.edges.iter().find(|e| e.id == id)?.clone();
        let (mx, my) = midpoint(&e.points)?;
        let (px, py) = pan.get();
        let z = zoom.get();
        let (left, top) = (px + mx * z, py + my * z);
        let sentence = edge_sentence(&g, id);
        let tasks = g.level == GraphLevel::Tasks;
        let lag = e.lag_days.unwrap_or(0);
        let on_lag = move |ev| {
            let text: String = event_target_value(&ev);
            match text.trim().parse::<u32>() {
                Ok(days) if days <= 365 => {
                    spawn_local(async move {
                        finish(
                            api::update_edge_attrs(id, lag_attrs(days)).await,
                            toasts,
                            version,
                        );
                    });
                }
                _ => {
                    toasts.error(&minimap_types::AppError {
                        code: "invalid".into(),
                        message: "A lag is a whole number of working days, 0 to 365".into(),
                    });
                    version.bump();
                }
            }
        };
        Some(view! {
            <div class="absolute z-10 w-72 -translate-x-1/2 rounded-sm border border-line bg-panel p-3 text-[12px]"
                 style=format!("left:{left:.1}px; top:{:.1}px", top + 10.0)
                 on:mousedown=|ev| ev.stop_propagation() on:click=|ev| ev.stop_propagation()>
                <div class="flex items-start gap-2">
                    <p class="min-w-0 flex-1">{sentence}</p>
                    <button class="rounded px-1 text-muted hover:bg-hover" aria-label="Close"
                            on:click=move |_| picked.set(None)>"✕"</button>
                </div>
                {tasks.then(|| view! {
                    <label class="mt-2 flex items-center gap-2 text-muted">
                        "Lag"
                        <input type="number" min="0" max="365" class=format!("{COMPACT_INPUT} w-16")
                               prop:value=lag.to_string() on:change=on_lag />
                        "working days"
                    </label>
                })}
                <div class="mt-3">
                    <button class=BUTTON_DANGER on:click=move |_| remove_link(id, picked, toasts, version)>
                        "Remove link"
                    </button>
                    <span class="ml-2 text-[11px] text-muted">"or press Delete"</span>
                </div>
            </div>
        })
    }
}

/// The work with no links, as chips under the graph: click one, then the box it should block.
#[component]
fn Tray(
    graph: RwSignal<Option<DependencyGraph>>,
    linking: RwSignal<Option<Linking>>,
    open: RwSignal<bool>,
) -> impl IntoView {
    move || {
        let g = graph.get()?;
        if g.hidden_unlinked == 0 {
            return None;
        }
        let what = match g.level {
            GraphLevel::Tasks => "tasks",
            GraphLevel::Projects => "projects",
        };
        let more = (g.hidden_unlinked as usize).saturating_sub(g.unlinked.len());
        let chips = g
            .unlinked
            .iter()
            .map(|u| {
                let id = u.node.id;
                let active = move || linking.get().is_some_and(|l| l.source == id);
                view! {
                    <button class=move || format!("{} max-w-[16rem] truncate {}", Tone::Neutral.chip(),
                                if active() { "border-accent text-accent" } else { "hover:bg-hover" })
                            title=u.subtitle.clone()
                            on:click=move |ev| {
                                ev.stop_propagation();
                                linking.set(Some(Linking { source: id, cursor: None, from_tray: true, reverse: false }));
                            }>
                        {u.label.clone()}
                    </button>
                }
            })
            .collect_view();
        Some(view! {
            <div class="shrink-0 border-t border-line bg-panel">
                <button class="flex w-full items-center gap-2 px-4 py-1.5 text-left text-[12px] hover:bg-hover"
                        aria-expanded=move || open.get().to_string()
                        on:click=move |_| open.update(|o| *o = !*o)>
                    <span class="text-muted">{move || if open.get() { "▾" } else { "▸" }}</span>
                    <span class="font-medium">{format!("Unlinked ({})", g.hidden_unlinked)}</span>
                    <span class="text-muted">{format!("{what} with no links. Click one, then the box it should {}", if g.level == GraphLevel::Tasks { "block" } else { "be needed by" })}</span>
                </button>
                {move || open.get().then(|| view! {
                    <div class="flex max-h-28 flex-wrap items-center gap-1.5 overflow-y-auto border-t border-line px-4 py-2">
                        {chips.clone()}
                        {(more > 0).then(|| view! {
                            <span class="text-[11px] text-muted">{format!("and {more} more (tick “Draw unlinked” to see them all)")}</span>
                        })}
                    </div>
                })}
            </div>
        })
    }
}

/// The names a made link will mention, from the two ends' ids.
fn link_names(names: StoredValue<HashMap<Uuid, String>>, a: Uuid, b: Uuid) -> (String, String) {
    names.with_value(|m| {
        (
            m.get(&a).cloned().unwrap_or_default(),
            m.get(&b).cloned().unwrap_or_default(),
        )
    })
}

#[component]
fn Drawing(
    graph: DependencyGraph,
    zoom: RwSignal<f64>,
    pan: RwSignal<(f64, f64)>,
    lens: RwSignal<Lens>,
    picked: RwSignal<Option<Picked>>,
    linking: RwSignal<Option<Linking>>,
    trace: Memo<Option<Trace>>,
    hues: Memo<HashMap<Uuid, u16>>,
    expander: Expander,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let colours = use_objective_colours();
    let fog = use_context::<FogCtx>();
    let level = graph.level;
    let detail = Memo::new(move |_| detail_level(zoom.get()));
    let transform = move || {
        let (x, y) = pan.get();
        format!("translate({x:.1} {y:.1}) scale({:.4})", zoom.get())
    };
    // The names a made link will mention, and where each box is (for the link being dragged).
    let names: HashMap<Uuid, String> = graph
        .nodes
        .iter()
        .map(|n| (n.node.id, n.label.clone()))
        .chain(graph.unlinked.iter().map(|u| (u.node.id, u.label.clone())))
        .collect();
    let names = StoredValue::new(names);
    let places: HashMap<Uuid, (f64, f64)> = graph
        .nodes
        .iter()
        .map(|n| (n.node.id, (n.x + n.w, n.y + n.h / 2.0)))
        .collect();
    let by_id: HashMap<Uuid, GraphNode> =
        graph.nodes.iter().map(|n| (n.node.id, n.clone())).collect();
    let by_id = StoredValue::new(by_id);
    let whole = StoredValue::new(graph.clone());
    let selected_node = move || match picked.get() {
        Some(Picked::Node(id)) => Some(id),
        _ => None,
    };
    let selected_edge = move || match picked.get() {
        Some(Picked::Edge(id)) => Some(id),
        _ => None,
    };

    let edges = graph
        .edges
        .iter()
        .map(|e: &GraphEdge| {
            let edge = e.clone();
            let id = e.id;
            let d = edge_path(&e.points);
            let label = edge_label(e, level).and_then(|text| {
                let (x, y) = midpoint(&e.points)?;
                Some(view! {
                    <text class="lag" x=x y=y - 4.0 text-anchor="middle">{text}</text>
                })
            });
            let sentence = edge_sentence(&graph, e.id);
            let e2 = edge.clone();
            let class = move || {
                let from = by_id.with_value(|m| m.get(&e2.from).cloned());
                edge_class(&e2, lens.get(), from.as_ref(), trace.get().as_ref(), selected_edge())
            };
            let marker = {
                let critical = e.critical;
                move || {
                    if critical && lens.get() == Lens::Route {
                        "url(#dep-arrow-hot)"
                    } else {
                        "url(#dep-arrow)"
                    }
                }
            };
            view! {
                <path d=d.clone() class=class marker-end=marker />
                {label}
                <path d=d class="edge-hit" on:mousedown=|ev| ev.stop_propagation()
                      on:click=move |ev| { ev.stop_propagation(); picked.set(Some(Picked::Edge(id))); }>
                    <title>{sentence}</title>
                </path>
            }
        })
        .collect_view();

    let nodes = graph
        .nodes
        .iter()
        .map(|n| {
            let id = n.node.id;
            let target = n.node;
            let n = StoredValue::new(n.clone());
            let class = move || {
                let node = n.get_value();
                let mut c = node_class(&node, lens.get(), trace.get().as_ref(), selected_node());
                if lens.get() == Lens::People && node.assignee.is_some() {
                    c.push_str(" owned");
                }
                c
            };
            let person_hue = n.get_value().assignee.as_ref().map(|a| a.node.id);
            let group_style = move || {
                person_hue
                    .and_then(|p| hues.with(|h| h.get(&p).copied()))
                    .map(|h| format!("--obj-h: {h}"))
                    .unwrap_or_default()
            };
            let stripe = {
                let project = n.get_value().project_id;
                move || {
                    colours
                        .first_hue(&colours.of_project(project))
                        .map(|h| view! { <rect class="stripe" x="0" y="0" width="3" height=n.get_value().h style=format!("--obj-h: {h}") /> })
                }
            };
            // A click traces (or finishes a link waiting for this box); a double-click opens the pane.
            let on_click = move |ev: ev::MouseEvent| {
                ev.stop_propagation();
                if let Some(l) = linking.get_untracked() {
                    if l.from_tray {
                        if l.source != id {
                            // Forward: the source blocks this box; reverse: this box blocks the source.
                            let (from, to) = if l.reverse { (id, l.source) } else { (l.source, id) };
                            make_link(level, from, to, link_names(names, from, to), toasts, version);
                        }
                        linking.set(None);
                    }
                    return;
                }
                picked.set(Some(Picked::Node(id)));
            };
            let on_double = move |ev: ev::MouseEvent| {
                ev.stop_propagation();
                selection.open(target);
            };
            let on_up = move |ev: ev::MouseEvent| {
                if let Some(l) = linking.get_untracked() {
                    if !l.from_tray {
                        ev.stop_propagation();
                        if l.source != id {
                            make_link(level, l.source, id, link_names(names, l.source, id), toasts, version);
                        }
                        linking.set(None);
                    }
                }
            };
            let start_link = move |ev: ev::MouseEvent| {
                ev.stop_propagation();
                ev.prevent_default();
                linking.set(Some(Linking { source: id, cursor: None, from_tray: false, reverse: false }));
            };
            let body = move || {
                let node = n.get_value();
                if node.expanded {
                    // The box is laid out large: its details fill it.
                    return view! {
                        <foreignObject x="0" y="0" width=node.w height=node.h>
                            <NodeCard graph=whole.get_value() node=node.clone()
                                      expander=expander linking=linking />
                        </foreignObject>
                    }
                    .into_any();
                }
                let d = detail.get();
                let title_w = node.w - 20.0;
                let title = (d != Detail::Shape).then(|| view! {
                    <text class="label" x="10" y="16">{fit_text(&node.label, title_w - 22.0, 6.6)}</text>
                });
                let icon = (d != Detail::Shape).then(|| view! {
                    <g class="info" transform=format!("translate({:.1} 10)", node.w - 14.0)
                       on:mousedown=|ev| ev.stop_propagation()
                       on:click=move |ev| { ev.stop_propagation(); expander.toggle(id); }>
                        <circle r="8" />
                        <path d="M-3.5 -1.2L0 2.3L3.5 -1.2" />
                        <title>"Details"</title>
                    </g>
                });
                let full = (d == Detail::Full).then(|| {
                    let owner = node.assignee.clone();
                    let owner_hue = owner
                        .as_ref()
                        .and_then(|a| hues.with(|h| h.get(&a.node.id).copied()));
                    let badge_w = if node.priority <= 2 { 52.0 } else { 26.0 };
                    let fog_chip = fog
                        .filter(|_| level == GraphLevel::Tasks)
                        .and_then(|f| f.mark(node.node.id))
                        .map(|m| view! { <text class="fog" x=node.w - 10.0 y="46">{crate::components::fog::chip_text(&m)}</text> });
                    view! {
                        <text class="sub" x="10" y="31">{fit_text(&line_two(&node), title_w - badge_w, 5.6)}</text>
                        <text class="sub" x="10" y="46">{fit_text(&line_three(&node, lens.get()), title_w - 40.0, 5.6)}</text>
                        {fog_chip}
                        {owner.map(|a| view! {
                            <circle class="person" cx=node.w - 16.0 cy="29" r="8"
                                    style=owner_hue.map(|h| format!("--obj-h: {h}")).unwrap_or_default() />
                            <text class="initials" x=node.w - 16.0 y="32">{initials(&a.label)}</text>
                        })}
                        {(node.priority <= 2).then(|| {
                            let class = if node.priority == 1 { "p1" } else { "p2" };
                            view! {
                                <g class=class>
                                    <rect x=node.w - 52.0 y="23" width="22" height="13" rx="2" />
                                    <text x=node.w - 41.0 y="33">{format!("P{}", node.priority)}</text>
                                </g>
                            }
                        })}
                    }
                });
                view! { {title} {icon} {full} }.into_any()
            };
            let (nx, ny) = (n.get_value().x, n.get_value().y);
            let handle = move || {
                let node = n.get_value();
                (!node.expanded).then(|| view! {
                    <circle class="port" cx=node.w cy=node.h / 2.0 r="5" on:mousedown=start_link>
                        <title>"Drag to another box to link"</title>
                    </circle>
                })
            };
            let (width, height) = (n.get_value().w, n.get_value().h);
            view! {
                <g class=class style=group_style transform=format!("translate({nx:.1} {ny:.1})")
                   on:mousedown=|ev| ev.stop_propagation()
                   on:mouseup=on_up on:click=on_click on:dblclick=on_double>
                    <rect class="box" width=width height=height rx="4" />
                    {stripe}
                    {body}
                    {handle}
                </g>
            }
        })
        .collect_view();

    // The link being dragged, from the handle to the mouse.
    let rubber = move || {
        let l = linking.get().filter(|l| !l.from_tray)?;
        let (x0, y0) = places.get(&l.source).copied()?;
        let (x1, y1) = l.cursor?;
        Some(view! { <path class="rubber" d=edge_path(&[(x0, y0), (x1, y1)]) /> })
    };

    view! {
        <svg class="dep" width="100%" height="100%">
            <defs>
                <marker id="dep-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                    <path class="arrow" d="M0 0L10 5L0 10z" />
                </marker>
                <marker id="dep-arrow-hot" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                    <path class="arrow critical" d="M0 0L10 5L0 10z" />
                </marker>
            </defs>
            <g transform=transform>{edges}{nodes}{rubber}</g>
        </svg>
    }
}

/// The inside of the one expanded box (it is laid out large to hold it): the full title, the
/// pills, what is wrong and why, what it waits on and holds up (click a name to go there, ✕ to
/// remove the link), the start of its description, and the quick actions: a status, a new blocker
/// or blocked task, Open, What if it slips?, Still accurate.
#[component]
fn NodeCard(
    graph: DependencyGraph,
    node: GraphNode,
    expander: Expander,
    linking: RwSignal<Option<Linking>>,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let scenario = expect_context::<Scenario>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let gate = expect_context::<FinishDecision>();
    let navigate = use_navigate();
    let colours = use_objective_colours();
    let id = node.node.id;
    let target = node.node;
    let is_task = node.node.node_type == NodeType::Task;
    // The start of the description, from the item itself.
    let description = LocalResource::new(move || async move {
        if is_task {
            api::get_task(id).await.map(|t| t.description)
        } else {
            api::get_project(id).await.map(|p| p.description)
        }
    });
    let (waits, holds) = neighbours(&graph, id);
    let lines = inspector_lines(&graph, id)
        .into_iter()
        .map(|l| view! { <li>{l}</li> })
        .collect_view();
    let why = why_lines(&graph, id)
        .into_iter()
        .map(|l| view! { <li class="text-warning">{l}</li> })
        .collect_view();
    let picked_none = expander.picked;
    let list = move |title: &'static str, items: Vec<Neighbour>| {
        let total = items.len();
        let rows = items
            .into_iter()
            .take(5)
            .map(|n| {
                let (node_id, edge) = (n.node, n.edge);
                view! {
                    <li class="flex items-center gap-1.5">
                        <button class="min-w-0 truncate text-left hover:underline" title="Go to it"
                                on:click=move |_| expander.set(Some(node_id))>{n.label}</button>
                        <span class=if n.stuck { "shrink-0 text-warning" } else { "shrink-0 text-muted" }>{n.status}</span>
                        <button class="ml-auto shrink-0 rounded px-1 text-muted hover:bg-hover hover:text-danger"
                                title="Remove this link" aria-label="Remove this link"
                                on:click=move |_| remove_link(edge, picked_none, toasts, version)>"✕"</button>
                    </li>
                }
            })
            .collect_view();
        (total > 0).then(|| {
            view! {
                <div>
                    <h4 class="text-[10px] font-semibold uppercase tracking-wider text-muted">{format!("{title} ({total})")}</h4>
                    <ul class="mt-0.5 space-y-0.5">{rows}</ul>
                    {(total > 5).then(|| view! { <p class="text-[11px] text-muted">{format!("and {} more", total - 5)}</p> })}
                </div>
            }
        })
    };
    let waits_title = if is_task { "Waits on" } else { "Needs" };
    let holds_title = if is_task { "Holds up" } else { "Needed by" };
    let start = move |reverse: bool| {
        linking.set(Some(Linking {
            source: id,
            cursor: None,
            from_tray: true,
            reverse,
        }));
    };
    let (before_label, after_label) = if is_task {
        ("Add blocker…", "Add task it blocks…")
    } else {
        ("Needs…", "Needed by…")
    };
    let status_buttons = is_task.then(|| {
        let current = node.task_status;
        let buttons = status_choices()
            .into_iter()
            .map(|(s, label, _tone)| {
                view! {
                    <button class=format!("{BUTTON} {}", if current == Some(s) { BUTTON_ON } else { "" })
                            on:click=move |_| {
                                spawn_local(async move {
                                    finish_task(
                                        api::update_task(id, UpdateTask { status: Some(s), ..Default::default() }).await,
                                        id, toasts, version, gate,
                                    );
                                });
                            }>{label}</button>
                }
            })
            .collect_view();
        view! { <div class="flex flex-wrap gap-1" role="group" aria-label="Status">{buttons}</div> }
    });
    let objectives = colours.of_project(node.project_id);
    let priority = (node.priority <= 2).then(|| {
        view! { <span class=priority_pill(node.priority, node.done) title="Priority (1 is highest)">{format!("P{}", node.priority)}</span> }
    });
    let task_type = node.task_type.clone();
    let owner = node.assignee.as_ref().map(|a| a.label.clone());
    let navigate_what_if = navigate.clone();
    view! {
        <div class="flex h-full flex-col gap-2 overflow-y-auto p-3 text-[12px] text-fg"
             on:mousedown=|ev| ev.stop_propagation() on:click=|ev| ev.stop_propagation()
             on:dblclick=|ev| ev.stop_propagation() on:wheel=|ev| ev.stop_propagation()>
            <div class="flex items-start gap-2">
                <h3 class="min-w-0 flex-1 break-words text-[13px] font-semibold">{node.label.clone()}</h3>
                <button class="shrink-0 rounded px-1 text-muted hover:bg-hover" aria-label="Collapse"
                        title="Collapse" on:click=move |_| expander.set(None)>"⌃"</button>
            </div>
            <div class="flex flex-wrap items-center gap-1.5">
                <span class=Tone::Neutral.chip()>{status_word(&node)}</span>
                {priority}
                {is_task.then(|| view! { <TypeChip id=task_type.clone() /> })}
                <ObjectiveChips objectives=objectives />
                {owner.map(|o| view! { <span class="text-muted">{o}</span> })}
            </div>
            <ul class="space-y-0.5">{lines}</ul>
            <ul class="space-y-0.5">{why}</ul>
            {list(waits_title, waits)}
            {list(holds_title, holds)}
            {move || match description.get() {
                Some(Ok(text)) if !text.trim().is_empty() => Some(view! {
                    <p class="max-h-[3.9em] overflow-hidden whitespace-pre-line text-muted">{text}</p>
                }),
                _ => None,
            }}
            {status_buttons}
            <div class="flex flex-wrap items-center gap-1.5">
                <button class=BUTTON on:click=move |_| start(true)>{before_label}</button>
                <button class=BUTTON on:click=move |_| start(false)>{after_label}</button>
            </div>
            <div class="flex flex-wrap items-center gap-1.5">
                <button class=BUTTON_SOFT on:click=move |_| selection.open(target)>"Open"</button>
                <button class=BUTTON title="See what it would push back if it slipped"
                        on:click=move |_| {
                            scenario.start(target, DEFAULT_SLIP_DAYS);
                            navigate_what_if("/what-if", Default::default());
                        }>"What if it slips?"</button>
                <StillAccurate node=target />
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, Uuid, Zone};

    fn node(label: &str, f: impl Fn(&mut GraphNode)) -> GraphNode {
        let mut n = GraphNode {
            node: NodeRef::new(NodeType::Task, Uuid::nil()),
            label: label.into(),
            subtitle: String::new(),
            x: 0.0,
            y: 0.0,
            w: 210.0,
            h: 54.0,
            layer: 0,
            context: false,
            expanded: false,
            done: false,
            critical: false,
            late: false,
            blocked: false,
            held_up: false,
            task_status: None,
            project_status: None,
            priority: 3,
            assignee: None,
            project_id: None,
            finish: None,
            due: None,
            slack_days: None,
            late_by_days: None,
            estimate_days: None,
            task_type: None,
            health: None,
            zone: Zone::None,
            progress: None,
            upstream: 0,
            downstream: 0,
        };
        f(&mut n);
        n
    }

    fn graph(
        nodes: Vec<GraphNode>,
        critical: u32,
        hidden: u32,
        level: GraphLevel,
    ) -> DependencyGraph {
        DependencyGraph {
            level,
            nodes,
            edges: vec![],
            width: 100.0,
            height: 100.0,
            critical_nodes: critical,
            hidden_unlinked: hidden,
            unlinked: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn fit_centres_and_never_enlarges() {
        // Fits: shrink to the tighter dimension and centre the other.
        let (z, (px, py)) = fit(2000.0, 500.0, 1000.0, 600.0);
        assert!((z - 0.5).abs() < 1e-9);
        assert!((px - 0.0).abs() < 1e-9 && (py - 175.0).abs() < 1e-9);
        // Small drawings stay at 100% and are centred.
        let (z, (px, py)) = fit(400.0, 200.0, 1000.0, 600.0);
        assert_eq!(z, 1.0);
        assert_eq!((px, py), (300.0, 200.0));
        // Huge drawings stop at the minimum zoom; nonsense sizes are harmless.
        assert_eq!(fit(100_000.0, 100_000.0, 1000.0, 600.0).0, MIN_ZOOM);
        assert_eq!(fit(0.0, 0.0, 1000.0, 600.0), (1.0, (0.0, 0.0)));
    }

    #[test]
    fn only_the_ends_of_the_zoom_change_the_level() {
        use GraphLevel::{Projects, Tasks};
        let (min, max) = (MIN_ZOOM, MAX_ZOOM);
        // Tasks: out at the smallest scale goes to projects; anything else just zooms.
        assert_eq!(next_level(min, min, max, -0.2, Tasks), Some(Projects));
        assert_eq!(next_level(min + 0.01, min, max, -0.2, Tasks), None);
        assert_eq!(next_level(min, min, max, 0.2, Tasks), None);
        assert_eq!(next_level(max, min, max, 0.2, Tasks), None);
        // Projects: in at the largest scale goes to tasks.
        assert_eq!(next_level(max, min, max, 0.2, Projects), Some(Tasks));
        assert_eq!(next_level(max - 0.01, min, max, 0.2, Projects), None);
        assert_eq!(next_level(max, min, max, -0.2, Projects), None);
        assert_eq!(next_level(min, min, max, -0.2, Projects), None);
        // A zoom that has just reached the bound (zoom_at clamps) has not switched yet.
        let (z, _) = zoom_at(0.16, (0.0, 0.0), (0.0, 0.0), 0.5);
        assert_eq!(z, MIN_ZOOM);
        assert_eq!(next_level(0.16, min, max, -0.7, Tasks), None);
        assert_eq!(next_level(z, min, max, -0.7, Tasks), Some(Projects));
    }

    #[test]
    fn zooming_out_of_the_tasks_lands_on_the_project_nearest_the_middle() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let g = graph(
            vec![
                node("a1", |n| {
                    n.project_id = Some(a);
                    n.x = 0.0;
                }),
                node("b1", |n| {
                    n.project_id = Some(b);
                    n.x = 1000.0;
                }),
                // An inbox task is nobody's project.
                node("inbox", |n| n.x = 480.0),
            ],
            0,
            0,
            GraphLevel::Tasks,
        );
        assert_eq!(nearest_project(&g, (50.0, 20.0)), Some(a));
        assert_eq!(nearest_project(&g, (900.0, 20.0)), Some(b));
        assert_eq!(
            nearest_project(&graph(vec![], 0, 0, GraphLevel::Tasks), (0.0, 0.0)),
            None
        );
    }

    #[test]
    fn a_project_is_found_under_a_point_and_centred_in_the_view() {
        let p = Uuid::from_u128(7);
        let g = graph(
            vec![node("P", |n| {
                n.node = NodeRef::new(NodeType::Project, p);
                n.x = 100.0;
                n.y = 100.0;
                n.w = 200.0;
                n.h = 50.0;
            })],
            0,
            0,
            GraphLevel::Projects,
        );
        assert_eq!(project_at(&g, (150.0, 120.0)), Some(p));
        assert_eq!(project_at(&g, (99.0, 120.0)), None);
        assert_eq!(project_at(&g, (150.0, 151.0)), None);
        // At 2x the box's middle (200, 125) lands in the middle of a 1000x600 view.
        assert_eq!(centre_on(&g.nodes[0], 2.0, (1000.0, 600.0)), (100.0, 50.0));
    }

    #[test]
    fn zooming_keeps_the_point_under_the_cursor_still() {
        let (zoom, pan) = (1.0, (40.0, 20.0));
        let cursor = (300.0, 200.0);
        let (z2, pan2) = zoom_at(zoom, pan, cursor, 2.0);
        assert_eq!(z2, 2.0);
        // The drawing point under the cursor before and after is the same.
        let before = ((cursor.0 - pan.0) / zoom, (cursor.1 - pan.1) / zoom);
        let after = ((cursor.0 - pan2.0) / z2, (cursor.1 - pan2.1) / z2);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        // Limits.
        assert_eq!(zoom_at(2.9, pan, cursor, 2.0).0, MAX_ZOOM);
        assert_eq!(zoom_at(0.2, pan, cursor, 0.1).0, MIN_ZOOM);
    }

    #[test]
    fn a_point_of_the_view_maps_back_to_the_drawing() {
        // Undo what the pan and zoom do to the drawing.
        let (pan, zoom) = ((40.0, 20.0), 2.0);
        assert_eq!(to_drawing((140.0, 120.0), pan, zoom), (50.0, 50.0));
        assert_eq!(to_drawing((40.0, 20.0), pan, zoom), (0.0, 0.0));
        assert_eq!(to_drawing((10.0, 10.0), (0.0, 0.0), 1.0), (10.0, 10.0));
    }

    #[test]
    fn routes_become_smooth_paths() {
        assert_eq!(edge_path(&[]), "");
        assert_eq!(
            edge_path(&[(0.0, 0.0), (100.0, 40.0)]),
            "M0.0 0.0 C50.0 0.0 50.0 40.0 100.0 40.0"
        );
        let three = edge_path(&[(0.0, 0.0), (100.0, 0.0), (200.0, 50.0)]);
        assert_eq!(three.matches('C').count(), 2);
        assert!(three.ends_with("200.0 50.0"));
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis_and_short_text_is_kept() {
        assert_eq!(fit_text("Short", 200.0, 6.6), "Short");
        let cut = fit_text(
            "A very long task title that will not fit in the box",
            100.0,
            6.6,
        );
        assert!(cut.ends_with('…') && cut.chars().count() <= 15, "{cut}");
        assert_eq!(fit_text("", 100.0, 6.6), "");
        // Multi-byte text is cut on characters.
        assert!(fit_text("日本語のとても長いタイトルです", 40.0, 6.6).ends_with('…'));
        assert_eq!(fit_text("x", 1.0, 100.0), "x");
    }

    #[test]
    fn the_summary_counts_what_is_shown_and_what_is_not() {
        let g = graph(
            vec![
                node("a", |_| {}),
                node("b", |n| n.critical = true),
                node("ctx", |n| n.context = true),
            ],
            1,
            3,
            GraphLevel::Tasks,
        );
        assert_eq!(
            summary_text(&g),
            "2 tasks · 1 more for context · 1 on the critical path · 3 unlinked hidden"
        );
        let one = graph(vec![node("a", |_| {})], 0, 0, GraphLevel::Projects);
        assert_eq!(summary_text(&one), "1 project");
    }
}
