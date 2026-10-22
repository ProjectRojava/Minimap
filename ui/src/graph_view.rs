//! The Dependencies screen's logic (spec 47), pure so it is tested natively: what each lens says,
//! how a click traces a chain, what the inspector and the key say, which link a drag makes, and
//! the insights above the graph. `pages/graph.rs` draws the result.

use std::collections::{HashMap, HashSet};

use minimap_types::{
    Date, DependencyGraph, EdgeType, GraphEdge, GraphEdgeKind, GraphLevel, GraphNode,
    GraphUnlinked, HealthLevel, NewEdge, NodeRef, NodeType, TaskStatus, Uuid,
};

use crate::{
    components::{page::Tone, zone::zone_text},
    labels::{project_status_label, task_status_label, task_status_tone},
};

const EPS: f64 = 1e-9;

// ------------------------------------------------------------------ hints

/// The tooltip of a box: what a click and a double-click do.
pub const NODE_HINT: &str =
    "Click to expand it and trace it. Double-click to open it on the right.";

/// The last line of an open box's card.
pub const CARD_HINT: &str =
    "Click an empty part of this card to close it. Double-click to open it on the right.";

/// The two ends `(from, to)` of the link made by an unlinked item and a box, whether it was
/// clicked or dropped: forward, the item is the source (it blocks the box, or the project is
/// needed by it); reverse, the box is. `None` when they are the same item.
pub fn tray_pair(item: Uuid, target: Uuid, reverse: bool) -> Option<(Uuid, Uuid)> {
    let pair = if reverse {
        (target, item)
    } else {
        (item, target)
    };
    (item != target).then_some(pair)
}

/// The link made by dropping an unlinked `item` beside the one `waiting` on the graph, from where
/// it is dropped: the graph reads left to right, so what is on the left blocks what is on the
/// right (on the Projects level, is needed by it). Dropped to the left of the waiting box's
/// centre, the item comes first; to the right (or exactly on the centre), it comes after. `None`
/// when they are the same item.
pub fn drop_pair(
    item: Uuid,
    waiting: Uuid,
    drop_x: f64,
    waiting_centre_x: f64,
) -> Option<(Uuid, Uuid)> {
    let pair = if drop_x < waiting_centre_x {
        (item, waiting)
    } else {
        (waiting, item)
    };
    (item != waiting).then_some(pair)
}

/// The line over the graph while an unlinked item is dragged and another waits on the graph.
pub fn drop_beside_banner(waiting_name: &str) -> String {
    format!(
        "Drop it to the right of \u{201c}{waiting_name}\u{201d} to put it after, or to the left to put it before (Esc cancels)"
    )
}

/// What the waiting box says on its third line.
pub const WAITING_LINE: &str = "Not linked yet · drop another task beside it";

/// The tooltip of an unlinked card.
pub const UNLINKED_CARD_HINT: &str =
    "Drag it onto the graph. Drop a second one to its right (it comes after) or left (it comes before) to link them. Or click it, then a box. Double-click to open it on the right.";

/// The line along the bottom of the graph.
pub const LEGEND_HINT: &str = "drag to pan · scroll to zoom (out far enough for projects) · click a box to expand it · double-click to open it on the right · drag from a box's handle to link";

// ------------------------------------------------------------------ lenses

/// What the marks on the graph answer: one question at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lens {
    /// Will it finish on time? The critical path, slack and lateness.
    Route,
    /// What is stuck? Late, blocked and held-up work, and why.
    Risk,
    /// Who holds it up? The owner's colour, and what each box holds up.
    People,
}

impl Lens {
    pub const ALL: [Lens; 3] = [Lens::Route, Lens::Risk, Lens::People];

    pub fn label(self) -> &'static str {
        match self {
            Lens::Route => "Route",
            Lens::Risk => "Risk",
            Lens::People => "People",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Lens::Route => "Will it finish on time? The critical path, slack and lateness",
            Lens::Risk => "What is stuck? Late, blocked and held-up work, and why",
            Lens::People => "Who holds it up? Owners, and what each box holds up",
        }
    }
}

/// The key under the graph for a lens: a mark and what it means. What is wrong with the work
/// (late, blocked, held up) is marked in every lens; the lens adds its own.
pub fn legend(lens: Lens) -> Vec<(Tone, &'static str)> {
    let mut key = Vec::new();
    match lens {
        Lens::Route => key.push((Tone::Accent, "critical path")),
        Lens::Risk => {}
        Lens::People => key.push((Tone::Neutral, "colour = owner")),
    }
    key.extend([
        (Tone::Danger, "late"),
        (Tone::Warning, "blocked"),
        (Tone::Danger, "held up by a blocked task"),
        (Tone::Neutral, "dim = context"),
    ]);
    key
}

/// What the lines of the task graph mean, beside the key for the marks: the dependency is the
/// arrow; the links that only relate tasks have no arrow and no effect on the dates.
pub fn link_key(level: GraphLevel) -> Vec<(Tone, &'static str)> {
    match level {
        GraphLevel::Tasks => vec![
            (Tone::Neutral, "arrow = waits on"),
            (Tone::Neutral, "thin line = part of"),
            (Tone::Neutral, "dotted = related"),
        ],
        GraphLevel::Projects => Vec::new(),
    }
}

/// How much a box says at a zoom: everything, the title and its marks, or only the shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Full,
    Compact,
    Shape,
}

pub fn detail_level(zoom: f64) -> Detail {
    if zoom < 0.3 {
        Detail::Shape
    } else if zoom < 0.5 {
        Detail::Compact
    } else {
        Detail::Full
    }
}

// ------------------------------------------------------------------ box text

/// "Oct 16".
pub fn short_date(d: Date) -> String {
    let month = d.month().to_string();
    format!("{} {}", month.get(..3).unwrap_or(&month), d.day())
}

/// "1 day", "3 days", "0.5 days".
fn days(n: f64) -> String {
    let n = (n * 10.0).round() / 10.0;
    if (n - 1.0).abs() < EPS {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// "2d", "0.5d".
fn short_days(n: f64) -> String {
    format!("{}d", (n * 10.0).round() / 10.0)
}

fn count(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The second line of a box: status, owner and when.
pub fn line_two(n: &GraphNode) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(s) = n.task_status {
        parts.push(task_status_label(s).to_owned());
    }
    if let Some(s) = n.project_status {
        parts.push(project_status_label(s).to_owned());
    }
    if let Some(a) = &n.assignee {
        parts.push(a.label.clone());
    }
    match n.node.node_type {
        NodeType::Project => {
            if let Some((done, total)) = n.progress {
                parts.push(format!("{done}/{total} done"));
            }
        }
        _ => {
            if let Some(f) = n.finish {
                parts.push(format!("→ {}", short_date(f)));
            }
        }
    }
    parts.join(" · ")
}

/// The third line of a box: what the lens is about.
pub fn line_three(n: &GraphNode, lens: Lens) -> String {
    let is_project = n.node.node_type == NodeType::Project;
    let mut parts: Vec<String> = Vec::new();
    match lens {
        Lens::Route if is_project => {
            if n.held_up {
                parts.push("held up".to_owned());
            }
            if let Some(z) = zone_text(n.zone) {
                parts.push(z);
            }
            if let Some(due) = n.due {
                parts.push(format!("target {}", short_date(due)));
            }
        }
        Lens::Route => {
            if n.done {
                parts.push("done".to_owned());
            } else {
                if n.held_up {
                    parts.push("held up".to_owned());
                }
                match n.slack_days {
                    Some(s) if s < -EPS => parts.push(format!("{} over", short_days(-s))),
                    Some(s) if n.critical && s.abs() < EPS => {
                        parts.push("critical · no slack".to_owned())
                    }
                    Some(s) if s.abs() < EPS => parts.push("no slack".to_owned()),
                    Some(s) => parts.push(format!("slack {}", short_days(s))),
                    None => {}
                }
                if let Some(e) = n.estimate_days {
                    parts.push(format!("est {}", short_days(e)));
                }
            }
        }
        Lens::Risk => {
            if let Some(late) = n.late_by_days {
                parts.push(format!("late {late}d"));
            } else if n.late {
                parts.push("late".to_owned());
            }
            if n.blocked {
                parts.push(if is_project { "paused" } else { "blocked" }.to_owned());
            }
            if n.held_up {
                parts.push("held up".to_owned());
            }
            if is_project {
                match n.health {
                    Some(HealthLevel::Red) => parts.push("health red".to_owned()),
                    Some(HealthLevel::Amber) => parts.push("health amber".to_owned()),
                    _ => {}
                }
            }
            if parts.is_empty() && !n.done {
                parts.push("no problems".to_owned());
            }
        }
        Lens::People => {
            if n.downstream > 0 {
                parts.push(format!("holds up {}", n.downstream));
            }
            if n.upstream > 0 {
                parts.push(format!("waits on {}", n.upstream));
            }
        }
    }
    parts.join(" · ")
}

/// The words on an arrow: a task link's lag ("+2d"), or the room between two projects.
pub fn edge_label(e: &GraphEdge, level: GraphLevel) -> Option<String> {
    match level {
        GraphLevel::Tasks => e.lag_days.filter(|l| *l > 0).map(|l| format!("+{l}d")),
        GraphLevel::Projects => e.room_days.map(|r| match r {
            0 => "no room".to_owned(),
            r => format!("{r}d room"),
        }),
    }
}

// ------------------------------------------------------------------ classes

/// What a trace lights: a node and the chain on both sides of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    pub nodes: HashSet<Uuid>,
    pub edges: HashSet<Uuid>,
    /// What it waits on, and what it holds up (the other nodes only).
    pub up: HashSet<Uuid>,
    pub down: HashSet<Uuid>,
}

fn walk(start: Uuid, next: &HashMap<Uuid, Vec<Uuid>>) -> HashSet<Uuid> {
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        for &n in next.get(&id).into_iter().flatten() {
            if n != start && seen.insert(n) {
                stack.push(n);
            }
        }
    }
    seen
}

