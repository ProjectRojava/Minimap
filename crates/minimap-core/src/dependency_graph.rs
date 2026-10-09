//! The dependency graph (spec 18): which work blocks which, laid out left to right. Pure.
//!
//! Two levels: **tasks** joined by `blocks`, and **projects** joined by `depends_on`. Filters
//! (project, team, objective) choose the work of interest; the nodes it is linked to (one hop)
//! are shown dimmed as *context* so no link dangles. Unlinked nodes are left out unless asked
//! for. The critical path comes from the schedule (spec 13): critical tasks and the links that
//! drive the finish are marked.

use std::collections::{HashMap, HashSet};

use minimap_types::{
    Date, DependencyGraph, Edge, EdgeType, GraphEdge, GraphFilter, GraphLevel, GraphNode, NodeRef,
    NodeType, Project, ProjectStatus, ScheduleScope, ScheduledTask, Task, TaskStatus, Team, Uuid,
    WorkWeek,
};

use crate::{
    layout::{layout, LayoutError, Params},
    schedule,
};

/// Most nodes one graph shows; beyond this the filters should narrow it.
pub const MAX_NODES: usize = 400;
const NODE_W: f64 = 210.0;
const NODE_H: f64 = 38.0;
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
    pub today: Date,
    pub work_week: WorkWeek,
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
}

struct DraftEdge {
    id: Uuid,
    from: Uuid,
    to: Uuid,
    lag_days: Option<u32>,
    critical: bool,
}

