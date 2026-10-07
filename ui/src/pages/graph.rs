//! Dependencies: how work depends on other work, left to right. Tasks joined by `blocks`, or
//! projects joined by `depends_on`, narrowed by project, team or objective; the critical path is
//! highlighted. The layout is computed by the backend; this draws it as SVG with drag to pan and
//! wheel / buttons to zoom. Click a box to open it.

use leptos::{ev, prelude::*};
use minimap_types::{
    DependencyGraph, GraphEdge, GraphFilter, GraphLevel, GraphNode, NodeRef, NodeType, Uuid,
};

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_ON},
        page::{EmptyState, PageHeader, Tone, FILTER_BAR},
    },
    state::{DataVersion, Selection},
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

/// Classes of a box: its state, in the order the stylesheet expects.
pub fn node_class(n: &GraphNode) -> String {
    let mut c = String::from("node");
    for (on, name) in [
        (n.critical, "critical"),
        (n.late, "late"),
        (n.blocked, "blocked"),
        (n.done, "done"),
        (n.context, "context"),
    ] {
        if on {
            c.push(' ');
            c.push_str(name);
        }
    }
    c
}

// ------------------------------------------------------------------ the screen

fn parse_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}

#[component]
pub fn Graph() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let selection = expect_context::<Selection>();

    let level = RwSignal::new(GraphLevel::Tasks);
    let project = RwSignal::new(String::new());
    let team = RwSignal::new(String::new());
    let objective = RwSignal::new(String::new());
    let include_done = RwSignal::new(false);
    let include_isolated = RwSignal::new(false);

    let zoom = RwSignal::new(1.0_f64);
    let pan = RwSignal::new((0.0_f64, 0.0_f64));
    let drag = RwSignal::new(Option::<((f64, f64), (f64, f64))>::None);
    let needs_fit = RwSignal::new(true);
    let viewport = leptos::prelude::NodeRef::<leptos::html::Div>::new();

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
        })
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

    // A different filter or level frames the new drawing; edits keep your place.
    Effect::new(move |first: Option<()>| {
        level.track();
        project.track();
        team.track();
        objective.track();
        include_done.track();
        include_isolated.track();
        if first.is_some() {
            needs_fit.set(true);
        }
    });
    let fit_now = move || {
        if let (Some(el), Some(Ok(g))) = (viewport.get_untracked(), graph.get_untracked()) {
            let r = el.get_bounding_client_rect();
            let (z, p) = fit(g.width, g.height, r.width(), r.height());
            zoom.set(z);
            pan.set(p);
        }
    };
    Effect::new(move |_| {
        if graph.get().is_some() && needs_fit.get_untracked() {
            needs_fit.set(false);
            fit_now();
        }
    });

    let zoom_by = move |factor: f64| {
        if let Some(el) = viewport.get_untracked() {
            let r = el.get_bounding_client_rect();
            let (z, p) = zoom_at(
                zoom.get_untracked(),
                pan.get_untracked(),
                (r.width() / 2.0, r.height() / 2.0),
                factor,
            );
            zoom.set(z);
            pan.set(p);
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
            let (z, p) = zoom_at(
                zoom.get_untracked(),
                pan.get_untracked(),
                cursor,
                (-ev.delta_y() * 0.0015).exp(),
            );
            zoom.set(z);
            pan.set(p);
        }
    };
    let on_down = move |ev: ev::MouseEvent| {
        drag.set(Some((
            (f64::from(ev.client_x()), f64::from(ev.client_y())),
            pan.get_untracked(),
        )));
    };
    let on_move = move |ev: ev::MouseEvent| {
        if let Some((start, pan0)) = drag.get_untracked() {
            pan.set((
                pan0.0 + f64::from(ev.client_x()) - start.0,
                pan0.1 + f64::from(ev.client_y()) - start.1,
            ));
        }
    };
    let end_drag = move |_| drag.set(None);

    let level_button = move |l: GraphLevel, text: &'static str| {
        view! {
            <button class=move || format!("{BUTTON} {}", if level.get() == l { BUTTON_ON } else { "" })
                    on:click=move |_| { project.set(String::new()); level.set(l) }>{text}</button>
        }
    };
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
        match graph.get() {
        None => view! { <p class="p-6 text-muted">"Laying it out…"</p> }.into_any(),
        Some(Err(e)) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
        Some(Ok(g)) if g.nodes.is_empty() => view! {
            <EmptyState icon="graph" title="No dependencies to draw"
                hint="Link tasks with Links → Blocks (or projects with Depends on), then they appear here. Tasks and projects with no links are hidden unless you include them." />
        }.into_any(),
        Some(Ok(g)) => view! { <Drawing graph=g zoom=zoom pan=pan selection=selection /> }.into_any(),
    }
    };
    let summary = move || match graph.get() {
        Some(Ok(g)) => summary_text(&g),
        _ => String::new(),
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="graph" title="Dependencies"
                        subtitle="How work depends on other work, left to right">
                {level_button(GraphLevel::Tasks, "Tasks")}
                {level_button(GraphLevel::Projects, "Projects")}
                <span class="ml-auto flex items-center gap-1">
                    <button class=BUTTON aria-label="Zoom out" on:click=move |_| zoom_by(1.0 / 1.25)>"−"</button>
                    <span class="w-12 text-center tabular-nums text-muted">{move || format!("{}%", (zoom.get() * 100.0).round() as i64)}</span>
                    <button class=BUTTON aria-label="Zoom in" on:click=move |_| zoom_by(1.25)>"+"</button>
                    <button class=BUTTON on:click=move |_| fit_now()>"Fit"</button>
                </span>
            </PageHeader>
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
                    "Include unlinked"
                </label>
                <span class="ml-auto text-[11px] text-muted">{summary}</span>
            </div>
            <div node_ref=viewport class="relative min-h-0 flex-1 overflow-hidden"
                 on:mousedown=on_down on:mousemove=on_move on:mouseup=end_drag on:mouseleave=end_drag
                 on:wheel=on_wheel>
                {body}
                <div class="pointer-events-none absolute bottom-2 left-3 flex flex-wrap items-center gap-2 text-[11px] text-muted">
                    <span class=Tone::Accent.chip()>"critical path"</span>
                    <span class=Tone::Danger.chip()>"late"</span>
                    <span class=Tone::Warning.chip()>"blocked"</span>
                    <span class=Tone::Neutral.chip()>"dim = context"</span>
                    <span class=Tone::Neutral.chip()>"dashed frame = subtasks of a group"</span>
                    <span>"drag to pan · scroll to zoom · click to open"</span>
                </div>
            </div>
        </div>
    }
}