/// Everything a node waits on and everything it holds up, through any number of links, and the
/// links along those chains.
pub fn trace_node(g: &DependencyGraph, id: Uuid) -> Trace {
    // The chain is the dependencies only; related and part-of links just add the box's own
    // neighbours (one step), which wait for nothing and hold nothing up.
    let mut forward: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut backward: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in g.edges.iter().filter(|e| e.kind.orders_work()) {
        forward.entry(e.from).or_default().push(e.to);
        backward.entry(e.to).or_default().push(e.from);
    }
    let down = walk(id, &forward);
    let up = walk(id, &backward);
    let mut nodes: HashSet<Uuid> = up.union(&down).copied().collect();
    nodes.insert(id);
    let mut edges: HashSet<Uuid> = g
        .edges
        .iter()
        .filter(|e| e.kind.orders_work())
        .filter(|e| {
            let upstream = up.contains(&e.from) && (up.contains(&e.to) || e.to == id);
            let downstream = down.contains(&e.to) && (down.contains(&e.from) || e.from == id);
            upstream || downstream
        })
        .map(|e| e.id)
        .collect();
    for e in g
        .edges
        .iter()
        .filter(|e| !e.kind.orders_work() && (e.from == id || e.to == id))
    {
        edges.insert(e.id);
        nodes.insert(e.from);
        nodes.insert(e.to);
    }
    Trace {
        nodes,
        edges,
        up,
        down,
    }
}

/// A link and the two boxes it joins.
pub fn trace_edge(g: &DependencyGraph, edge: Uuid) -> Trace {
    let Some(e) = g.edges.iter().find(|e| e.id == edge) else {
        return Trace::default();
    };
    Trace {
        nodes: HashSet::from([e.from, e.to]),
        edges: HashSet::from([e.id]),
        up: HashSet::new(),
        down: HashSet::new(),
    }
}

/// The classes of a box: what is wrong with the work (late, blocked, held up) in every lens, what
/// the lens adds (the critical path on Route), then what is always true of it.
pub fn node_class(
    n: &GraphNode,
    lens: Lens,
    trace: Option<&Trace>,
    selected: Option<Uuid>,
) -> String {
    let mut c = String::from("node");
    let marks: [(bool, &str); 4] = [
        (lens == Lens::Route && n.critical, "critical"),
        (n.late, "late"),
        (n.blocked, "blocked"),
        (n.held_up, "held-up"),
    ];
    for (on, name) in marks {
        if on {
            c.push(' ');
            c.push_str(name);
        }
    }
    for (on, name) in [
        (n.done, "done"),
        (n.context, "context"),
        (n.expanded, "expanded"),
        (selected == Some(n.node.id), "selected"),
        (trace.is_some_and(|t| !t.nodes.contains(&n.node.id)), "dim"),
    ] {
        if on {
            c.push(' ');
            c.push_str(name);
        }
    }
    c
}

/// The classes of an arrow.
pub fn edge_class(
    e: &GraphEdge,
    lens: Lens,
    from: Option<&GraphNode>,
    trace: Option<&Trace>,
    selected: Option<Uuid>,
) -> String {
    let mut c = String::from("edge");
    if lens == Lens::Route && e.critical {
        c.push_str(" critical");
    }
    if lens == Lens::Risk && from.is_some_and(|f| f.blocked || f.late) {
        c.push_str(" risk");
    }
    if e.cross_project {
        c.push_str(" cross");
    }
    match e.kind {
        GraphEdgeKind::Blocks => {}
        GraphEdgeKind::RelatesTo => c.push_str(" related"),
        GraphEdgeKind::Subtask => c.push_str(" part"),
    }
    if selected == Some(e.id) {
        c.push_str(" selected");
    }
    if trace.is_some_and(|t| !t.edges.contains(&e.id)) {
        c.push_str(" dim");
    }
    c
}

// ------------------------------------------------------------------ inspector

fn label_of(g: &DependencyGraph, id: Uuid) -> String {
    g.nodes
        .iter()
        .find(|n| n.node.id == id)
        .map_or_else(|| "?".to_owned(), |n| n.label.clone())
}

/// What the inspector says about a box, one sentence a line.
pub fn inspector_lines(g: &DependencyGraph, id: Uuid) -> Vec<String> {
    let Some(n) = g.nodes.iter().find(|n| n.node.id == id) else {
        return Vec::new();
    };
    let trace = trace_node(g, id);
    let is_project = n.node.node_type == NodeType::Project;
    let thing = if is_project { "project" } else { "task" };
    let mut lines = Vec::new();
    let down = trace.down.len() as u32;
    if down == 0 {
        lines.push("Holds nothing up.".to_owned());
    } else {
        let projects: HashSet<Uuid> = g
            .nodes
            .iter()
            .filter(|m| trace.down.contains(&m.node.id))
            .filter_map(|m| m.project_id)
            .collect();
        let plural = |n: u32| count(n, thing, &format!("{thing}s"));
        if is_project || projects.len() < 2 {
            lines.push(format!("Holds up {}.", plural(down)));
        } else {
            lines.push(format!(
                "Holds up {} in {}.",
                plural(down),
                count(projects.len() as u32, "project", "projects")
            ));
        }
    }
    let up = trace.up.len() as u32;
    lines.push(if up == 0 {
        "Waits on nothing.".to_owned()
    } else {
        format!("Waits on {}.", count(up, thing, &format!("{thing}s")))
    });
    match (n.finish, n.due) {
        (Some(f), Some(d)) => match n.late_by_days {
            Some(late) => lines.push(format!(
                "Finishes {}, due {}: {} late.",
                short_date(f),
                short_date(d),
                count(late, "working day", "working days")
            )),
            None => lines.push(format!(
                "Finishes {}, due {}: on time.",
                short_date(f),
                short_date(d)
            )),
        },
        (Some(f), None) => lines.push(format!("Finishes {}.", short_date(f))),
        _ => {}
    }
    if is_project {
        if let Some(z) = zone_text(n.zone) {
            lines.push(format!("{z}."));
        }
        if let Some((done, total)) = n.progress {
            lines.push(format!("{done} of {total} tasks done."));
        }
        match n.health {
            Some(HealthLevel::Red) => lines.push("Health is red.".to_owned()),
            Some(HealthLevel::Amber) => lines.push("Health is amber.".to_owned()),
            _ => {}
        }
    } else {
        if n.critical {
            lines.push("On the critical path: any slip moves the finish.".to_owned());
        } else if let Some(s) = n.slack_days.filter(|s| *s > EPS) {
            lines.push(format!("{} of slack.", days(s)));
        }
        if n.blocked {
            lines.push("Blocked.".to_owned());
        } else if n.held_up {
            lines.push("Held up by a blocked task.".to_owned());
        }
    }
    lines
}

