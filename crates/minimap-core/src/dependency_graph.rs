//! The dependency graph (spec 18): which work blocks which, laid out left to right. Pure.
//!
//! Two levels: **tasks** joined by `blocks`, and **projects** joined by `depends_on`. The task level
//! also draws the links that do not order the work (`relates_to`, `subtask_of`), so every relation
//! between two tasks is on the graph; they take part in the layout (to be placed near each other)
//! but never in the schedule, the critical path or what a task waits on or holds up. Filters
//! (project, team, objective) choose the work of interest; the nodes it is linked to (one hop)
//! are shown dimmed as *context* so no link dangles. Unlinked nodes are left out unless asked
//! for. The critical path comes from the schedule (spec 13): critical tasks and the links that
//! drive the finish are marked.

use std::collections::{HashMap, HashSet};

use minimap_types::{
    Date, DependencyGraph, Edge, EdgeType, GraphEdge, GraphEdgeKind, GraphFilter, GraphLevel,
    GraphNode, GraphUnlinked, HealthLevel, NodeRef, NodeSummary, NodeType, Person, Project,
    ProjectStatus, ScheduleScope, ScheduledTask, Task, TaskStatus, Team, Uuid, WorkWeek, Zone,
    EXPANDED_PROJECT_H, EXPANDED_TASK_H, EXPANDED_W, MAX_UNLINKED_LISTED,
};

use crate::{
    layout::{layout, LayoutError, Params},
    schedule,
};

/// Most nodes one graph shows; beyond this the filters should narrow it.
pub const MAX_NODES: usize = 400;
const NODE_W: f64 = 210.0;
/// Tall enough for three lines (spec 47).
const NODE_H: f64 = 54.0;
/// The space between the graph and the dropped item waiting below it.
const WAITING_GAP: f64 = 70.0;
const EPS: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    #[error("The links form a loop, so the graph can't be laid out")]
    Cycle,
    #[error(
        "That is {0} nodes; narrow it with a project, team or objective (the limit is {MAX_NODES})"
    )]
    TooLarge(usize),
}

impl From<LayoutError> for GraphError {
    fn from(_: LayoutError) -> Self {
        GraphError::Cycle
    }
}

pub struct GraphInput<'a> {
    pub tasks: &'a [Task],
    pub edges: &'a [Edge],
    pub projects: &'a [Project],
    pub teams: &'a [Team],
    /// For the name of an assignee or an owner (spec 47).
    pub people: &'a [Person],
    /// The projects' computed health (the Overview's), for the Projects level.
    pub project_health: &'a HashMap<Uuid, HealthLevel>,
    /// The safe zone is narrow at this many days of room or fewer (spec 44).
    pub zone_narrow_days: u32,
    pub today: Date,
    pub work_week: WorkWeek,
    /// What is in My view; only read when the filter asks for `mine`.
    pub scope: Option<&'a minimap_types::MyScope>,
}

/// What a box says beyond its title (spec 47).
#[derive(Default, Clone)]
struct Facts {
    task_status: Option<TaskStatus>,
    project_status: Option<ProjectStatus>,
    priority: u8,
    assignee: Option<NodeSummary>,
    project_id: Option<Uuid>,
    finish: Option<Date>,
    due: Option<Date>,
    slack_days: Option<f64>,
    late_by_days: Option<u32>,
    estimate_days: Option<f64>,
    task_type: Option<String>,
    health: Option<HealthLevel>,
    zone: Zone,
    progress: Option<(u32, u32)>,
}

/// A node before layout.
struct Draft {
    node: NodeRef,
    label: String,
    subtitle: String,
    context: bool,
    done: bool,
    critical: bool,
    late: bool,
    blocked: bool,
    held_up: bool,
    /// The unlinked item the user dropped on the graph (`GraphFilter::waiting`).
    waiting: bool,
    facts: Facts,
}

struct DraftEdge {
    id: Uuid,
    from: Uuid,
    to: Uuid,
    lag_days: Option<u32>,
    critical: bool,
    cross_project: bool,
    room_days: Option<u32>,
    kind: GraphEdgeKind,
    reversed: bool,
}

fn person_summary(people: &[Person], id: Uuid) -> Option<NodeSummary> {
    people.iter().find(|p| p.id == id).map(|p| NodeSummary {
        node: NodeRef::new(NodeType::Person, p.id),
        label: p.name.clone(),
        archived: false,
    })
}

fn active(e: &Edge, kind: EdgeType) -> bool {
    e.edge_type == kind && e.archived_at.is_none()
}

/// Whether `from` can already reach `to` along `next`.
fn reaches(next: &HashMap<Uuid, Vec<Uuid>>, from: Uuid, to: Uuid) -> bool {
    let mut seen: HashSet<Uuid> = HashSet::from([from]);
    let mut stack = vec![from];
    while let Some(id) = stack.pop() {
        if id == to {
            return true;
        }
        for &n in next.get(&id).into_iter().flatten() {
            if seen.insert(n) {
                stack.push(n);
            }
        }
    }
    false
}