#[component]
fn Drawing(
    graph: DependencyGraph,
    zoom: RwSignal<f64>,
    pan: RwSignal<(f64, f64)>,
    selection: Selection,
) -> impl IntoView {
    let transform = move || {
        let (x, y) = pan.get();
        format!("translate({x:.1} {y:.1}) scale({:.4})", zoom.get())
    };
    let edges = graph
        .edges
        .iter()
        .map(|e: &GraphEdge| {
            let class = if e.critical { "edge critical" } else { "edge" };
            let marker = if e.critical { "url(#dep-arrow-hot)" } else { "url(#dep-arrow)" };
            let lag = e.lag_days.filter(|l| *l > 0).and_then(|l| {
                let mid = e.points.len() / 2;
                let (a, b) = (e.points.get(mid.saturating_sub(1))?, e.points.get(mid)?);
                Some(view! {
                    <text class="lag" x=(a.0 + b.0) / 2.0 y=(a.1 + b.1) / 2.0 - 4.0 text-anchor="middle">
                        {format!("+{l}d")}
                    </text>
                })
            });
            view! {
                <path d=edge_path(&e.points) class=class marker-end=marker />
                {lag}
            }
        })
        .collect_view();
    // A frame around the subtasks of a group; click it to open the group task.
    let groups = graph
        .groups
        .iter()
        .map(|g| {
            let target = NodeRef::new(NodeType::Task, g.id);
            let label = g.first.then(|| {
                view! {
                    <text class="group-label" x=g.x + 8.0 y=g.y + 13.0>
                        {fit_text(&g.label, g.w - 16.0, 6.0)}
                    </text>
                }
            });
            view! {
                <g class="group" on:mousedown=|ev| ev.stop_propagation()
                   on:click=move |_| selection.open(target)>
                    <title>{format!("Group: {} (its subtasks are inside)", g.label)}</title>
                    <rect class="frame" x=g.x y=g.y width=g.w height=g.h rx="6" />
                    {label}
                </g>
            }
        })
        .collect_view();
    let nodes = graph
        .nodes
        .iter()
        .map(|n| {
            let target = n.node;
            let class = node_class(n);
            view! {
                <g class=class transform=format!("translate({:.1} {:.1})", n.x, n.y)
                   on:mousedown=|ev| ev.stop_propagation()
                   on:click=move |_| selection.open(target)>
                    <rect class="box" width=n.w height=n.h rx="4" />
                    <text class="label" x="10" y="16">{fit_text(&n.label, n.w - 20.0, 6.6)}</text>
                    <text class="sub" x="10" y="30">{fit_text(&n.subtitle, n.w - 20.0, 5.6)}</text>
                </g>
            }
        })
        .collect_view();
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
            <g transform=transform>{groups}{edges}{nodes}</g>
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, Uuid};

    fn node(label: &str, f: impl Fn(&mut GraphNode)) -> GraphNode {
        let mut n = GraphNode {
            node: NodeRef::new(NodeType::Task, Uuid::nil()),
            label: label.into(),
            subtitle: String::new(),
            x: 0.0,
            y: 0.0,
            w: 210.0,
            h: 38.0,
            layer: 0,
            context: false,
            done: false,
            critical: false,
            late: false,
            blocked: false,
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
            groups: Vec::new(),
            level,
            nodes,
            edges: vec![],
            width: 100.0,
            height: 100.0,
            critical_nodes: critical,
            hidden_unlinked: hidden,
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

    #[test]
    fn box_classes_follow_the_state() {
        assert_eq!(node_class(&node("a", |_| {})), "node");
        assert_eq!(
            node_class(&node("a", |n| {
                n.critical = true;
                n.late = true
            })),
            "node critical late"
        );
        assert_eq!(
            node_class(&node("a", |n| {
                n.done = true;
                n.context = true
            })),
            "node done context"
        );
    }
}