/// What a link means, in a sentence.
pub fn edge_sentence(g: &DependencyGraph, edge: Uuid) -> String {
    let Some(e) = g.edges.iter().find(|e| e.id == edge) else {
        return String::new();
    };
    let (from, to) = (label_of(g, e.from), label_of(g, e.to));
    match g.level {
        GraphLevel::Tasks if e.kind == GraphEdgeKind::RelatesTo => {
            format!("{from} is related to {to}. Neither task waits for the other.")
        }
        GraphLevel::Tasks if e.kind == GraphEdgeKind::Subtask => {
            let (parent, sub) = if e.reversed { (to, from) } else { (from, to) };
            format!("{sub} is part of {parent}. Nothing waits and no date moves.")
        }
        GraphLevel::Tasks => {
            let lag = e
                .lag_days
                .filter(|l| *l > 0)
                .map(|l| format!(", plus {}", count(l, "day", "days")))
                .unwrap_or_default();
            let crossing = if e.cross_project {
                " This link crosses projects."
            } else {
                ""
            };
            format!(
                "{from} blocks {to}: {to} can't start until {from} has finished{lag}.{crossing}"
            )
        }
        GraphLevel::Projects => {
            let room = match e.room_days {
                Some(0) => " There is no room between them.".to_owned(),
                Some(r) => format!(
                    " There are {} between them.",
                    count(r, "working day of room", "working days of room")
                ),
                None => String::new(),
            };
            format!("{to} depends on {from}: it can't start until {from} has finished.{room}")
        }
    }
}

/// The link a drag from `source`'s handle onto `target` makes: tasks, `source` blocks `target`;
/// projects, `source` is needed by `target` (`target` depends on `source`).
pub fn new_link(level: GraphLevel, source: Uuid, target: Uuid) -> NewEdge {
    match level {
        GraphLevel::Tasks => NewEdge {
            edge_type: EdgeType::Blocks,
            from: NodeRef::new(NodeType::Task, source),
            to: NodeRef::new(NodeType::Task, target),
            attrs: serde_json::json!({}),
        },
        GraphLevel::Projects => NewEdge {
            edge_type: EdgeType::DependsOn,
            from: NodeRef::new(NodeType::Project, target),
            to: NodeRef::new(NodeType::Project, source),
            attrs: serde_json::json!({}),
        },
    }
}

/// What a made link says in the toast.
pub fn link_message(level: GraphLevel, source: &str, target: &str) -> String {
    match level {
        GraphLevel::Tasks => format!("Linked: {source} blocks {target}. Press Ctrl/Cmd+Z to undo."),
        GraphLevel::Projects => {
            format!("Linked: {target} depends on {source}. Press Ctrl/Cmd+Z to undo.")
        }
    }
}

/// The attributes for a lag of `days` working days (none means no lag).
pub fn lag_attrs(days: u32) -> serde_json::Value {
    if days == 0 {
        serde_json::json!({})
    } else {
        serde_json::json!({ "lag_days": days })
    }
}

// ------------------------------------------------------------------ the expanded box

/// A box on the other end of a link, for the lists in an expanded box.
#[derive(Clone, Debug, PartialEq)]
pub struct Neighbour {
    pub edge: Uuid,
    pub node: Uuid,
    pub label: String,
    /// Its status in words ("Blocked", "In progress").
    pub status: String,
    /// Blocked, late or held up: worth a glance.
    pub stuck: bool,
    /// How it is linked, when the link doesn't order the work ("Related", "Part of",
    /// "Sub-task"); empty for a box waited on or held up.
    pub how: &'static str,
}

/// A box's status in words: a task's or a project's.
pub fn status_word(n: &GraphNode) -> String {
    match (n.task_status, n.project_status) {
        (Some(s), _) => task_status_label(s).to_owned(),
        (None, Some(s)) => project_status_label(s).to_owned(),
        _ => String::new(),
    }
}

/// The box on the other end of a link, by name.
fn neighbour(
    g: &DependencyGraph,
    e: &GraphEdge,
    other: Uuid,
    how: &'static str,
) -> Option<Neighbour> {
    g.nodes
        .iter()
        .find(|n| n.node.id == other)
        .map(|n| Neighbour {
            edge: e.id,
            node: other,
            label: n.label.clone(),
            status: status_word(n),
            stuck: n.blocked || n.late || n.held_up,
            how,
        })
}

fn by_label(list: &mut [Neighbour]) {
    list.sort_by(|a, b| {
        a.label
            .to_lowercase()
            .cmp(&b.label.to_lowercase())
            .then(a.node.cmp(&b.node))
    });
}

/// What a box waits on and what it holds up, directly, by name (the dependencies only).
pub fn neighbours(g: &DependencyGraph, id: Uuid) -> (Vec<Neighbour>, Vec<Neighbour>) {
    let mut waits: Vec<Neighbour> = g
        .edges
        .iter()
        .filter(|e| e.kind.orders_work() && e.to == id)
        .filter_map(|e| neighbour(g, e, e.from, ""))
        .collect();
    let mut holds: Vec<Neighbour> = g
        .edges
        .iter()
        .filter(|e| e.kind.orders_work() && e.from == id)
        .filter_map(|e| neighbour(g, e, e.to, ""))
        .collect();
    by_label(&mut waits);
    by_label(&mut holds);
    (waits, holds)
}

/// The tasks linked to a box without ordering the work: related ones, its parent and its
/// sub-tasks, by name.
pub fn also_linked(g: &DependencyGraph, id: Uuid) -> Vec<Neighbour> {
    let mut list: Vec<Neighbour> = g
        .edges
        .iter()
        .filter(|e| !e.kind.orders_work() && (e.from == id || e.to == id))
        .filter_map(|e| {
            let other = if e.from == id { e.to } else { e.from };
            let how = match e.kind {
                GraphEdgeKind::RelatesTo => "Related",
                GraphEdgeKind::Subtask => {
                    // The parent end: `from`, unless the link is drawn the other way round.
                    let parent = if e.reversed { e.to } else { e.from };
                    if parent == id {
                        "Sub-task"
                    } else {
                        "Part of"
                    }
                }
                GraphEdgeKind::Blocks => return None,
            };
            neighbour(g, e, other, how)
        })
        .collect();
    by_label(&mut list);
    list
}

/// "“A”", "“A” and “B”", "“A”, “B” and 2 more".
fn names(labels: &[String]) -> String {
    let quote = |l: &String| format!("“{l}”");
    match labels {
        [] => String::new(),
        [one] => quote(one),
        [a, b] => format!("{} and {}", quote(a), quote(b)),
        [a, b, rest @ ..] => format!("{}, {} and {} more", quote(a), quote(b), rest.len()),
    }
}