fn lag_of(e: &Edge) -> Option<u32> {
    e.attrs
        .get("lag_days")
        .and_then(|v| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

pub fn build(input: &GraphInput, filter: &GraphFilter) -> Result<DependencyGraph, GraphError> {
    match filter.level {
        GraphLevel::Tasks => build_tasks(input, filter),
        GraphLevel::Projects => build_projects(input, filter),
    }
}

/// `team` and everything nested under it.
fn team_family(teams: &[Team], root: Uuid) -> HashSet<Uuid> {
    let mut family = HashSet::from([root]);
    loop {
        let before = family.len();
        for t in teams {
            if t.parent_team_id.is_some_and(|p| family.contains(&p)) {
                family.insert(t.id);
            }
        }
        if family.len() == before {
            return family;
        }
    }
}

/// Unfinished tasks that wait, directly or down the chain, on a task whose status is Blocked.
fn held_up_by_blocked(input: &GraphInput) -> HashSet<Uuid> {
    let open: HashMap<Uuid, &Task> = input
        .tasks
        .iter()
        .filter(|t| {
            t.archived_at.is_none() && !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
        })
        .map(|t| (t.id, t))
        .collect();
    let mut next: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in input.edges.iter().filter(|e| active(e, EdgeType::Blocks)) {
        next.entry(e.from_id).or_default().push(e.to_id);
    }
    let mut stack: Vec<Uuid> = open
        .values()
        .filter(|t| t.status == TaskStatus::Blocked)
        .map(|t| t.id)
        .collect();
    let mut held = HashSet::new();
    while let Some(id) = stack.pop() {
        for &to in next.get(&id).into_iter().flatten() {
            if open.contains_key(&to) && held.insert(to) {
                stack.push(to);
            }
        }
    }
    held
}

fn build_tasks(input: &GraphInput, filter: &GraphFilter) -> Result<DependencyGraph, GraphError> {
    let schedule = schedule::compute(
        input.tasks,
        input.edges,
        input.projects,
        input.today,
        input.work_week,
        ScheduleScope::Portfolio,
    )
    .map_err(|_| GraphError::Cycle)?;
    let scheduled: HashMap<Uuid, &ScheduledTask> =
        schedule.tasks.iter().map(|t| (t.id, t)).collect();
    let project_title: HashMap<Uuid, &str> = input
        .projects
        .iter()
        .map(|p| (p.id, p.title.as_str()))
        .collect();

    let candidates: HashMap<Uuid, &Task> = input
        .tasks
        .iter()
        .filter(|t| t.archived_at.is_none() && t.status != TaskStatus::Cancelled)
        .filter(|t| filter.include_done || t.status != TaskStatus::Done)
        .map(|t| (t.id, t))
        .collect();
    // Every link between two of these tasks: the dependency and the ones that only relate them.
    let links: Vec<(&Edge, GraphEdgeKind)> = input
        .edges
        .iter()
        .filter_map(|e| {
            let kind = if active(e, EdgeType::Blocks) {
                GraphEdgeKind::Blocks
            } else if active(e, EdgeType::RelatesTo) {
                GraphEdgeKind::RelatesTo
            } else if active(e, EdgeType::SubtaskOf) {
                GraphEdgeKind::Subtask
            } else {
                return None;
            };
            Some((e, kind))
        })
        .filter(|(e, _)| candidates.contains_key(&e.from_id) && candidates.contains_key(&e.to_id))
        .collect();

    // Which tasks the filters select.
    let mut seeds: HashSet<Uuid> = candidates.keys().copied().collect();
    if let Some(pid) = filter.project_id {
        seeds.retain(|id| candidates[id].project_id == Some(pid));
    }
    if let Some(team) = filter.team_id {
        let family = team_family(input.teams, team);
        let members: HashSet<Uuid> = input
            .edges
            .iter()
            .filter(|e| active(e, EdgeType::MemberOf) && family.contains(&e.to_id))
            .map(|e| e.from_id)
            .collect();
        let assigned: HashSet<Uuid> = input
            .edges
            .iter()
            .filter(|e| active(e, EdgeType::AssignedTo) && members.contains(&e.to_id))
            .map(|e| e.from_id)
            .collect();
        seeds.retain(|id| assigned.contains(id));
    }
    if let Some(obj) = filter.objective_id {
        let feeding: HashSet<Uuid> = input
            .edges
            .iter()
            .filter(|e| active(e, EdgeType::ContributesTo) && e.to_id == obj)
            .map(|e| e.from_id)
            .collect();
        seeds.retain(|id| {
            feeding.contains(id)
                || candidates[id]
                    .project_id
                    .is_some_and(|p| feeding.contains(&p))
        });
    }
    if filter.mine {
        seeds.retain(|id| input.scope.is_some_and(|s| s.tasks.contains_key(id)));
    }
    let filtered = filter.project_id.is_some()
        || filter.team_id.is_some()
        || filter.objective_id.is_some()
        || filter.mine;

    // Seeds, plus (when filtering) what they are linked to as context.
    let mut visible: HashSet<Uuid> = seeds.clone();
    if filtered {
        for (e, _) in &links {
            if seeds.contains(&e.from_id) {
                visible.insert(e.to_id);
            }
            if seeds.contains(&e.to_id) {
                visible.insert(e.from_id);
            }
        }
    }
    let mut edges: Vec<(&Edge, GraphEdgeKind)> = links
        .iter()
        .copied()
        .filter(|(e, _)| visible.contains(&e.from_id) && visible.contains(&e.to_id))
        // Two pieces of context aren't worth drawing a link between.
        .filter(|(e, _)| seeds.contains(&e.from_id) || seeds.contains(&e.to_id))
        .collect();
    // The dependencies first: they are acyclic, so the others are placed around them.
    edges.sort_by_key(|(_, kind)| match kind {
        GraphEdgeKind::Blocks => 0,
        GraphEdgeKind::Subtask => 1,
        GraphEdgeKind::RelatesTo => 2,
    });
    let linked: HashSet<Uuid> = edges
        .iter()
        .flat_map(|(e, _)| [e.from_id, e.to_id])
        .collect();
    let mut hidden: Vec<Uuid> = Vec::new();
    // The item dropped onto the graph is drawn whether or not anything links to it.
    let waiting = filter.waiting.filter(|w| candidates.contains_key(w));
    visible.extend(waiting);
    if !filter.include_isolated {
        visible.retain(|id| {
            let keep = linked.contains(id) || waiting == Some(*id);
            if !keep {
                hidden.push(*id);
            }
            keep
        });
    }
    if visible.len() > MAX_NODES {
        return Err(GraphError::TooLarge(visible.len()));
    }

    let by_title = |a: &Uuid, b: &Uuid| {
        let (ta, tb) = (candidates[a], candidates[b]);
        ta.title
            .to_lowercase()
            .cmp(&tb.title.to_lowercase())
            .then(a.cmp(b))
    };
    let mut ids: Vec<Uuid> = visible.into_iter().collect();
    ids.sort_by(by_title);
    let held_up = held_up_by_blocked(input);
    // Each task's first assignee (the earliest link).
    let mut assignee_of: HashMap<Uuid, Uuid> = HashMap::new();
    let mut assign_links: Vec<&Edge> = input
        .edges
        .iter()
        .filter(|e| active(e, EdgeType::AssignedTo) && e.from_type == NodeType::Task)
        .collect();
    assign_links.sort_by_key(|e| e.id);
    for e in assign_links {
        assignee_of.entry(e.from_id).or_insert(e.to_id);
    }
    let subtitle_of = |id: &Uuid| -> String {
        let t = candidates[id];
        let project = t
            .project_id
            .and_then(|p| project_title.get(&p).copied())
            .unwrap_or("Inbox");
        match scheduled.get(id) {
            Some(s) if !s.done => format!("→ {} · {project}", s.finish),
            _ => project.to_owned(),
        }
    };
    let make_draft = |id: &Uuid| {
        let t = candidates[id];
        let s = scheduled.get(id);
        Draft {
            node: NodeRef::new(NodeType::Task, *id),
            label: t.title.clone(),
            subtitle: subtitle_of(id),
            context: !seeds.contains(id) && waiting != Some(*id),
            done: t.status == TaskStatus::Done,
            critical: s.is_some_and(|s| s.critical),
            late: s.is_some_and(|s| s.slack_days < -EPS),
            blocked: t.status == TaskStatus::Blocked,
            held_up: held_up.contains(id),
            waiting: waiting == Some(*id),
            facts: Facts {
                task_status: Some(t.status),
                priority: t.priority,
                assignee: assignee_of
                    .get(id)
                    .and_then(|p| person_summary(input.people, *p)),
                project_id: t.project_id,
                finish: s.filter(|s| !s.done).map(|s| s.finish),
                due: t.due_date,
                slack_days: s.filter(|s| !s.done).map(|s| s.slack_days),
                late_by_days: s.and_then(|s| s.late_by_days),
                estimate_days: t.estimate_days,
                task_type: t.task_type.clone(),
                ..Facts::default()
            },
        }
    };
    let drafts: Vec<Draft> = ids.iter().map(make_draft).collect();
    // Which way each link is drawn (left to right). The layout needs no loop, so a link that
    // doesn't order the work, and would close one with the links before it, is drawn the other
    // way round.
    let mut next: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut draft_edges: Vec<DraftEdge> = Vec::new();
    for (e, kind) in &edges {
        let (mut from, mut to) = match kind {
            GraphEdgeKind::Blocks | GraphEdgeKind::RelatesTo => (e.from_id, e.to_id),
            // Stored sub-task -> parent; drawn parent -> sub-task.
            GraphEdgeKind::Subtask => (e.to_id, e.from_id),
        };
        let mut reversed = false;
        if !kind.orders_work() && reaches(&next, to, from) {
            std::mem::swap(&mut from, &mut to);
            reversed = *kind == GraphEdgeKind::Subtask;
        }
        next.entry(from).or_default().push(to);
        let (lag, critical) = if kind.orders_work() {
            let lag = lag_of(e).unwrap_or(0);
            let critical = match (scheduled.get(&e.from_id), scheduled.get(&e.to_id)) {
                (Some(a), Some(b)) => {
                    a.critical && b.critical && b.es - (a.ef + f64::from(lag)) <= EPS
                }
                _ => false,
            };
            (lag_of(e), critical)
        } else {
            (None, false)
        };
        draft_edges.push(DraftEdge {
            id: e.id,
            from,
            to,
            lag_days: lag,
            critical,
            cross_project: candidates[&from].project_id != candidates[&to].project_id,
            room_days: None,
            kind: *kind,
            reversed,
        });
    }
    hidden.sort_by(by_title);
    let unlinked: Vec<GraphUnlinked> = hidden
        .iter()
        .take(MAX_UNLINKED_LISTED)
        .map(|id| unlisted(make_draft(id)))
        .collect();
    finish(
        GraphLevel::Tasks,
        drafts,
        draft_edges,
        hidden.len() as u32,
        unlinked,
        Vec::new(),
        filter.expanded,
    )
}

fn build_projects(input: &GraphInput, filter: &GraphFilter) -> Result<DependencyGraph, GraphError> {
    let schedule = schedule::compute(
        input.tasks,
        input.edges,
        input.projects,
        input.today,
        input.work_week,
        ScheduleScope::Portfolio,
    )
    .ok();
    let forecast: HashMap<Uuid, &minimap_types::ProjectForecast> = schedule
        .iter()
        .flat_map(|s| s.projects.iter())
        .map(|f| (f.project_id, f))
        .collect();
    let mut warnings = Vec::new();
    if schedule.is_none() {
        warnings.push("Projected finishes are unavailable: the task links form a loop.".to_owned());
    }
    let candidates: HashMap<Uuid, &Project> = input
        .projects
        .iter()
        .filter(|p| p.archived_at.is_none() && p.status != ProjectStatus::Cancelled)
        .filter(|p| filter.include_done || p.status != ProjectStatus::Done)
        .map(|p| (p.id, p))
        .collect();
    let links: Vec<&Edge> = input
        .edges
        .iter()
        .filter(|e| active(e, EdgeType::DependsOn))
        .filter(|e| candidates.contains_key(&e.from_id) && candidates.contains_key(&e.to_id))
        .collect();

    let mut seeds: HashSet<Uuid> = candidates.keys().copied().collect();
    // At this level "project" means: focus on this one project.
    if let Some(pid) = filter.project_id {
        seeds.retain(|id| *id == pid);
    }
    if let Some(obj) = filter.objective_id {
        let feeding: HashSet<Uuid> = input
            .edges
            .iter()
            .filter(|e| active(e, EdgeType::ContributesTo) && e.to_id == obj)
            .map(|e| e.from_id)
            .collect();
        seeds.retain(|id| feeding.contains(id));
    }
    if filter.mine {
        seeds.retain(|id| input.scope.is_some_and(|s| s.projects.contains_key(id)));
    }
    let filtered = filter.project_id.is_some() || filter.objective_id.is_some() || filter.mine;
    let mut visible = seeds.clone();
    if filtered {
        for e in &links {
            if seeds.contains(&e.from_id) {
                visible.insert(e.to_id);
            }
            if seeds.contains(&e.to_id) {
                visible.insert(e.from_id);
            }
        }
    }
    let edges: Vec<&Edge> = links
        .iter()
        .copied()
        .filter(|e| visible.contains(&e.from_id) && visible.contains(&e.to_id))
        .filter(|e| seeds.contains(&e.from_id) || seeds.contains(&e.to_id))
        .collect();
    let linked: HashSet<Uuid> = edges.iter().flat_map(|e| [e.from_id, e.to_id]).collect();
    let mut hidden: Vec<Uuid> = Vec::new();
    // The item dropped onto the graph is drawn whether or not anything links to it.
    let waiting = filter.waiting.filter(|w| candidates.contains_key(w));
    visible.extend(waiting);
    if !filter.include_isolated {
        visible.retain(|id| {
            let keep = linked.contains(id) || waiting == Some(*id);
            if !keep {
                hidden.push(*id);
            }
            keep
        });
    }
    if visible.len() > MAX_NODES {
        return Err(GraphError::TooLarge(visible.len()));
    }
    let by_title = |a: &Uuid, b: &Uuid| {
        candidates[a]
            .title
            .to_lowercase()
            .cmp(&candidates[b].title.to_lowercase())
            .then(a.cmp(b))
    };
    let mut ids: Vec<Uuid> = visible.into_iter().collect();
    ids.sort_by(by_title);
    let subtitle_of = |id: &Uuid| -> String {
        let p = candidates[id];
        let status = crate::projects::status_title(p.status);
        match forecast.get(id).and_then(|f| f.projected_finish) {
            Some(d) if p.status != ProjectStatus::Done => format!("{status} · ends {d}"),
            _ => status.to_owned(),
        }
    };
    // How many of each project's tasks are done (cancelled ones don't count).
    let mut progress: HashMap<Uuid, (u32, u32)> = HashMap::new();
    for t in input
        .tasks
        .iter()
        .filter(|t| t.archived_at.is_none() && t.status != TaskStatus::Cancelled)
    {
        if let Some(p) = t.project_id {
            let entry = progress.entry(p).or_default();
            entry.1 += 1;
            if t.status == TaskStatus::Done {
                entry.0 += 1;
            }
        }
    }
    let make_draft = |id: &Uuid| {
        let p = candidates[id];
        let f = forecast.get(id);
        Draft {
            node: NodeRef::new(NodeType::Project, *id),
            label: p.title.clone(),
            subtitle: subtitle_of(id),
            context: !seeds.contains(id) && waiting != Some(*id),
            done: p.status == ProjectStatus::Done,
            critical: false,
            late: f.is_some_and(|f| f.late_by_days.is_some()),
            blocked: p.status == ProjectStatus::Paused,
            held_up: false,
            waiting: waiting == Some(*id),
            facts: Facts {
                project_status: Some(p.status),
                priority: p.priority,
                assignee: p
                    .owner_person_id
                    .and_then(|o| person_summary(input.people, o)),
                project_id: Some(*id),
                finish: f.and_then(|f| f.projected_finish),
                due: p.target_date,
                late_by_days: f.and_then(|f| f.late_by_days),
                health: input.project_health.get(id).copied(),
                zone: f.map_or(Zone::None, |f| {
                    crate::zone::of(
                        f,
                        p.status,
                        input.today,
                        input.work_week,
                        input.zone_narrow_days,
                    )
                }),
                progress: progress.get(id).copied(),
                ..Facts::default()
            },
        }
    };
    let drafts: Vec<Draft> = ids.iter().map(make_draft).collect();
    // The earliest start of each project's open work, for the room between two projects.
    let mut starts: HashMap<Uuid, f64> = HashMap::new();
    for t in schedule
        .iter()
        .flat_map(|s| s.tasks.iter())
        .filter(|t| !t.done)
    {
        if let Some(p) = t.project_id {
            let e = starts.entry(p).or_insert(f64::INFINITY);
            *e = e.min(t.es);
        }
    }
    let draft_edges: Vec<DraftEdge> = edges
        .iter()
        .map(|e| {
            // `A depends_on B` is drawn B -> A, so what is needed first is on the left.
            let (needed, dependent) = (e.to_id, e.from_id);
            let room_days = match (
                forecast.get(&needed).and_then(|f| f.finish_offset),
                starts.get(&dependent),
            ) {
                (Some(end), Some(start)) => Some((start - end).max(0.0).floor() as u32),
                _ => None,
            };
            DraftEdge {
                id: e.id,
                from: needed,
                to: dependent,
                lag_days: None,
                critical: false,
                cross_project: false,
                room_days,
                kind: GraphEdgeKind::Blocks,
                reversed: false,
            }
        })
        .collect();
    hidden.sort_by(by_title);
    let unlinked: Vec<GraphUnlinked> = hidden
        .iter()
        .take(MAX_UNLINKED_LISTED)
        .map(|id| unlisted(make_draft(id)))
        .collect();
    finish(
        GraphLevel::Projects,
        drafts,
        draft_edges,
        hidden.len() as u32,
        unlinked,
        warnings,
        filter.expanded,
    )
}

/// For each node, how many others it can reach going forward (`.0`, what it holds up) and how
/// many reach it (`.1`, what it waits on), through any number of links.
fn reach_counts(count: usize, pairs: &[(usize, usize)]) -> Vec<(u32, u32)> {
    let mut forward: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut backward: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in pairs {
        forward[a].push(b);
        backward[b].push(a);
    }
    let walk = |start: usize, next: &Vec<Vec<usize>>| -> u32 {
        let mut seen = vec![false; count];
        let mut stack = vec![start];
        seen[start] = true;
        let mut n = 0;
        while let Some(i) = stack.pop() {
            for &j in &next[i] {
                if !seen[j] {
                    seen[j] = true;
                    n += 1;
                    stack.push(j);
                }
            }
        }
        n
    };
    (0..count)
        .map(|i| (walk(i, &forward), walk(i, &backward)))
        .collect()
}

/// A draft as the node it is drawn as: its place `(x, y, w, h, layer)`, how many nodes it waits
/// on and holds up, and whether it is the open box.
fn to_node(
    d: Draft,
    (x, y, w, h, layer): (f64, f64, f64, f64, u32),
    (upstream, downstream): (u32, u32),
    expanded: bool,
) -> GraphNode {
    GraphNode {
        node: d.node,
        label: d.label,
        subtitle: d.subtitle,
        x,
        y,
        w,
        h,
        layer,
        context: d.context,
        done: d.done,
        critical: d.critical,
        late: d.late,
        blocked: d.blocked,
        held_up: d.held_up,
        task_status: d.facts.task_status,
        project_status: d.facts.project_status,
        priority: d.facts.priority,
        assignee: d.facts.assignee,
        project_id: d.facts.project_id,
        finish: d.facts.finish,
        due: d.facts.due,
        slack_days: d.facts.slack_days,
        late_by_days: d.facts.late_by_days,
        estimate_days: d.facts.estimate_days,
        task_type: d.facts.task_type,
        health: d.facts.health,
        zone: d.facts.zone,
        progress: d.facts.progress,
        upstream,
        downstream,
        expanded,
        waiting: d.waiting,
    }
}

/// An unlinked node for the list beside the graph: its name, and the box it would be drawn as
/// (no place, no links).
fn unlisted(d: Draft) -> GraphUnlinked {
    let (node, label, subtitle) = (d.node, d.label.clone(), d.subtitle.clone());
    GraphUnlinked {
        node,
        label,
        subtitle,
        look: to_node(d, (0.0, 0.0, NODE_W, NODE_H, 0), (0, 0), false),
    }
}

fn finish(
    level: GraphLevel,
    drafts: Vec<Draft>,
    edges: Vec<DraftEdge>,
    hidden_unlinked: u32,
    unlinked: Vec<GraphUnlinked>,
    warnings: Vec<String>,
    expanded: Option<Uuid>,
) -> Result<DependencyGraph, GraphError> {
    // The dropped item that nothing links to yet has no place in the layers: it goes below the
    // graph, centred, so the next item dropped to its left or right can be linked to it.
    let mut drafts = drafts;
    let lonely = drafts
        .iter()
        .position(|d| {
            d.waiting
                && !edges
                    .iter()
                    .any(|e| e.from == d.node.id || e.to == d.node.id)
        })
        .map(|i| drafts.remove(i));
    let index: HashMap<Uuid, usize> = drafts
        .iter()
        .enumerate()
        .map(|(i, d)| (d.node.id, i))
        .collect();
    let pairs: Vec<(usize, usize)> = edges
        .iter()
        .map(|e| (index[&e.from], index[&e.to]))
        .collect();
    // What a box waits on and holds up counts the dependencies only.
    let ordering: Vec<(usize, usize)> = edges
        .iter()
        .zip(&pairs)
        .filter(|(e, _)| e.kind.orders_work())
        .map(|(_, p)| *p)
        .collect();
    // The expanded box is laid out large, so the others make room for it.
    let sizes: Vec<(f64, f64)> = drafts
        .iter()
        .map(|d| {
            if expanded == Some(d.node.id) {
                let h = match level {
                    GraphLevel::Tasks => EXPANDED_TASK_H,
                    GraphLevel::Projects => EXPANDED_PROJECT_H,
                };
                (EXPANDED_W, h)
            } else {
                (NODE_W, NODE_H)
            }
        })
        .collect();
    let placed = layout(&sizes, &pairs, &Params::default())?;
    let reach = reach_counts(drafts.len(), &ordering);
    let nodes: Vec<GraphNode> = drafts
        .into_iter()
        .zip(&placed.nodes)
        .zip(&reach)
        .map(|((d, p), (downstream, upstream))| {
            let expanded = expanded == Some(d.node.id);
            to_node(
                d,
                (p.x, p.y, p.w, p.h, p.layer as u32),
                (*upstream, *downstream),
                expanded,
            )
        })
        .collect();
    let mut nodes = nodes;
    let (mut width, mut height) = (placed.width, placed.height);
    if let Some(d) = lonely {
        let open = expanded == Some(d.node.id);
        let (w, h) = if open {
            (
                EXPANDED_W,
                match level {
                    GraphLevel::Tasks => EXPANDED_TASK_H,
                    GraphLevel::Projects => EXPANDED_PROJECT_H,
                },
            )
        } else {
            (NODE_W, NODE_H)
        };
        let y = if nodes.is_empty() {
            0.0
        } else {
            height + WAITING_GAP
        };
        let x = ((width - w) / 2.0).max(0.0);
        width = width.max(w);
        height = y + h;
        nodes.push(to_node(d, (x, y, w, h, 0), (0, 0), open));
    }
    let critical_nodes = nodes.iter().filter(|n| n.critical).count() as u32;
    let edges = edges
        .into_iter()
        .zip(placed.routes)
        .map(|(e, points)| GraphEdge {
            id: e.id,
            from: e.from,
            to: e.to,
            lag_days: e.lag_days,
            critical: e.critical,
            points,
            cross_project: e.cross_project,
            room_days: e.room_days,
            kind: e.kind,
            reversed: e.reversed,
        })
        .collect();
    Ok(DependencyGraph {
        level,
        nodes,
        edges,
        width,
        height,
        critical_nodes,
        hidden_unlinked,
        unlinked,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{macros::date, OffsetDateTime};

    // Monday 2027-03-01.
    const MON: Date = date!(2027 - 03 - 01);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, days: f64, project: Option<u128>) -> Task {
        Task {
            confirmed_on: None,

            links: Vec::new(),
            task_type: None,
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: project.map(id),
            status: TaskStatus::Todo,
            estimate_days: Some(days),
            start_date: None,
            due_date: None,
            completed_at: None,
            priority: 3,
            recurrence: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn project(n: u128, title: &str, target: Option<Date>) -> Project {
        Project {
            confirmed_on: None,

            id: id(n),
            title: title.into(),
            slug: title.to_lowercase(),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: target,
            status: ProjectStatus::Active,
            priority: 3,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn team(n: u128, name: &str, parent: Option<u128>) -> Team {
        Team {
            id: id(n),
            name: name.into(),
            description: String::new(),
            parent_team_id: parent.map(id),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn edge(kind: EdgeType, ft: NodeType, from: u128, tt: NodeType, to: u128) -> Edge {
        Edge {
            id: Uuid::from_u128(90_000 + from * 1000 + to + u128::from(kind as u8) * 3),
            edge_type: kind,
            from_type: ft,
            from_id: id(from),
            to_type: tt,
            to_id: id(to),
            attrs: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn blocks(a: u128, b: u128) -> Edge {
        edge(EdgeType::Blocks, NodeType::Task, a, NodeType::Task, b)
    }

    #[derive(Default)]
    struct World {
        tasks: Vec<Task>,
        edges: Vec<Edge>,
        projects: Vec<Project>,
        teams: Vec<Team>,
        people: Vec<Person>,
        health: HashMap<Uuid, HealthLevel>,
        scope: Option<minimap_types::MyScope>,
    }

    impl World {
        fn graph(&self, filter: GraphFilter) -> Result<DependencyGraph, GraphError> {
            build(
                &GraphInput {
                    people: &self.people,
                    project_health: &self.health,
                    zone_narrow_days: 3,
                    tasks: &self.tasks,
                    edges: &self.edges,
                    projects: &self.projects,
                    teams: &self.teams,
                    today: MON,
                    work_week: WorkWeek::MON_FRI,
                    scope: self.scope.as_ref(),
                },
                &filter,
            )
        }
    }

    fn labels(g: &DependencyGraph) -> Vec<&str> {
        let mut v: Vec<&str> = g.nodes.iter().map(|n| n.label.as_str()).collect();
        v.sort();
        v
    }

    fn by<'a>(g: &'a DependencyGraph, label: &str) -> &'a GraphNode {
        g.nodes
            .iter()
            .find(|n| n.label == label)
            .unwrap_or_else(|| panic!("no {label}"))
    }

    // ------------------------------------------------------------- tasks

    #[test]
    fn a_chain_is_laid_out_left_to_right_with_the_critical_path_marked() {
        // A(2) -> B(3) -> C(1), and a side task S(1) -> C with plenty of slack.
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "A", 2.0, Some(10)),
                task(2, "B", 3.0, Some(10)),
                task(3, "C", 1.0, Some(10)),
                task(4, "S", 1.0, Some(10)),
            ],
            edges: vec![blocks(1, 2), blocks(2, 3), blocks(4, 3)],
            projects: vec![project(10, "P", None)],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(labels(&g), ["A", "B", "C", "S"]);
        assert_eq!(g.edges.len(), 3);
        // Columns follow the dependencies.
        assert!(by(&g, "A").x < by(&g, "B").x && by(&g, "B").x < by(&g, "C").x);
        assert_eq!(
            (by(&g, "A").layer, by(&g, "B").layer, by(&g, "C").layer),
            (0, 1, 2)
        );
        // The chain A-B-C decides the finish; S has slack.
        assert!(by(&g, "A").critical && by(&g, "B").critical && by(&g, "C").critical);
        assert!(!by(&g, "S").critical);
        assert_eq!(g.critical_nodes, 3);
        let edge = |from: u128, to: u128| {
            g.edges
                .iter()
                .find(|e| e.from == id(from) && e.to == id(to))
                .unwrap()
        };
        assert!(edge(1, 2).critical && edge(2, 3).critical);
        assert!(
            !edge(4, 3).critical,
            "a link from a task with slack doesn't drive the finish"
        );
        // Routes join the boxes' edges.
        let r = &edge(1, 2).points;
        assert_eq!(r[0].0, by(&g, "A").x + by(&g, "A").w);
        assert_eq!(r.last().unwrap().0, by(&g, "B").x);
        assert_eq!(g.hidden_unlinked, 0);
        assert!(by(&g, "A").subtitle.contains("P"));
    }

    #[test]
    fn unlinked_work_is_left_out_unless_asked_for() {
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "A", 1.0, None),
                task(2, "B", 1.0, None),
                task(3, "Lone", 1.0, None),
            ],
            edges: vec![blocks(1, 2)],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(labels(&g), ["A", "B"]);
        assert_eq!(g.hidden_unlinked, 1);
        let all = w
            .graph(GraphFilter {
                include_isolated: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&all), ["A", "B", "Lone"]);
        assert_eq!(all.hidden_unlinked, 0);
        assert!(by(&all, "Lone").subtitle.contains("Inbox"));
    }

    #[test]
    fn finished_cancelled_and_archived_work_is_hidden_and_done_can_be_shown() {
        let mut done = task(2, "Done", 1.0, None);
        done.status = TaskStatus::Done;
        done.completed_at = Some(date!(2027 - 02 - 26).midnight().assume_utc());
        let mut cancelled = task(3, "Cancelled", 1.0, None);
        cancelled.status = TaskStatus::Cancelled;
        let mut archived = task(4, "Archived", 1.0, None);
        archived.archived_at = Some(OffsetDateTime::UNIX_EPOCH);
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![task(1, "Open", 1.0, None), done, cancelled, archived],
            edges: vec![blocks(2, 1), blocks(3, 1), blocks(4, 1)],
            ..Default::default()
        };
        // Everything linked to "Open" is out, so "Open" is unlinked and hidden too.
        let g = w.graph(GraphFilter::default()).unwrap();
        assert!(g.nodes.is_empty());
        let with_done = w
            .graph(GraphFilter {
                include_done: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&with_done), ["Done", "Open"]);
        assert!(by(&with_done, "Done").done && !by(&with_done, "Done").critical);
    }

    #[test]
    fn a_project_filter_shows_its_tasks_and_dims_what_they_are_linked_to() {
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "Design", 2.0, Some(10)),
                task(2, "Build", 3.0, Some(11)),
                task(3, "Docs", 1.0, Some(11)),
                task(4, "Unrelated", 1.0, Some(12)),
                task(5, "Unrelated 2", 1.0, Some(12)),
            ],
            // Design (P10) blocks Build (P11); Build blocks Docs; the P12 pair is separate.
            edges: vec![blocks(1, 2), blocks(2, 3), blocks(4, 5)],
            projects: vec![
                project(10, "P10", None),
                project(11, "P11", None),
                project(12, "P12", None),
            ],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                project_id: Some(id(11)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&g), ["Build", "Design", "Docs"]);
        assert!(by(&g, "Design").context && !by(&g, "Build").context && !by(&g, "Docs").context);
        assert_eq!(g.edges.len(), 2);
        // Context is only what is linked to the project's own tasks, one hop.
        let g10 = w
            .graph(GraphFilter {
                project_id: Some(id(10)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&g10), ["Build", "Design"]);
        assert!(by(&g10, "Build").context);
    }

    #[test]
    fn a_team_filter_follows_members_including_sub_teams() {
        let person = |n| id(n);
        let _ = person;
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "Platform work", 1.0, None),
                task(2, "Infra work", 1.0, None),
                task(3, "Outsider work", 1.0, None),
                task(4, "Downstream", 1.0, None),
            ],
            edges: vec![
                blocks(1, 4),
                blocks(2, 4),
                blocks(3, 4),
                // Priya is in Platform; Raj in Infra, a sub-team of Platform; Sam is in no team.
                edge(EdgeType::MemberOf, NodeType::Person, 30, NodeType::Team, 50),
                edge(EdgeType::MemberOf, NodeType::Person, 31, NodeType::Team, 51),
                edge(
                    EdgeType::AssignedTo,
                    NodeType::Task,
                    1,
                    NodeType::Person,
                    30,
                ),
                edge(
                    EdgeType::AssignedTo,
                    NodeType::Task,
                    2,
                    NodeType::Person,
                    31,
                ),
                edge(
                    EdgeType::AssignedTo,
                    NodeType::Task,
                    3,
                    NodeType::Person,
                    32,
                ),
            ],
            teams: vec![team(50, "Platform", None), team(51, "Infra", Some(50))],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                team_id: Some(id(50)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&g), ["Downstream", "Infra work", "Platform work"]);
        assert!(by(&g, "Downstream").context);
        // Only the sub-team: just Raj's work.
        let infra = w
            .graph(GraphFilter {
                team_id: Some(id(51)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&infra), ["Downstream", "Infra work"]);
    }

    #[test]
    fn an_objective_filter_includes_projects_tasks_and_directly_linked_tasks() {
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "In project", 1.0, Some(10)),
                task(2, "Direct", 1.0, None),
                task(3, "Other", 1.0, Some(11)),
                task(4, "Join", 1.0, None),
            ],
            edges: vec![
                blocks(1, 4),
                blocks(2, 4),
                blocks(3, 4),
                edge(
                    EdgeType::ContributesTo,
                    NodeType::Project,
                    10,
                    NodeType::Objective,
                    20,
                ),
                edge(
                    EdgeType::ContributesTo,
                    NodeType::Task,
                    2,
                    NodeType::Objective,
                    20,
                ),
            ],
            projects: vec![project(10, "P10", None), project(11, "P11", None)],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                objective_id: Some(id(20)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            labels(&g),
            ["Direct", "In project", "Join", "Other"]
                .iter()
                .filter(|l| **l != "Other")
                .copied()
                .collect::<Vec<_>>()
        );
        assert!(by(&g, "Join").context);
    }

    #[test]
    fn filters_combine_with_and() {
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "A", 1.0, Some(10)),
                task(2, "B", 1.0, Some(10)),
                task(3, "C", 1.0, Some(11)),
                task(4, "D", 1.0, Some(11)),
            ],
            edges: vec![
                blocks(1, 2),
                blocks(3, 4),
                edge(
                    EdgeType::ContributesTo,
                    NodeType::Project,
                    11,
                    NodeType::Objective,
                    20,
                ),
            ],
            projects: vec![project(10, "P10", None), project(11, "P11", None)],
            ..Default::default()
        };
        let both = w
            .graph(GraphFilter {
                project_id: Some(id(10)),
                objective_id: Some(id(20)),
                ..Default::default()
            })
            .unwrap();
        assert!(both.nodes.is_empty(), "P10 isn't under objective 20");
        let one = w
            .graph(GraphFilter {
                project_id: Some(id(11)),
                objective_id: Some(id(20)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&one), ["C", "D"]);
    }

    #[test]
    fn late_work_is_flagged_and_blocked_status_is_shown() {
        // 5 days of chained work against a 2-day target.
        let mut stuck = task(2, "Stuck", 3.0, Some(10));
        stuck.status = TaskStatus::Blocked;
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![task(1, "First", 2.0, Some(10)), stuck],
            edges: vec![blocks(1, 2)],
            projects: vec![project(10, "P", Some(date!(2027 - 03 - 02)))],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert!(by(&g, "First").late && by(&g, "Stuck").late);
        assert!(by(&g, "Stuck").blocked && !by(&g, "First").blocked);
    }

    #[test]
    fn work_downstream_of_a_blocked_task_is_held_up() {
        // A -> B(blocked) -> C -> D, and C is also needed by a finished task E.
        let mut b = task(2, "B", 1.0, Some(10));
        b.status = TaskStatus::Blocked;
        let mut e = task(5, "E", 1.0, Some(10));
        e.status = TaskStatus::Done;
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![
                task(1, "A", 1.0, Some(10)),
                b,
                task(3, "C", 1.0, Some(10)),
                task(4, "D", 1.0, Some(10)),
                e,
            ],
            edges: vec![blocks(1, 2), blocks(2, 3), blocks(3, 4), blocks(2, 5)],
            projects: vec![project(10, "P", None)],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert!(by(&g, "C").held_up && by(&g, "D").held_up);
        assert!(!by(&g, "A").held_up && !by(&g, "B").held_up);
    }

    #[test]
    fn a_loop_and_an_oversized_graph_are_refused() {
        let w = World {
            people: vec![],
            health: HashMap::new(),
            tasks: vec![task(1, "A", 1.0, None), task(2, "B", 1.0, None)],
            edges: vec![blocks(1, 2), blocks(2, 1)],
            ..Default::default()
        };
        assert_eq!(
            w.graph(GraphFilter::default()).unwrap_err(),
            GraphError::Cycle
        );
        let mut big = World::default();
        for i in 0..(MAX_NODES as u128 + 2) {
            big.tasks.push(task(1000 + i, &format!("t{i}"), 1.0, None));
        }
        let e = big
            .graph(GraphFilter {
                include_isolated: true,
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(e, GraphError::TooLarge(n) if n == MAX_NODES + 2));
        assert!(e.to_string().contains("narrow it"));
    }

    #[test]
    fn a_hundred_nodes_lay_out_cleanly() {
        let mut w = World::default();
        for i in 0..100u128 {
            w.tasks.push(task(
                1000 + i,
                &format!("t{i:03}"),
                1.0 + (i % 4) as f64,
                Some(10 + i % 3),
            ));
        }
        for i in 0..100u128 {
            for step in [1u128, 3, 7] {
                if (i + step) < 100 && (i * 7 + step) % 3 != 0 {
                    w.edges.push(blocks(1000 + i, 1000 + i + step));
                }
            }
        }
        w.projects = vec![
            project(10, "A", None),
            project(11, "B", None),
            project(12, "C", None),
        ];
        let g = w.graph(GraphFilter::default()).unwrap();
        assert!(g.nodes.len() >= 90);
        for (i, a) in g.nodes.iter().enumerate() {
            for b in &g.nodes[i + 1..] {
                let overlap =
                    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "{} overlaps {}", a.label, b.label);
            }
        }
        assert!(g.edges.iter().all(|e| e.points.len() >= 2));
    }

    // ---------------------------------------------------------- projects

    #[test]
    fn the_project_level_draws_depends_on_with_status_and_lateness() {
        let mut paused = project(12, "Paused", None);
        paused.status = ProjectStatus::Paused;
        let mut shipped = project(13, "Shipped", None);
        shipped.status = ProjectStatus::Done;
        let w = World {
            people: vec![],
            health: HashMap::new(),
            // P10 is late: 5 days of work against a 2-day target.
            tasks: vec![task(1, "work", 5.0, Some(10))],
            projects: vec![
                project(10, "Core", Some(date!(2027 - 03 - 02))),
                project(11, "App", None),
                paused,
                shipped,
            ],
            edges: vec![
                edge(
                    EdgeType::DependsOn,
                    NodeType::Project,
                    11,
                    NodeType::Project,
                    10,
                ),
                edge(
                    EdgeType::DependsOn,
                    NodeType::Project,
                    12,
                    NodeType::Project,
                    11,
                ),
                edge(
                    EdgeType::DependsOn,
                    NodeType::Project,
                    11,
                    NodeType::Project,
                    13,
                ),
            ],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(g.level, GraphLevel::Projects);
        // Done projects are hidden by default, so the link to "Shipped" goes with it.
        assert_eq!(labels(&g), ["App", "Core", "Paused"]);
        assert_eq!(g.edges.len(), 2);
        // Needed projects are on the left of the ones that need them.
        assert!(by(&g, "Core").x < by(&g, "App").x && by(&g, "App").x < by(&g, "Paused").x);
        assert!(by(&g, "Core").late && !by(&g, "App").late);
        assert!(by(&g, "Paused").blocked);
        assert!(by(&g, "Core").subtitle.starts_with("Active · ends "));
        assert_eq!(by(&g, "Paused").node.node_type, NodeType::Project);
        let with_done = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                include_done: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(with_done.nodes.len(), 4);
    }

    #[test]
    fn the_project_level_filters_by_objective_and_focus() {
        let w = World {
            people: vec![],
            health: HashMap::new(),
            projects: vec![
                project(10, "A", None),
                project(11, "B", None),
                project(12, "C", None),
                project(13, "D", None),
            ],
            edges: vec![
                edge(
                    EdgeType::DependsOn,
                    NodeType::Project,
                    11,
                    NodeType::Project,
                    10,
                ),
                edge(
                    EdgeType::DependsOn,
                    NodeType::Project,
                    12,
                    NodeType::Project,
                    11,
                ),
                edge(
                    EdgeType::DependsOn,
                    NodeType::Project,
                    13,
                    NodeType::Project,
                    12,
                ),
                edge(
                    EdgeType::ContributesTo,
                    NodeType::Project,
                    11,
                    NodeType::Objective,
                    20,
                ),
            ],
            ..Default::default()
        };
        let obj = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                objective_id: Some(id(20)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&obj), ["A", "B", "C"]);
        assert!(by(&obj, "A").context && by(&obj, "C").context && !by(&obj, "B").context);
        let focus = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                project_id: Some(id(13)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(labels(&focus), ["C", "D"]);
    }

    // ------------------------------------------------------------------ spec 47

    fn person(n: u128, name: &str) -> Person {
        Person {
            id: id(n),
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: 40.0,
            is_self: false,
            notes: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    #[test]
    fn a_task_box_carries_who_what_and_when() {
        let mut a = task(1, "A", 2.0, Some(10));
        a.status = TaskStatus::InProgress;
        a.priority = 1;
        a.due_date = Some(date!(2027 - 03 - 02));
        a.task_type = Some("design".into());
        let w = World {
            tasks: vec![a, task(2, "B", 3.0, Some(10))],
            edges: vec![
                blocks(1, 2),
                edge(
                    EdgeType::AssignedTo,
                    NodeType::Task,
                    1,
                    NodeType::Person,
                    50,
                ),
            ],
            projects: vec![project(10, "P", None)],
            people: vec![person(50, "Priya")],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        let a = by(&g, "A");
        assert_eq!(a.task_status, Some(TaskStatus::InProgress));
        assert_eq!(a.priority, 1);
        assert_eq!(a.assignee.as_ref().map(|p| p.label.as_str()), Some("Priya"));
        assert_eq!(a.project_id, Some(id(10)));
        assert_eq!(a.due, Some(date!(2027 - 03 - 02)));
        assert_eq!(a.estimate_days, Some(2.0));
        assert_eq!(a.task_type.as_deref(), Some("design"));
        // Two days of work from Monday: it finishes on Tuesday, the day it is due, with no slack.
        assert_eq!(a.finish, Some(date!(2027 - 03 - 02)));
        assert_eq!(a.slack_days, Some(0.0));
        assert!(a.project_status.is_none() && a.health.is_none());
        // B has nobody and no due date.
        let b = by(&g, "B");
        assert!(b.assignee.is_none() && b.due.is_none());
        assert_eq!(b.task_status, Some(TaskStatus::Todo));
    }

    #[test]
    fn each_box_counts_what_it_waits_on_and_what_it_holds_up_through_any_number_of_links() {
        // A -> B, A -> C, B -> D, C -> D (a diamond), and D -> E.
        let w = World {
            tasks: vec![
                task(1, "A", 1.0, None),
                task(2, "B", 1.0, None),
                task(3, "C", 1.0, None),
                task(4, "D", 1.0, None),
                task(5, "E", 1.0, None),
            ],
            edges: vec![
                blocks(1, 2),
                blocks(1, 3),
                blocks(2, 4),
                blocks(3, 4),
                blocks(4, 5),
            ],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        let counts = |l: &str| (by(&g, l).upstream, by(&g, l).downstream);
        assert_eq!(counts("A"), (0, 4));
        assert_eq!(counts("B"), (1, 2));
        assert_eq!(counts("D"), (3, 1));
        assert_eq!(counts("E"), (4, 0));
    }

    #[test]
    fn a_link_between_projects_or_to_the_inbox_is_flagged_as_crossing() {
        let w = World {
            tasks: vec![
                task(1, "In P", 1.0, Some(10)),
                task(2, "Also in P", 1.0, Some(10)),
                task(3, "In Q", 1.0, Some(11)),
                task(4, "Inbox", 1.0, None),
            ],
            edges: vec![blocks(1, 2), blocks(2, 3), blocks(4, 1)],
            projects: vec![project(10, "P", None), project(11, "Q", None)],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        let crossing = |from: u128, to: u128| {
            g.edges
                .iter()
                .find(|e| e.from == id(from) && e.to == id(to))
                .unwrap()
                .cross_project
        };
        assert!(!crossing(1, 2));
        assert!(crossing(2, 3));
        assert!(crossing(4, 1));
    }

    #[test]
    fn a_dropped_unlinked_task_is_drawn_below_the_graph_and_not_listed() {
        let w = World {
            tasks: vec![
                task(1, "A", 1.0, None),
                task(2, "B", 1.0, None),
                task(3, "Loose", 1.0, None),
                task(4, "Other loose", 1.0, None),
            ],
            edges: vec![blocks(1, 2)],
            ..Default::default()
        };
        let plain = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(plain.hidden_unlinked, 2);
        let g = w
            .graph(GraphFilter {
                waiting: Some(id(3)),
                ..Default::default()
            })
            .unwrap();
        let waiting = g.nodes.iter().find(|n| n.node.id == id(3)).unwrap();
        assert!(waiting.waiting && !waiting.context);
        // Below everything else, centred under the graph, and no layer of its own.
        let lowest = g
            .nodes
            .iter()
            .filter(|n| n.node.id != id(3))
            .map(|n| n.y + n.h)
            .fold(0.0, f64::max);
        assert!(waiting.y > lowest, "{} {}", waiting.y, lowest);
        assert!((waiting.x + waiting.w / 2.0 - g.width / 2.0).abs() < 1.0);
        assert!(g.height >= waiting.y + waiting.h);
        // It is no longer an unlinked item to list, and nothing else changed.
        assert_eq!(g.hidden_unlinked, 1);
        assert_eq!(g.unlinked.len(), 1);
        assert_eq!(g.unlinked[0].node.id, id(4));
        assert_eq!(g.nodes.len(), plain.nodes.len() + 1);
        assert!(g.edges.len() == plain.edges.len());
        // The others are laid out exactly as before.
        for n in &plain.nodes {
            let same = g.nodes.iter().find(|m| m.node.id == n.node.id).unwrap();
            assert_eq!((same.x, same.y), (n.x, n.y));
        }
    }

    #[test]
    fn a_dropped_task_on_an_empty_graph_is_the_only_box() {
        let w = World {
            tasks: vec![task(1, "Loose", 1.0, None), task(2, "Other", 1.0, None)],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                waiting: Some(id(1)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(g.nodes.len(), 1);
        assert_eq!((g.nodes[0].x, g.nodes[0].y), (0.0, 0.0));
        assert!(g.nodes[0].waiting);
        assert_eq!(g.hidden_unlinked, 1);
        assert!(g.edges.is_empty());
    }

    #[test]
    fn once_the_dropped_task_is_linked_it_is_an_ordinary_box() {
        let w = World {
            tasks: vec![task(1, "A", 1.0, None), task(2, "B", 1.0, None)],
            edges: vec![blocks(1, 2)],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                waiting: Some(id(2)),
                ..Default::default()
            })
            .unwrap();
        // It sits in its layer, to the right of what blocks it.
        let a = g.nodes.iter().find(|n| n.node.id == id(1)).unwrap();
        let b = g.nodes.iter().find(|n| n.node.id == id(2)).unwrap();
        assert!(b.x > a.x && b.layer == 1);
        assert_eq!(g.nodes.len(), 2);
    }

    #[test]
    fn a_dropped_item_the_graph_would_not_draw_is_ignored() {
        let mut done = task(9, "Done", 1.0, None);
        done.status = TaskStatus::Done;
        let w = World {
            tasks: vec![task(1, "A", 1.0, None), task(2, "B", 1.0, None), done],
            edges: vec![blocks(1, 2)],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                waiting: Some(id(9)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert!(g.nodes.iter().all(|n| !n.waiting));
        // So is an id that is nothing at all.
        let g = w
            .graph(GraphFilter {
                waiting: Some(id(77)),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(g.nodes.len(), 2);
    }

    #[test]
    fn a_dropped_project_waits_below_the_graph_too() {
        let w = World {
            projects: vec![
                project(10, "P1", None),
                project(11, "P2", None),
                project(12, "P3", None),
            ],
            edges: vec![edge(
                EdgeType::DependsOn,
                NodeType::Project,
                11,
                NodeType::Project,
                10,
            )],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                waiting: Some(id(12)),
                ..Default::default()
            })
            .unwrap();
        let waiting = g.nodes.iter().find(|n| n.node.id == id(12)).unwrap();
        assert!(waiting.waiting);
        assert!(g
            .nodes
            .iter()
            .filter(|n| n.node.id != id(12))
            .all(|n| n.y < waiting.y));
        assert_eq!(g.hidden_unlinked, 0);
    }

    #[test]
    fn an_unlinked_entry_carries_the_facts_of_the_box_it_would_be() {
        let w = World {
            tasks: vec![
                task(1, "Blocker", 1.0, None),
                task(2, "Solo", 2.0, Some(10)),
                task(3, "Next", 1.0, None),
            ],
            edges: vec![blocks(1, 3)],
            projects: vec![project(10, "P", Some(MON + time::Duration::days(30)))],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(g.unlinked.len(), 1);
        let look = &g.unlinked[0].look;
        assert_eq!(look.node, g.unlinked[0].node);
        assert_eq!(look.label, "Solo");
        assert_eq!(look.task_status, Some(TaskStatus::Todo));
        assert_eq!(look.priority, 3);
        assert_eq!(look.estimate_days, Some(2.0));
        assert_eq!(look.project_id, Some(id(10)));
        // It has no place and no links of its own.
        assert_eq!((look.x, look.y, look.layer), (0.0, 0.0, 0));
        assert_eq!((look.upstream, look.downstream), (0, 0));
        // It is the same box it would be if it were drawn.
        let drawn = w
            .graph(GraphFilter {
                include_isolated: true,
                ..Default::default()
            })
            .unwrap();
        let same = drawn.nodes.iter().find(|n| n.node.id == id(2)).unwrap();
        assert_eq!(
            (
                same.task_status,
                same.priority,
                same.estimate_days,
                &same.subtitle
            ),
            (
                look.task_status,
                look.priority,
                look.estimate_days,
                &look.subtitle
            )
        );
    }

    #[test]
    fn many_unlinked_nodes_are_all_listed_so_a_search_can_find_them() {
        let tasks: Vec<Task> = (1..=200)
            .map(|n| task(n, &format!("T{n:03}"), 1.0, None))
            .collect();
        let w = World {
            tasks,
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(g.hidden_unlinked, 200);
        assert_eq!(g.unlinked.len(), 200);
        const { assert!(MAX_UNLINKED_LISTED >= 200) };
    }

    #[test]
    fn unlinked_work_is_listed_by_name_instead_of_only_counted() {
        let w = World {
            tasks: vec![
                task(1, "A", 1.0, None),
                task(2, "B", 1.0, None),
                task(3, "zeta", 1.0, None),
                task(4, "Alone", 1.0, Some(10)),
            ],
            edges: vec![blocks(1, 2)],
            projects: vec![project(10, "P", None)],
            ..Default::default()
        };
        let g = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(g.hidden_unlinked, 2);
        let names: Vec<&str> = g.unlinked.iter().map(|u| u.label.as_str()).collect();
        assert_eq!(names, ["Alone", "zeta"]);
        assert!(g.unlinked[0].subtitle.contains('P'));
        assert_eq!(g.unlinked[0].node.node_type, NodeType::Task);
        // Drawn instead when asked for: nothing is left out.
        let all = w
            .graph(GraphFilter {
                include_isolated: true,
                ..Default::default()
            })
            .unwrap();
        assert!(all.unlinked.is_empty() && all.hidden_unlinked == 0);
    }

    #[test]
    fn a_project_box_carries_health_owner_zone_progress_and_the_room_to_the_next_project() {
        // Q depends on P. P's three days of work end on Wednesday; Q can't start until next
        // Monday, so there are 2 working days of room between them.
        let mut q_work = task(2, "Q work", 1.0, Some(11));
        q_work.start_date = Some(date!(2027 - 03 - 08));
        let mut done = task(3, "Done already", 1.0, Some(10));
        done.status = TaskStatus::Done;
        done.completed_at = Some(OffsetDateTime::UNIX_EPOCH);
        let mut p = project(10, "P", Some(date!(2027 - 03 - 12)));
        p.owner_person_id = Some(id(50));
        let w = World {
            tasks: vec![task(1, "P work", 3.0, Some(10)), q_work, done],
            edges: vec![edge(
                EdgeType::DependsOn,
                NodeType::Project,
                11,
                NodeType::Project,
                10,
            )],
            projects: vec![p, project(11, "Q", None)],
            people: vec![person(50, "Priya")],
            health: HashMap::from([(id(10), HealthLevel::Green), (id(11), HealthLevel::Red)]),
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                ..Default::default()
            })
            .unwrap();
        let p = by(&g, "P");
        assert_eq!(p.project_status, Some(ProjectStatus::Active));
        assert_eq!(p.health, Some(HealthLevel::Green));
        assert_eq!(p.assignee.as_ref().map(|a| a.label.as_str()), Some("Priya"));
        assert_eq!(p.progress, Some((1, 2)));
        assert_eq!(p.due, Some(date!(2027 - 03 - 12)));
        assert_eq!(p.finish, Some(date!(2027 - 03 - 03)));
        assert!(matches!(p.zone, Zone::Inside { room: 7, .. }));
        assert_eq!(by(&g, "Q").health, Some(HealthLevel::Red));
        assert!(by(&g, "Q").assignee.is_none());
        // The arrow says there are 2 days of room, and does not call itself a task link.
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].room_days, Some(2));
        assert!(!g.edges[0].cross_project);
        // Needed first, so on the left.
        assert!(by(&g, "P").x < by(&g, "Q").x);
    }

    #[test]
    fn one_box_can_be_laid_out_large_and_the_others_make_room() {
        // A -> B, A -> C, B -> D, C -> D.
        let w = World {
            tasks: vec![
                task(1, "A", 1.0, None),
                task(2, "B", 1.0, None),
                task(3, "C", 1.0, None),
                task(4, "D", 1.0, None),
            ],
            edges: vec![blocks(1, 2), blocks(1, 3), blocks(2, 4), blocks(3, 4)],
            ..Default::default()
        };
        let plain = w.graph(GraphFilter::default()).unwrap();
        assert!(plain.nodes.iter().all(|n| !n.expanded));
        assert_eq!(by(&plain, "B").h, NODE_H);

        let g = w
            .graph(GraphFilter {
                expanded: Some(id(2)),
                ..Default::default()
            })
            .unwrap();
        // Only B is large, and it says so.
        let b = by(&g, "B");
        assert!(b.expanded);
        assert_eq!((b.w, b.h), (EXPANDED_W, EXPANDED_TASK_H));
        assert_eq!(g.nodes.iter().filter(|n| n.expanded).count(), 1);
        // Nothing overlaps it: C, in the same column, keeps its distance.
        let c = by(&g, "C");
        assert_eq!(b.layer, c.layer);
        let apart = c.y >= b.y + b.h || b.y >= c.y + c.h;
        assert!(
            apart,
            "B ({}..{}) and C ({}..{})",
            b.y,
            b.y + b.h,
            c.y,
            c.y + c.h
        );
        // The column to its right starts after the wider box.
        assert!(by(&g, "D").x >= b.x + b.w);
        // The same boxes and links are drawn; only the sizes differ.
        assert_eq!(labels(&g), labels(&plain));
        assert_eq!(g.edges.len(), plain.edges.len());
        // Asking for a box that isn't there changes nothing.
        let none = w
            .graph(GraphFilter {
                expanded: Some(id(99)),
                ..Default::default()
            })
            .unwrap();
        assert!(none.nodes.iter().all(|n| !n.expanded));
        assert_eq!(by(&none, "B").h, NODE_H);
    }

    #[test]
    fn an_expanded_project_box_is_a_little_smaller_than_a_task_box() {
        let w = World {
            tasks: vec![
                task(1, "P work", 1.0, Some(10)),
                task(2, "Q work", 1.0, Some(11)),
            ],
            edges: vec![edge(
                EdgeType::DependsOn,
                NodeType::Project,
                11,
                NodeType::Project,
                10,
            )],
            projects: vec![project(10, "P", None), project(11, "Q", None)],
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                level: GraphLevel::Projects,
                expanded: Some(id(10)),
                ..Default::default()
            })
            .unwrap();
        let p = by(&g, "P");
        assert!(p.expanded);
        assert_eq!((p.w, p.h), (EXPANDED_W, EXPANDED_PROJECT_H));
        const { assert!(EXPANDED_PROJECT_H < EXPANDED_TASK_H) };
        assert!(!by(&g, "Q").expanded);
    }
    #[test]
    fn my_view_shows_my_tasks_what_reaches_them_and_the_one_hop_context() {
        use minimap_types::{MyScope, Why};
        // 1 -> 2 -> 3 -> 4, and an unrelated 5 -> 6. In my view: 2 and 3.
        let w = World {
            tasks: (1..=6)
                .map(|n| task(n, &format!("T{n}"), 1.0, None))
                .collect(),
            edges: vec![blocks(1, 2), blocks(2, 3), blocks(3, 4), blocks(5, 6)],
            scope: Some(MyScope {
                tasks: [2u128, 3]
                    .iter()
                    .map(|n| (id(*n), vec![Why::Assigned]))
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let g = w
            .graph(GraphFilter {
                mine: true,
                ..Default::default()
            })
            .unwrap();
        let shown: Vec<(u128, bool)> = g
            .nodes
            .iter()
            .map(|n| (n.node.id.as_u128(), n.context))
            .collect();
        let mut sorted = shown.clone();
        sorted.sort();
        assert_eq!(sorted, [(1, true), (2, false), (3, false), (4, true)]);
        // Without the filter everything linked is there; with no scope, nothing is mine.
        let all = w.graph(GraphFilter::default()).unwrap();
        assert_eq!(all.nodes.len(), 6);
        let none = World {
            scope: None,
            tasks: w.tasks.clone(),
            edges: w.edges.clone(),
            ..Default::default()
        }
        .graph(GraphFilter {
            mine: true,
            ..Default::default()
        })
        .unwrap();
        assert!(none.nodes.is_empty());
    }

    fn relates(a: u128, b: u128) -> Edge {
        edge(EdgeType::RelatesTo, NodeType::Task, a, NodeType::Task, b)
    }

    /// Stored sub-task -> parent.
    fn subtask(sub: u128, parent: u128) -> Edge {
        edge(
            EdgeType::SubtaskOf,
            NodeType::Task,
            sub,
            NodeType::Task,
            parent,
        )
    }

    fn world(edges: Vec<Edge>) -> World {
        World {
            tasks: vec![
                task(1, "A", 2.0, Some(10)),
                task(2, "B", 3.0, Some(10)),
                task(3, "C", 1.0, Some(10)),
                task(4, "D", 1.0, Some(10)),
            ],
            edges,
            projects: vec![project(10, "P", None)],
            ..Default::default()
        }
    }

    fn kind_of(g: &DependencyGraph, a: &str, b: &str) -> Option<GraphEdgeKind> {
        let (a, b) = (by(g, a).node.id, by(g, b).node.id);
        g.edges
            .iter()
            .find(|e| (e.from, e.to) == (a, b) || (e.from, e.to) == (b, a))
            .map(|e| e.kind)
    }

    #[test]
    fn related_and_part_of_links_are_drawn_and_make_their_tasks_linked() {
        let g = world(vec![blocks(1, 2), relates(2, 3), subtask(4, 1)])
            .graph(GraphFilter::default())
            .unwrap();
        // All four tasks are linked now (C only by "related", D only by "part of").
        assert_eq!(labels(&g), ["A", "B", "C", "D"]);
        assert_eq!(g.hidden_unlinked, 0);
        assert_eq!(g.edges.len(), 3);
        assert_eq!(kind_of(&g, "A", "B"), Some(GraphEdgeKind::Blocks));
        assert_eq!(kind_of(&g, "B", "C"), Some(GraphEdgeKind::RelatesTo));
        assert_eq!(kind_of(&g, "A", "D"), Some(GraphEdgeKind::Subtask));
        // A part-of link is drawn from the parent to the sub-task, left to right.
        let part = g
            .edges
            .iter()
            .find(|e| e.kind == GraphEdgeKind::Subtask)
            .unwrap();
        assert_eq!((part.from, part.to), (id(1), id(4)));
        assert!(!part.reversed);
        assert!(by(&g, "A").x < by(&g, "D").x);
        // Without them C and D would have been left out.
        let only_blocks = world(vec![blocks(1, 2)])
            .graph(GraphFilter::default())
            .unwrap();
        assert_eq!(only_blocks.hidden_unlinked, 2);
    }

    #[test]
    fn only_the_dependencies_count_for_waiting_holding_up_lag_and_the_critical_path() {
        let g = world(vec![blocks(1, 2), relates(2, 3), subtask(3, 4)])
            .graph(GraphFilter::default())
            .unwrap();
        // C is next to B and D next to C, but nothing waits on anything because of them.
        assert_eq!((by(&g, "C").upstream, by(&g, "C").downstream), (0, 0));
        assert_eq!((by(&g, "D").upstream, by(&g, "D").downstream), (0, 0));
        assert_eq!((by(&g, "B").upstream, by(&g, "A").downstream), (1, 1));
        assert!(g
            .edges
            .iter()
            .filter(|e| !e.kind.orders_work())
            .all(|e| !e.critical && e.lag_days.is_none()));
    }

    #[test]
    fn a_link_that_would_close_a_loop_is_drawn_the_other_way_round() {
        // C blocks D, and C is a sub-task of D. A parent is drawn before its sub-task, but D
        // can't come before C, so the part-of link is drawn C -> D and says it is reversed.
        let g = world(vec![blocks(3, 4), subtask(3, 4)])
            .graph(GraphFilter::default())
            .unwrap();
        let part = g
            .edges
            .iter()
            .find(|e| e.kind == GraphEdgeKind::Subtask)
            .unwrap();
        assert!(by(&g, "C").x < by(&g, "D").x);
        assert_eq!((part.from, part.to), (id(3), id(4)));
        assert!(part.reversed);
        // A related link between the same two tasks is just placed with them.
        let g = world(vec![blocks(3, 4), relates(4, 3)])
            .graph(GraphFilter::default())
            .unwrap();
        let rel = g
            .edges
            .iter()
            .find(|e| e.kind == GraphEdgeKind::RelatesTo)
            .unwrap();
        assert_eq!((rel.from, rel.to), (id(3), id(4)));
    }

    #[test]
    fn two_links_between_the_same_tasks_are_both_drawn() {
        let g = world(vec![
            blocks(1, 2),
            relates(1, 2),
            relates(2, 1),
            subtask(2, 1),
        ])
        .graph(GraphFilter::default())
        .unwrap();
        assert_eq!(g.edges.len(), 4);
        assert!(g.edges.iter().all(|e| e.points.len() >= 2));
        assert!(by(&g, "A").x < by(&g, "B").x);
    }

    #[test]
    fn filters_pull_in_related_tasks_as_context() {
        let mut w = world(vec![relates(1, 2), subtask(3, 1)]);
        w.tasks[1].project_id = Some(id(11));
        w.tasks[2].project_id = Some(id(11));
        w.projects.push(project(11, "Q", None));
        let g = w
            .graph(GraphFilter {
                project_id: Some(id(10)),
                ..Default::default()
            })
            .unwrap();
        // A is in P; B and C (in Q) are shown dimmed because they are linked to A.
        assert!(!by(&g, "A").context && by(&g, "B").context && by(&g, "C").context);
        assert!(g.edges.iter().all(|e| e.cross_project));
    }
}