fn active(e: &Edge, kind: EdgeType) -> bool {
    e.edge_type == kind && e.archived_at.is_none()
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
    let links: Vec<&Edge> = input
        .edges
        .iter()
        .filter(|e| active(e, EdgeType::Blocks))
        .filter(|e| candidates.contains_key(&e.from_id) && candidates.contains_key(&e.to_id))
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
    let filtered =
        filter.project_id.is_some() || filter.team_id.is_some() || filter.objective_id.is_some();

    // Seeds, plus (when filtering) what they are linked to as context.
    let mut visible: HashSet<Uuid> = seeds.clone();
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
        // Two pieces of context aren't worth drawing a link between.
        .filter(|e| seeds.contains(&e.from_id) || seeds.contains(&e.to_id))
        .collect();
    let linked: HashSet<Uuid> = edges.iter().flat_map(|e| [e.from_id, e.to_id]).collect();
    let mut hidden_unlinked = 0;
    if !filter.include_isolated {
        visible.retain(|id| {
            let keep = linked.contains(id);
            if !keep {
                hidden_unlinked += 1;
            }
            keep
        });
    }
    if visible.len() > MAX_NODES {
        return Err(GraphError::TooLarge(visible.len()));
    }

    let mut ids: Vec<Uuid> = visible.into_iter().collect();
    ids.sort_by(|a, b| {
        let (ta, tb) = (candidates[a], candidates[b]);
        ta.title
            .to_lowercase()
            .cmp(&tb.title.to_lowercase())
            .then(a.cmp(b))
    });
    let held_up = held_up_by_blocked(input);
    let drafts: Vec<Draft> = ids
        .iter()
        .map(|id| {
            let t = candidates[id];
            let s = scheduled.get(id);
            let project = t
                .project_id
                .and_then(|p| project_title.get(&p).copied())
                .unwrap_or("Inbox");
            let subtitle = match s {
                Some(s) if !s.done => format!("→ {} · {project}", s.finish),
                _ => project.to_owned(),
            };
            Draft {
                node: NodeRef::new(NodeType::Task, *id),
                label: t.title.clone(),
                subtitle,
                context: !seeds.contains(id),
                done: t.status == TaskStatus::Done,
                critical: s.is_some_and(|s| s.critical),
                late: s.is_some_and(|s| s.slack_days < -EPS),
                blocked: t.status == TaskStatus::Blocked,
                held_up: held_up.contains(id),
            }
        })
        .collect();
    let draft_edges: Vec<DraftEdge> = edges
        .iter()
        .map(|e| {
            let lag = lag_of(e).unwrap_or(0);
            let critical = match (scheduled.get(&e.from_id), scheduled.get(&e.to_id)) {
                (Some(a), Some(b)) => {
                    a.critical && b.critical && b.es - (a.ef + f64::from(lag)) <= EPS
                }
                _ => false,
            };
            DraftEdge {
                id: e.id,
                from: e.from_id,
                to: e.to_id,
                lag_days: lag_of(e),
                critical,
            }
        })
        .collect();
    finish(
        GraphLevel::Tasks,
        drafts,
        draft_edges,
        hidden_unlinked,
        Vec::new(),
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
    let filtered = filter.project_id.is_some() || filter.objective_id.is_some();
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
    let mut hidden_unlinked = 0;
    if !filter.include_isolated {
        visible.retain(|id| {
            let keep = linked.contains(id);
            if !keep {
                hidden_unlinked += 1;
            }
            keep
        });
    }
    if visible.len() > MAX_NODES {
        return Err(GraphError::TooLarge(visible.len()));
    }
    let mut ids: Vec<Uuid> = visible.into_iter().collect();
    ids.sort_by(|a, b| {
        candidates[a]
            .title
            .to_lowercase()
            .cmp(&candidates[b].title.to_lowercase())
            .then(a.cmp(b))
    });
    let drafts: Vec<Draft> = ids
        .iter()
        .map(|id| {
            let p = candidates[id];
            let f = forecast.get(id);
            let status = crate::projects::status_title(p.status);
            let subtitle = match f.and_then(|f| f.projected_finish) {
                Some(d) if p.status != ProjectStatus::Done => format!("{status} · ends {d}"),
                _ => status.to_owned(),
            };
            Draft {
                node: NodeRef::new(NodeType::Project, *id),
                label: p.title.clone(),
                subtitle,
                context: !seeds.contains(id),
                done: p.status == ProjectStatus::Done,
                critical: false,
                late: f.is_some_and(|f| f.late_by_days.is_some()),
                blocked: p.status == ProjectStatus::Paused,
                held_up: false,
            }
        })
        .collect();
    let draft_edges: Vec<DraftEdge> = edges
        .iter()
        .map(|e| DraftEdge {
            id: e.id,
            // `A depends_on B` is drawn B -> A, so what is needed first is on the left.
            from: e.to_id,
            to: e.from_id,
            lag_days: None,
            critical: false,
        })
        .collect();
    finish(
        GraphLevel::Projects,
        drafts,
        draft_edges,
        hidden_unlinked,
        warnings,
    )
}

fn finish(
    level: GraphLevel,
    drafts: Vec<Draft>,
    edges: Vec<DraftEdge>,
    hidden_unlinked: u32,
    warnings: Vec<String>,
) -> Result<DependencyGraph, GraphError> {
    let index: HashMap<Uuid, usize> = drafts
        .iter()
        .enumerate()
        .map(|(i, d)| (d.node.id, i))
        .collect();
    let pairs: Vec<(usize, usize)> = edges
        .iter()
        .map(|e| (index[&e.from], index[&e.to]))
        .collect();
    let placed = layout(
        &vec![(NODE_W, NODE_H); drafts.len()],
        &pairs,
        &Params::default(),
    )?;
    let nodes: Vec<GraphNode> = drafts
        .into_iter()
        .zip(&placed.nodes)
        .map(|(d, p)| GraphNode {
            node: d.node,
            label: d.label,
            subtitle: d.subtitle,
            x: p.x,
            y: p.y,
            w: p.w,
            h: p.h,
            layer: p.layer as u32,
            context: d.context,
            done: d.done,
            critical: d.critical,
            late: d.late,
            blocked: d.blocked,
            held_up: d.held_up,
        })
        .collect();
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
        })
        .collect();
    Ok(DependencyGraph {
        level,
        nodes,
        edges,
        width: placed.width,
        height: placed.height,
        critical_nodes,
        hidden_unlinked,
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
            links: Vec::new(),
            task_type: None,
            focus: None,
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
    }

    impl World {
        fn graph(&self, filter: GraphFilter) -> Result<DependencyGraph, GraphError> {
            build(
                &GraphInput {
                    tasks: &self.tasks,
                    edges: &self.edges,
                    projects: &self.projects,
                    teams: &self.teams,
                    today: MON,
                    work_week: WorkWeek::MON_FRI,
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
}