/// Why a box is marked, in words: what blocks it, what holds it up, how late it is.
pub fn why_lines(g: &DependencyGraph, id: Uuid) -> Vec<String> {
    let Some(n) = g.nodes.iter().find(|n| n.node.id == id) else {
        return Vec::new();
    };
    let is_project = n.node.node_type == NodeType::Project;
    let mut lines = Vec::new();
    if n.blocked {
        lines.push(if is_project {
            "Paused.".to_owned()
        } else {
            "Blocked: its status is Blocked.".to_owned()
        });
    }
    if n.held_up {
        let trace = trace_node(g, id);
        let mut roots: Vec<String> = g
            .nodes
            .iter()
            .filter(|m| trace.up.contains(&m.node.id) && m.blocked && !m.done)
            .map(|m| m.label.clone())
            .collect();
        roots.sort_by_key(|l| l.to_lowercase());
        if roots.is_empty() {
            lines.push("Held up by a blocked task.".to_owned());
        } else {
            lines.push(format!(
                "Held up by {}, which {} blocked.",
                names(&roots),
                if roots.len() == 1 { "is" } else { "are" }
            ));
        }
    }
    if let Some(late) = n.late_by_days {
        lines.push(if is_project {
            format!(
                "Projected {} past its target.",
                count(late, "working day", "working days")
            )
        } else {
            format!("Late by {}.", count(late, "working day", "working days"))
        });
    }
    lines
}

/// The status buttons of a task's expanded box: the status, its words and its tone.
pub fn status_choices() -> [(TaskStatus, &'static str, Tone); 4] {
    [
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::Done,
    ]
    .map(|s| (s, task_status_label(s), task_status_tone(s)))
}

/// The pan that keeps a box where it was on screen after the layout moved it from `old` to `new`
/// (both in the graph's own pixels).
pub fn keep_in_place(pan: (f64, f64), zoom: f64, old: (f64, f64), new: (f64, f64)) -> (f64, f64) {
    (
        pan.0 + (old.0 - new.0) * zoom,
        pan.1 + (old.1 - new.1) * zoom,
    )
}

/// The point halfway along a link's route (by length), in the graph's own pixels.
pub fn midpoint(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let length = |a: (f64, f64), b: (f64, f64)| (b.0 - a.0).hypot(b.1 - a.1);
    let total: f64 = points.windows(2).map(|w| length(w[0], w[1])).sum();
    let mut left = total / 2.0;
    for w in points.windows(2) {
        let seg = length(w[0], w[1]);
        if left <= seg && seg > 0.0 {
            let t = left / seg;
            return Some((
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            ));
        }
        left -= seg;
    }
    points.last().copied()
}

// ------------------------------------------------------------------ insights

/// A line above the graph, and the box it is about (clicking it selects that box).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Insight {
    pub text: String,
    pub node: Option<Uuid>,
}

/// What stands out: the biggest bottleneck, links across projects, what is blocked or late.
pub fn insights(g: &DependencyGraph) -> Vec<Insight> {
    let shown = || g.nodes.iter().filter(|n| !n.context);
    let mut out = Vec::new();
    let top = shown()
        .filter(|n| !n.done && n.downstream >= 2)
        .max_by(|a, b| {
            a.downstream
                .cmp(&b.downstream)
                .then_with(|| b.label.cmp(&a.label))
        });
    if let Some(n) = top {
        let verb = match g.level {
            GraphLevel::Tasks => "Biggest bottleneck",
            GraphLevel::Projects => "Most depended on",
        };
        out.push(Insight {
            text: format!("{verb}: {} holds up {}", n.label, n.downstream),
            node: Some(n.node.id),
        });
    }
    let crossing = g.edges.iter().filter(|e| e.cross_project).count() as u32;
    if g.level == GraphLevel::Tasks && crossing > 0 {
        out.push(Insight {
            text: if crossing == 1 {
                "1 link crosses projects".to_owned()
            } else {
                format!("{crossing} links cross projects")
            },
            node: None,
        });
    }
    let blocked = shown().filter(|n| n.blocked && !n.done).count() as u32;
    if blocked > 0 {
        let word = if g.level == GraphLevel::Tasks {
            "blocked"
        } else {
            "paused"
        };
        out.push(Insight {
            text: format!("{blocked} {word}"),
            node: shown().find(|n| n.blocked && !n.done).map(|n| n.node.id),
        });
    }
    let late = shown().filter(|n| n.late && !n.done).count() as u32;
    if late > 0 {
        out.push(Insight {
            text: format!("{late} late"),
            node: shown().find(|n| n.late && !n.done).map(|n| n.node.id),
        });
    }
    out
}

// ------------------------------------------------------------------ the unlinked list

/// What the unlinked list is narrowed to. Empty text and empty choices match everything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnlinkedFilter {
    /// Every word must be in the name, the project line, the owner or the status.
    pub text: String,
    /// A status word's id (`todo`, `blocked`...), or empty.
    pub status: String,
    /// A project's id, `none` for work in no project, or empty.
    pub project: String,
    /// A person's id, `none` for work nobody has, or empty.
    pub owner: String,
}

impl UnlinkedFilter {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The status id of an unlinked box (a task's or a project's).
pub fn status_id(n: &GraphNode) -> String {
    match (n.task_status, n.project_status) {
        (Some(s), _) => s.as_str().to_owned(),
        (None, Some(s)) => s.as_str().to_owned(),
        _ => String::new(),
    }
}

/// Whether one unlinked entry passes the filter.
pub fn unlinked_matches(item: &GraphUnlinked, f: &UnlinkedFilter) -> bool {
    let look = &item.look;
    if !f.status.is_empty() && status_id(look) != f.status {
        return false;
    }
    match f.project.as_str() {
        "" => {}
        "none" => {
            if look.project_id.is_some() {
                return false;
            }
        }
        id => {
            if look.project_id.map(|p| p.to_string()).as_deref() != Some(id) {
                return false;
            }
        }
    }
    match f.owner.as_str() {
        "" => {}
        "none" => {
            if look.assignee.is_some() {
                return false;
            }
        }
        id => {
            if look
                .assignee
                .as_ref()
                .map(|a| a.node.id.to_string())
                .as_deref()
                != Some(id)
            {
                return false;
            }
        }
    }
    let hay = format!(
        "{} {} {} {}",
        item.label,
        item.subtitle,
        look.assignee.as_ref().map_or("", |a| a.label.as_str()),
        status_word(look)
    )
    .to_lowercase();
    f.text
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

/// The entries that pass the filter, in their order (by name).
pub fn filter_unlinked<'a>(
    items: &'a [GraphUnlinked],
    f: &UnlinkedFilter,
) -> Vec<&'a GraphUnlinked> {
    items.iter().filter(|i| unlinked_matches(i, f)).collect()
}

/// The choices a filter box offers, from what is actually in the list: `(value, label)`, the
/// "any" choice first.
pub fn status_choices_of(items: &[GraphUnlinked]) -> Vec<(String, String)> {
    let mut seen: Vec<(String, String)> = Vec::new();
    for i in items {
        let id = status_id(&i.look);
        if !id.is_empty() && !seen.iter().any(|(v, _)| *v == id) {
            seen.push((id, status_word(&i.look)));
        }
    }
    seen.sort_by(|a, b| a.1.cmp(&b.1));
    std::iter::once((String::new(), "Any status".to_owned()))
        .chain(seen)
        .collect()
}

/// People who own something in the list, then "Unassigned" when some of it has no owner.
pub fn owner_choices_of(items: &[GraphUnlinked]) -> Vec<(String, String)> {
    let mut people: Vec<(String, String)> = Vec::new();
    for i in items {
        if let Some(a) = &i.look.assignee {
            let id = a.node.id.to_string();
            if !people.iter().any(|(v, _)| *v == id) {
                people.push((id, a.label.clone()));
            }
        }
    }
    people.sort_by_key(|p| p.1.to_lowercase());
    let mut out = vec![(String::new(), "Anyone".to_owned())];
    out.extend(people);
    if items.iter().any(|i| i.look.assignee.is_none()) {
        out.push(("none".to_owned(), "Unassigned".to_owned()));
    }
    out
}

/// Projects that hold something in the list (named by `name_of`), then "No project" when some of
/// it is in none.
pub fn project_choices_of(
    items: &[GraphUnlinked],
    name_of: &dyn Fn(Uuid) -> Option<String>,
) -> Vec<(String, String)> {
    let mut projects: Vec<(String, String)> = Vec::new();
    for i in items {
        if let Some(p) = i.look.project_id {
            let id = p.to_string();
            if !projects.iter().any(|(v, _)| *v == id) {
                projects.push((id, name_of(p).unwrap_or_else(|| "A project".to_owned())));
            }
        }
    }
    projects.sort_by_key(|p| p.1.to_lowercase());
    let mut out = vec![(String::new(), "Any project".to_owned())];
    out.extend(projects);
    if items.iter().any(|i| i.look.project_id.is_none()) {
        out.push(("none".to_owned(), "No project".to_owned()));
    }
    out
}

/// What the list's header says: how many there are, and how many pass the filter.
pub fn unlinked_count_text(total: u32, shown: usize, filtered: bool) -> String {
    if filtered {
        format!("Unlinked ({shown} of {total})")
    } else {
        format!("Unlinked ({total})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeSummary, ProjectStatus, TaskStatus, Zone, ZoneLevel};
    use time::macros::date;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn node(n: u128, label: &str) -> GraphNode {
        GraphNode {
            node: NodeRef::new(NodeType::Task, id(n)),
            label: label.into(),
            subtitle: String::new(),
            x: 0.0,
            y: 0.0,
            w: 210.0,
            h: 54.0,
            layer: 0,
            context: false,
            expanded: false,
            waiting: false,
            done: false,
            critical: false,
            late: false,
            blocked: false,
            held_up: false,
            task_status: Some(TaskStatus::Todo),
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
        }
    }

    fn edge(n: u128, from: u128, to: u128) -> GraphEdge {
        GraphEdge {
            id: id(1000 + n),
            from: id(from),
            to: id(to),
            lag_days: None,
            critical: false,
            points: Vec::new(),
            cross_project: false,
            room_days: None,
            kind: GraphEdgeKind::Blocks,
            reversed: false,
        }
    }

    fn linked(n: u128, from: u128, to: u128, kind: GraphEdgeKind) -> GraphEdge {
        GraphEdge {
            kind,
            ..edge(n, from, to)
        }
    }

    fn graph(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>, level: GraphLevel) -> DependencyGraph {
        DependencyGraph {
            level,
            nodes,
            edges,
            width: 100.0,
            height: 100.0,
            critical_nodes: 0,
            hidden_unlinked: 0,
            unlinked: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// A -> B -> D, A -> C -> D, D -> E, and an island X -> Y.
    fn diamond() -> DependencyGraph {
        graph(
            vec![
                node(1, "A"),
                node(2, "B"),
                node(3, "C"),
                node(4, "D"),
                node(5, "E"),
                node(8, "X"),
                node(9, "Y"),
            ],
            vec![
                edge(1, 1, 2),
                edge(2, 1, 3),
                edge(3, 2, 4),
                edge(4, 3, 4),
                edge(5, 4, 5),
                edge(6, 8, 9),
            ],
            GraphLevel::Tasks,
        )
    }

    #[test]
    fn tracing_a_box_lights_what_it_waits_on_and_what_it_holds_up_and_nothing_else() {
        let g = diamond();
        let t = trace_node(&g, id(4));
        // D waits on A, B, C and holds up E.
        assert_eq!(t.up, HashSet::from([id(1), id(2), id(3)]));
        assert_eq!(t.down, HashSet::from([id(5)]));
        assert_eq!(t.nodes.len(), 5);
        assert!(!t.nodes.contains(&id(8)) && !t.nodes.contains(&id(9)));
        // Every link on those chains, not the island's.
        assert_eq!(t.edges.len(), 5);
        assert!(!t.edges.contains(&id(1006)));
        // From the middle of a branch: B waits on A and holds up D and E; C is not on its chain.
        let b = trace_node(&g, id(2));
        assert_eq!(b.up, HashSet::from([id(1)]));
        assert_eq!(b.down, HashSet::from([id(4), id(5)]));
        assert!(!b.nodes.contains(&id(3)));
        assert!(!b.edges.contains(&id(1002)) && !b.edges.contains(&id(1004)));
        // A box on its own traces to itself.
        let lone = graph(vec![node(1, "A")], vec![], GraphLevel::Tasks);
        assert_eq!(trace_node(&lone, id(1)).nodes, HashSet::from([id(1)]));
    }

    #[test]
    fn tracing_a_link_lights_it_and_its_two_ends() {
        let g = diamond();
        let t = trace_edge(&g, id(1003));
        assert_eq!(t.nodes, HashSet::from([id(2), id(4)]));
        assert_eq!(t.edges, HashSet::from([id(1003)]));
        assert!(trace_edge(&g, id(1)).nodes.is_empty());
    }

    #[test]
    fn a_lens_asks_for_its_own_marks_and_a_trace_dims_the_rest() {
        let mut n = node(1, "A");
        n.critical = true;
        n.late = true;
        n.blocked = true;
        n.held_up = true;
        // Blocked, late and held-up work is marked in every lens; the critical path only on Route.
        assert_eq!(
            node_class(&n, Lens::Route, None, None),
            "node critical late blocked held-up"
        );
        assert_eq!(
            node_class(&n, Lens::Risk, None, None),
            "node late blocked held-up"
        );
        assert_eq!(
            node_class(&n, Lens::People, None, None),
            "node late blocked held-up"
        );
        // A healthy box has no marks in any lens.
        let fine = node(7, "Fine");
        for lens in Lens::ALL {
            assert_eq!(node_class(&fine, lens, None, None), "node");
        }
        n.done = true;
        n.context = true;
        assert_eq!(
            node_class(&n, Lens::People, None, None),
            "node late blocked held-up done context"
        );
        // Selected, and dimmed when it is not on the trace.
        let trace = Trace::default();
        let plain = node(2, "B");
        assert_eq!(
            node_class(&plain, Lens::Route, Some(&trace), Some(id(2))),
            "node selected dim"
        );
        let on = Trace {
            nodes: HashSet::from([id(2)]),
            ..Trace::default()
        };
        assert_eq!(node_class(&plain, Lens::Route, Some(&on), None), "node");
    }

    #[test]
    fn arrows_follow_the_lens_and_say_when_they_cross_projects() {
        let mut e = edge(1, 1, 2);
        e.critical = true;
        e.cross_project = true;
        let mut blocked = node(1, "A");
        blocked.blocked = true;
        assert_eq!(
            edge_class(&e, Lens::Route, None, None, None),
            "edge critical cross"
        );
        assert_eq!(
            edge_class(&e, Lens::Risk, Some(&blocked), None, None),
            "edge risk cross"
        );
        assert_eq!(
            edge_class(&e, Lens::People, None, None, Some(e.id)),
            "edge cross selected"
        );
        let off = Trace::default();
        assert!(edge_class(&e, Lens::People, None, Some(&off), None).ends_with(" dim"));
    }

    #[test]
    fn the_second_line_says_status_owner_and_when_and_the_third_what_the_lens_is_about() {
        let mut n = node(1, "A");
        n.task_status = Some(TaskStatus::InProgress);
        n.assignee = Some(NodeSummary {
            node: NodeRef::new(NodeType::Person, id(50)),
            label: "Priya Nair".into(),
            archived: false,
        });
        n.finish = Some(date!(2027 - 03 - 05));
        assert_eq!(line_two(&n), "In progress · Priya Nair · → Mar 5");
        // Route: slack and estimate.
        n.slack_days = Some(2.0);
        n.estimate_days = Some(3.0);
        assert_eq!(line_three(&n, Lens::Route), "slack 2d · est 3d");
        // Held up by something blocked, in words as well as colour.
        n.held_up = true;
        assert_eq!(line_three(&n, Lens::Route), "held up · slack 2d · est 3d");
        n.held_up = false;
        n.critical = true;
        n.slack_days = Some(0.0);
        assert_eq!(line_three(&n, Lens::Route), "critical · no slack · est 3d");
        n.slack_days = Some(-1.5);
        assert_eq!(line_three(&n, Lens::Route), "1.5d over · est 3d");
        n.done = true;
        assert_eq!(line_three(&n, Lens::Route), "done");
        // Risk: why.
        let mut r = node(2, "B");
        assert_eq!(line_three(&r, Lens::Risk), "no problems");
        r.late = true;
        r.late_by_days = Some(2);
        r.blocked = true;
        r.held_up = true;
        assert_eq!(line_three(&r, Lens::Risk), "late 2d · blocked · held up");
        // People: what it holds up.
        r.downstream = 5;
        r.upstream = 1;
        assert_eq!(line_three(&r, Lens::People), "holds up 5 · waits on 1");
        // Nothing to say for a box with no owner or date.
        assert_eq!(line_two(&node(3, "C")), "To do");
    }

    #[test]
    fn a_project_box_says_progress_zone_and_health() {
        let mut p = node(1, "Platform");
        p.node = NodeRef::new(NodeType::Project, id(1));
        p.task_status = None;
        p.project_status = Some(ProjectStatus::Active);
        p.progress = Some((3, 15));
        p.due = Some(date!(2027 - 04 - 01));
        p.zone = Zone::Inside {
            remaining: 5,
            room: 10,
            level: ZoneLevel::Wide,
        };
        assert_eq!(line_two(&p), "Active · 3/15 done");
        assert_eq!(
            line_three(&p, Lens::Route),
            "10 days of room · target Apr 1"
        );
        p.health = Some(HealthLevel::Red);
        p.late_by_days = Some(3);
        p.late = true;
        assert_eq!(line_three(&p, Lens::Risk), "late 3d · health red");
        p.blocked = true;
        assert!(line_three(&p, Lens::Risk).contains("paused"));
    }

    #[test]
    fn arrows_say_the_lag_or_the_room() {
        let mut e = edge(1, 1, 2);
        assert_eq!(edge_label(&e, GraphLevel::Tasks), None);
        e.lag_days = Some(0);
        assert_eq!(edge_label(&e, GraphLevel::Tasks), None);
        e.lag_days = Some(2);
        assert_eq!(edge_label(&e, GraphLevel::Tasks).as_deref(), Some("+2d"));
        assert_eq!(edge_label(&e, GraphLevel::Projects), None);
        e.room_days = Some(0);
        assert_eq!(
            edge_label(&e, GraphLevel::Projects).as_deref(),
            Some("no room")
        );
        e.room_days = Some(3);
        assert_eq!(
            edge_label(&e, GraphLevel::Projects).as_deref(),
            Some("3d room")
        );
    }

    #[test]
    fn the_inspector_says_what_a_box_holds_up_waits_on_and_when() {
        let mut g = diamond();
        for n in &mut g.nodes {
            n.project_id = Some(id(if n.node.id == id(5) { 11 } else { 10 }));
        }
        let d = g.nodes.iter_mut().find(|n| n.node.id == id(4)).unwrap();
        d.finish = Some(date!(2027 - 03 - 05));
        d.due = Some(date!(2027 - 03 - 03));
        d.late_by_days = Some(2);
        d.critical = true;
        let lines = inspector_lines(&g, id(4));
        assert_eq!(lines[0], "Holds up 1 task.");
        assert_eq!(lines[1], "Waits on 3 tasks.");
        assert_eq!(lines[2], "Finishes Mar 5, due Mar 3: 2 working days late.");
        assert_eq!(lines[3], "On the critical path: any slip moves the finish.");
        // A box that holds up work in two projects says so.
        let a = inspector_lines(&g, id(1));
        assert_eq!(a[0], "Holds up 4 tasks in 2 projects.");
        assert_eq!(a[1], "Waits on nothing.");
        // Slack and a blocker.
        let c = g.nodes.iter_mut().find(|n| n.node.id == id(3)).unwrap();
        c.slack_days = Some(2.0);
        c.blocked = true;
        let c = inspector_lines(&g, id(3));
        assert!(c.contains(&"2 days of slack.".to_owned()), "{c:?}");
        assert!(c.contains(&"Blocked.".to_owned()));
        assert!(inspector_lines(&g, id(99)).is_empty());
    }

    #[test]
    fn a_link_is_said_in_a_sentence_for_tasks_and_for_projects() {
        let mut g = diamond();
        g.edges[0].lag_days = Some(2);
        g.edges[0].cross_project = true;
        assert_eq!(
            edge_sentence(&g, id(1001)),
            "A blocks B: B can't start until A has finished, plus 2 days. This link crosses projects."
        );
        assert_eq!(
            edge_sentence(&g, id(1002)),
            "A blocks C: C can't start until A has finished."
        );
        let mut p = graph(
            vec![node(1, "Platform"), node(2, "Launch")],
            vec![edge(1, 1, 2)],
            GraphLevel::Projects,
        );
        p.edges[0].room_days = Some(3);
        assert_eq!(
            edge_sentence(&p, id(1001)),
            "Launch depends on Platform: it can't start until Platform has finished. There are 3 working days of room between them."
        );
        p.edges[0].room_days = Some(0);
        assert!(edge_sentence(&p, id(1001)).ends_with("There is no room between them."));
        assert_eq!(edge_sentence(&p, id(5)), "");
    }

    #[test]
    fn a_drag_makes_the_link_the_direction_of_the_drag_means() {
        let tasks = new_link(GraphLevel::Tasks, id(1), id(2));
        assert_eq!(tasks.edge_type, EdgeType::Blocks);
        assert_eq!((tasks.from.id, tasks.to.id), (id(1), id(2)));
        // Projects: dragging from the one needed first onto the one that needs it.
        let projects = new_link(GraphLevel::Projects, id(1), id(2));
        assert_eq!(projects.edge_type, EdgeType::DependsOn);
        assert_eq!(projects.from.node_type, NodeType::Project);
        assert_eq!((projects.from.id, projects.to.id), (id(2), id(1)));
        assert_eq!(
            link_message(GraphLevel::Tasks, "A", "B"),
            "Linked: A blocks B. Press Ctrl/Cmd+Z to undo."
        );
        assert_eq!(
            link_message(GraphLevel::Projects, "Platform", "Launch"),
            "Linked: Launch depends on Platform. Press Ctrl/Cmd+Z to undo."
        );
        assert_eq!(lag_attrs(0), serde_json::json!({}));
        assert_eq!(lag_attrs(3), serde_json::json!({ "lag_days": 3 }));
    }

    #[test]
    fn the_insights_name_the_bottleneck_and_count_what_is_stuck() {
        let mut g = diamond();
        g.nodes[0].downstream = 4; // A
        g.nodes[3].downstream = 1; // D
        g.nodes[1].blocked = true;
        g.nodes[2].late = true;
        g.edges[0].cross_project = true;
        let found = insights(&g);
        let texts: Vec<&str> = found.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Biggest bottleneck: A holds up 4",
                "1 link crosses projects",
                "1 blocked",
                "1 late"
            ]
        );
        assert_eq!(found[0].node, Some(id(1)));
        assert_eq!(found[1].node, None);
        assert_eq!(found[2].node, Some(id(2)));
        // Nothing worth saying: nothing said. Finished work is not stuck.
        let mut quiet = diamond();
        for n in &mut quiet.nodes {
            n.downstream = 1;
        }
        quiet.nodes[0].blocked = true;
        quiet.nodes[0].done = true;
        assert!(insights(&quiet).is_empty());
        // At the Projects level the same idea has its own words.
        let mut p = graph(
            vec![node(1, "Platform"), node(2, "Launch")],
            vec![edge(1, 1, 2)],
            GraphLevel::Projects,
        );
        p.nodes[0].downstream = 3;
        p.nodes[1].blocked = true;
        let texts: Vec<String> = insights(&p).into_iter().map(|i| i.text).collect();
        assert_eq!(texts, ["Most depended on: Platform holds up 3", "1 paused"]);
    }

    #[test]
    fn an_expanded_box_lists_what_it_waits_on_and_holds_up_by_name_with_the_link_to_remove() {
        let mut g = diamond();
        g.nodes[1].blocked = true; // B
        g.nodes[1].task_status = Some(TaskStatus::Blocked);
        let (waits, holds) = neighbours(&g, id(4)); // D waits on B and C, holds up E
        let w: Vec<(&str, &str, bool)> = waits
            .iter()
            .map(|n| (n.label.as_str(), n.status.as_str(), n.stuck))
            .collect();
        assert_eq!(w, [("B", "Blocked", true), ("C", "To do", false)]);
        assert_eq!(waits[0].edge, id(1003));
        assert_eq!(holds.len(), 1);
        assert_eq!((holds[0].label.as_str(), holds[0].node), ("E", id(5)));
        assert_eq!(holds[0].edge, id(1005));
        // Nothing around it, nothing listed.
        let (w, h) = neighbours(&g, id(99));
        assert!(w.is_empty() && h.is_empty());
    }

    /// The diamond (A -> B, A -> C, B -> D, C -> D, D -> E, see `diamond`) with E related to A
    /// and C part of B.
    fn with_relations() -> DependencyGraph {
        let mut g = diamond();
        g.edges.push(linked(7, 1, 5, GraphEdgeKind::RelatesTo));
        g.edges.push(linked(8, 2, 3, GraphEdgeKind::Subtask));
        g
    }

    #[test]
    fn related_and_part_of_links_are_listed_apart_from_what_a_box_waits_on_and_holds_up() {
        let g = with_relations();
        // B waits on A, holds up D; the sub-task C is not "held up" by it.
        let (waits, holds) = neighbours(&g, id(2));
        assert_eq!(waits.iter().map(|n| n.node).collect::<Vec<_>>(), [id(1)]);
        assert_eq!(holds.iter().map(|n| n.node).collect::<Vec<_>>(), [id(4)]);
        let also = |of: u128| -> Vec<(u128, &'static str)> {
            also_linked(&g, id(of))
                .into_iter()
                .map(|n| (n.node.as_u128(), n.how))
                .collect()
        };
        assert_eq!(also(2), [(3, "Sub-task")]);
        assert_eq!(also(3), [(2, "Part of")]);
        assert_eq!(also(1), [(5, "Related")]);
        assert_eq!(also(5), [(1, "Related")]);
        assert!(also(4).is_empty());
        // A part-of link drawn the other way round still says who is the parent.
        let mut flipped = with_relations();
        flipped.edges[7].from = id(3);
        flipped.edges[7].to = id(2);
        flipped.edges[7].reversed = true;
        let names: Vec<(u128, &str)> = also_linked(&flipped, id(2))
            .into_iter()
            .map(|n| (n.node.as_u128(), n.how))
            .collect();
        assert_eq!(names, [(3, "Sub-task")]);
    }

    #[test]
    fn a_trace_follows_the_dependencies_and_adds_only_the_boxes_own_other_links() {
        let g = with_relations();
        let t = trace_node(&g, id(2));
        // The chain B is in: A before it, D and E after. The related E-A link is not on it.
        assert_eq!(t.up, HashSet::from([id(1)]));
        assert_eq!(t.down, HashSet::from([id(4), id(5)]));
        assert!(!t.edges.contains(&id(1007)));
        // B's own part-of link is lit, with its other end.
        assert!(t.edges.contains(&id(1008)) && t.nodes.contains(&id(3)));
        // The sub-task C is not "up" or "down" of B because of it.
        assert!(!t.up.contains(&id(3)) && !t.down.contains(&id(3)));
        // A box with only a related link lights the link and the box on its other end.
        let e = trace_node(
            &graph(
                vec![node(1, "A"), node(2, "B")],
                vec![linked(1, 1, 2, GraphEdgeKind::RelatesTo)],
                GraphLevel::Tasks,
            ),
            id(1),
        );
        assert_eq!(e.nodes, HashSet::from([id(1), id(2)]));
        assert_eq!(e.edges, HashSet::from([id(1001)]));
        assert!(e.up.is_empty() && e.down.is_empty());
    }

    #[test]
    fn links_that_only_relate_tasks_have_their_own_classes_and_sentences() {
        let g = with_relations();
        let class = |n: u128| edge_class(&g.edges[(n - 1) as usize], Lens::Route, None, None, None);
        assert_eq!(class(1), "edge");
        assert_eq!(class(7), "edge related");
        assert_eq!(class(8), "edge part");
        assert_eq!(
            edge_sentence(&g, id(1007)),
            "A is related to E. Neither task waits for the other."
        );
        assert_eq!(
            edge_sentence(&g, id(1008)),
            "C is part of B. Nothing waits and no date moves."
        );
        // A link drawn the other way round says the same.
        let mut flipped = with_relations();
        flipped.edges[7].from = id(3);
        flipped.edges[7].to = id(2);
        flipped.edges[7].reversed = true;
        assert_eq!(
            edge_sentence(&flipped, id(1008)),
            "C is part of B. Nothing waits and no date moves."
        );
        // The key explains the lines on the task graph and has none for projects.
        assert_eq!(link_key(GraphLevel::Tasks).len(), 3);
        assert!(link_key(GraphLevel::Projects).is_empty());
    }

    #[test]
    fn why_a_box_is_marked_is_said_with_names() {
        let mut g = diamond();
        g.nodes[0].blocked = true; // A
        g.nodes[1].blocked = true; // B
        g.nodes[3].held_up = true; // D waits on A, B and C
        g.nodes[3].late_by_days = Some(2);
        assert_eq!(
            why_lines(&g, id(4)),
            [
                "Held up by “A” and “B”, which are blocked.",
                "Late by 2 working days."
            ]
        );
        assert_eq!(why_lines(&g, id(1)), ["Blocked: its status is Blocked."]);
        // One blocker, three, and a held-up box whose blocker is out of view.
        g.nodes[1].blocked = false;
        assert_eq!(why_lines(&g, id(4))[0], "Held up by “A”, which is blocked.");
        g.nodes[1].blocked = true;
        g.nodes[2].blocked = true;
        assert_eq!(
            why_lines(&g, id(4))[0],
            "Held up by “A”, “B” and 1 more, which are blocked."
        );
        g.nodes[0].blocked = false;
        g.nodes[1].blocked = false;
        g.nodes[2].blocked = false;
        assert_eq!(why_lines(&g, id(4))[0], "Held up by a blocked task.");
        // A box with nothing wrong says nothing; a project says it its own way.
        assert!(why_lines(&g, id(5)).is_empty());
        let mut p = graph(vec![node(1, "Platform")], vec![], GraphLevel::Projects);
        p.nodes[0].node = NodeRef::new(NodeType::Project, id(1));
        p.nodes[0].blocked = true;
        p.nodes[0].late_by_days = Some(3);
        assert_eq!(
            why_lines(&p, id(1)),
            ["Paused.", "Projected 3 working days past its target."]
        );
    }

    #[test]
    fn the_status_buttons_cover_the_four_working_statuses_and_a_layout_move_is_undone_by_the_pan() {
        let labels: Vec<&str> = status_choices().iter().map(|c| c.1).collect();
        assert_eq!(labels, ["To do", "In progress", "Blocked", "Done"]);
        // A box moved from (100, 50) to (100, 20) at 2x zoom: pan down by 60 so it stays put.
        assert_eq!(
            keep_in_place((10.0, 10.0), 2.0, (100.0, 50.0), (100.0, 20.0)),
            (10.0, 70.0)
        );
        assert_eq!(
            keep_in_place((10.0, 10.0), 1.0, (5.0, 5.0), (5.0, 5.0)),
            (10.0, 10.0)
        );
        assert_eq!(
            midpoint(&[(0.0, 0.0), (10.0, 0.0), (10.0, 20.0)]),
            Some((10.0, 5.0))
        );
        assert_eq!(midpoint(&[(0.0, 0.0), (10.0, 4.0)]), Some((5.0, 2.0)));
        assert_eq!(midpoint(&[]), None);
        assert_eq!(midpoint(&[(1.0, 1.0)]), None);
    }

    #[test]
    fn detail_follows_the_zoom_and_each_lens_has_a_key_and_a_hint() {
        assert_eq!(detail_level(1.0), Detail::Full);
        assert_eq!(detail_level(0.5), Detail::Full);
        assert_eq!(detail_level(0.4), Detail::Compact);
        assert_eq!(detail_level(0.3), Detail::Compact);
        assert_eq!(detail_level(0.2), Detail::Shape);
        for lens in Lens::ALL {
            assert!(!legend(lens).is_empty() && !lens.hint().is_empty());
            // Every lens explains what is wrong with work, not only its own marks.
            let words: Vec<&str> = legend(lens).into_iter().map(|k| k.1).collect();
            for word in ["late", "blocked", "held up by a blocked task"] {
                assert!(words.contains(&word), "{lens:?} lacks {word}");
            }
        }
        assert_eq!(legend(Lens::Route)[0].1, "critical path");
        assert_eq!(legend(Lens::People)[0].1, "colour = owner");
        assert_eq!(Lens::Route.label(), "Route");
        assert_eq!(short_date(date!(2027 - 10 - 07)), "Oct 7");
    }

    // ---- the unlinked list

    fn unlinked(n: u128, label: &str, f: impl Fn(&mut GraphNode)) -> GraphUnlinked {
        let mut look = node(n, label);
        f(&mut look);
        GraphUnlinked {
            node: look.node,
            label: label.into(),
            subtitle: look.subtitle.clone(),
            look,
        }
    }

    fn person(n: u128, name: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Person, id(n)),
            label: name.into(),
            archived: false,
        }
    }

    fn list() -> Vec<GraphUnlinked> {
        vec![
            unlinked(1, "Select a data provider", |n| {
                n.assignee = Some(person(100, "Udit"));
                n.project_id = Some(id(50));
                n.subtitle = "Small Bonds Portal".into();
            }),
            unlinked(2, "Rotate credentials", |n| {
                n.task_status = Some(TaskStatus::Blocked);
                n.assignee = Some(person(101, "Vinay"));
                n.project_id = Some(id(51));
                n.subtitle = "Platform".into();
            }),
            unlinked(3, "Book the offsite", |n| {
                n.task_status = Some(TaskStatus::InProgress);
                n.subtitle = "Inbox".into();
            }),
        ]
    }

    fn found(f: &UnlinkedFilter) -> Vec<String> {
        filter_unlinked(&list(), f)
            .into_iter()
            .map(|u| u.label.clone())
            .collect()
    }

    #[test]
    fn an_empty_filter_keeps_everything_in_order() {
        assert!(UnlinkedFilter::default().is_empty());
        assert_eq!(
            found(&UnlinkedFilter::default()),
            [
                "Select a data provider",
                "Rotate credentials",
                "Book the offsite"
            ]
        );
    }

    #[test]
    fn the_search_needs_every_word_in_the_name_project_owner_or_status() {
        let text = |t: &str| UnlinkedFilter {
            text: t.into(),
            ..Default::default()
        };
        assert_eq!(found(&text("provider")), ["Select a data provider"]);
        assert_eq!(found(&text("DATA  select")), ["Select a data provider"]);
        // The project line, the owner and the status are searched too.
        assert_eq!(found(&text("platform")), ["Rotate credentials"]);
        assert_eq!(found(&text("vinay")), ["Rotate credentials"]);
        assert_eq!(found(&text("in progress")), ["Book the offsite"]);
        assert!(found(&text("provider vinay")).is_empty());
        assert!(found(&text("zzz")).is_empty());
    }

    #[test]
    fn status_project_and_owner_each_narrow_the_list() {
        let blocked = UnlinkedFilter {
            status: "blocked".into(),
            ..Default::default()
        };
        assert_eq!(found(&blocked), ["Rotate credentials"]);
        let project = UnlinkedFilter {
            project: id(50).to_string(),
            ..Default::default()
        };
        assert_eq!(found(&project), ["Select a data provider"]);
        let inbox = UnlinkedFilter {
            project: "none".into(),
            ..Default::default()
        };
        assert_eq!(found(&inbox), ["Book the offsite"]);
        let udit = UnlinkedFilter {
            owner: id(100).to_string(),
            ..Default::default()
        };
        assert_eq!(found(&udit), ["Select a data provider"]);
        let nobody = UnlinkedFilter {
            owner: "none".into(),
            ..Default::default()
        };
        assert_eq!(found(&nobody), ["Book the offsite"]);
        // They combine: all must hold.
        let both = UnlinkedFilter {
            status: "blocked".into(),
            owner: id(100).to_string(),
            ..Default::default()
        };
        assert!(found(&both).is_empty());
    }

    #[test]
    fn the_filter_boxes_offer_only_what_is_in_the_list() {
        let items = list();
        let statuses: Vec<String> = status_choices_of(&items)
            .into_iter()
            .map(|(_, l)| l)
            .collect();
        assert_eq!(statuses, ["Any status", "Blocked", "In progress", "To do"]);
        let owners: Vec<String> = owner_choices_of(&items)
            .into_iter()
            .map(|(_, l)| l)
            .collect();
        assert_eq!(owners, ["Anyone", "Udit", "Vinay", "Unassigned"]);
        let name = |p: Uuid| (p == id(50)).then(|| "Small Bonds Portal".to_owned());
        let projects: Vec<String> = project_choices_of(&items, &name)
            .into_iter()
            .map(|(_, l)| l)
            .collect();
        assert_eq!(
            projects,
            [
                "Any project",
                "A project",
                "Small Bonds Portal",
                "No project"
            ]
        );
        // Nothing unassigned, nothing in the inbox: those choices are not offered.
        let only = vec![items[0].clone()];
        assert_eq!(owner_choices_of(&only).len(), 2);
        assert_eq!(project_choices_of(&only, &name).len(), 2);
    }

    #[test]
    fn a_dropped_or_clicked_item_links_in_the_direction_asked_for() {
        let (item, target) = (id(1), id(2));
        // Forward: the item blocks the box. Reverse: the box blocks the item.
        assert_eq!(tray_pair(item, target, false), Some((item, target)));
        assert_eq!(tray_pair(item, target, true), Some((target, item)));
        // Dropping something on itself makes no link.
        assert_eq!(tray_pair(item, item, false), None);
        // And the link it makes is the one a drag from a box's handle makes.
        let edge = new_link(GraphLevel::Tasks, item, target);
        assert_eq!((edge.from.id, edge.to.id), (item, target));
    }

    #[test]
    fn dropping_beside_the_waiting_box_follows_the_left_to_right_convention() {
        let (item, waiting) = (id(1), id(2));
        // To the right of the waiting box: it blocks the item (the item comes after).
        assert_eq!(
            drop_pair(item, waiting, 500.0, 300.0),
            Some((waiting, item))
        );
        // To the left: the item blocks it (the item comes first).
        assert_eq!(
            drop_pair(item, waiting, 100.0, 300.0),
            Some((item, waiting))
        );
        // Exactly on the centre counts as the right.
        assert_eq!(
            drop_pair(item, waiting, 300.0, 300.0),
            Some((waiting, item))
        );
        // The same task dropped twice links nothing.
        assert_eq!(drop_pair(item, item, 100.0, 300.0), None);
        // And the pair is the order `new_link` draws left to right.
        let edge = new_link(GraphLevel::Tasks, item, waiting);
        assert_eq!((edge.from.id, edge.to.id), (item, waiting));
        assert!(drop_beside_banner("Fetch").contains("right of \u{201c}Fetch\u{201d}"));
        assert!(WAITING_LINE.contains("Not linked yet"));
    }

    #[test]
    fn the_header_says_how_many_pass() {
        assert_eq!(unlinked_count_text(14, 14, false), "Unlinked (14)");
        assert_eq!(unlinked_count_text(14, 3, true), "Unlinked (3 of 14)");
    }

    #[test]
    fn the_hints_say_what_a_click_and_a_double_click_do() {
        for hint in [NODE_HINT, CARD_HINT, LEGEND_HINT] {
            assert!(
                hint.contains("double-click") || hint.contains("Double-click"),
                "{hint}"
            );
            assert!(hint.contains("right"), "{hint}");
        }
        assert!(NODE_HINT.contains("expand"));
        assert!(LEGEND_HINT.contains("click a box to expand it"));
    }
}
