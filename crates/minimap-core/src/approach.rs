//! Approaching (spec 45): trouble elsewhere that will reach what you are looking at, through `blocks`
//! and `depends_on` links. Pure; it reads one portfolio schedule and stores nothing.
//!
//! Looking at a project (or your own week, or an objective), the *scope* is the open tasks (and
//! projects) that belong to it. A *source* is an open task outside it that is overdue, blocked,
//! scheduled to finish after its due date or without slack, or a project outside it that is late or
//! red. From each task in scope the walk goes back along `blocks` links through open tasks, at
//! most [`MAX_HOPS`] of them, and every source found is reported with the shortest chain to the
//! task it reaches. A chain stops at the first task that is back inside the scope (that task is a
//! start of its own). Projects reach projects the same way through `depends_on`.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use minimap_types::{
    ApproachGroup, ApproachReport, ApproachScope, Approaching, Date, Edge, EdgeType, Headline,
    NodeRef, NodeSummary, NodeType, Project, ProjectForecast, ProjectStatus, Reason, Schedule,
    ScheduledTask, Severity, Task, TaskStatus, Uuid, MAX_HOPS, THREAT_SLACK,
};

/// How many `depends_on` links a chain between projects may have.
const MAX_PROJECT_HOPS: usize = 3;
const EPS: f64 = 1e-6;

/// Everything `approaching` reads.
pub struct ApproachWorld<'a> {
    /// The active tasks.
    pub tasks: &'a [Task],
    /// The active links (of every type; the ones that matter are picked out).
    pub edges: &'a [Edge],
    /// The active projects.
    pub projects: &'a [Project],
    /// The schedule of every open task (`ScheduleScope::Portfolio`).
    pub schedule: &'a Schedule,
    /// The projects whose computed health is red.
    pub red_projects: &'a HashSet<Uuid>,
    /// The "me" person, for the `Me` scope.
    pub self_id: Option<Uuid>,
    pub today: Date,
}

fn is_open(t: &Task) -> bool {
    t.archived_at.is_none()
        && !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
        && !t.is_meeting()
}

fn summary(node_type: NodeType, id: Uuid, label: &str) -> NodeSummary {
    NodeSummary {
        node: NodeRef::new(node_type, id),
        label: label.to_owned(),
        archived: false,
    }
}

/// What belongs to the thing being looked at.
struct Inside {
    /// Projects whose open tasks are all inside.
    task_projects: HashSet<Uuid>,
    /// Tasks that are inside whatever their project is (assigned to me; feeding an objective).
    tasks: HashSet<Uuid>,
    /// Projects that trouble can reach through `depends_on`.
    targets: HashSet<Uuid>,
}

impl Inside {
    fn has(&self, t: &Task) -> bool {
        self.tasks.contains(&t.id)
            || t.project_id
                .is_some_and(|p| self.task_projects.contains(&p))
    }
}

fn inside_of(w: &ApproachWorld, scope: ApproachScope) -> Option<Inside> {
    let edges = || w.edges.iter().filter(|e| e.archived_at.is_none());
    match scope {
        ApproachScope::Project(p) => Some(Inside {
            task_projects: HashSet::from([p]),
            tasks: HashSet::new(),
            targets: HashSet::from([p]),
        }),
        ApproachScope::Me => {
            let me = w.self_id?;
            Some(Inside {
                task_projects: HashSet::new(),
                tasks: edges()
                    .filter(|e| e.edge_type == EdgeType::AssignedTo && e.to_id == me)
                    .map(|e| e.from_id)
                    .collect(),
                targets: w
                    .projects
                    .iter()
                    .filter(|p| p.owner_person_id == Some(me))
                    .map(|p| p.id)
                    .collect(),
            })
        }
        ApproachScope::Objective(o) => {
            let feeds: Vec<&Edge> = edges()
                .filter(|e| e.edge_type == EdgeType::ContributesTo && e.to_id == o)
                .collect();
            let projects: HashSet<Uuid> = feeds
                .iter()
                .filter(|e| e.from_type == NodeType::Project)
                .map(|e| e.from_id)
                .collect();
            Some(Inside {
                task_projects: projects.clone(),
                tasks: feeds
                    .iter()
                    .filter(|e| e.from_type == NodeType::Task)
                    .map(|e| e.from_id)
                    .collect(),
                targets: projects,
            })
        }
    }
}

/// Whole working days from today until a task is due to start: 0 when it is under way, blocked or
/// its start is past.
fn arrives_in(task: &Task, scheduled: &ScheduledTask) -> u32 {
    if matches!(task.status, TaskStatus::InProgress | TaskStatus::Blocked) {
        0
    } else {
        (scheduled.es + EPS).floor().max(0.0) as u32
    }
}

/// Why an open task outside the scope is in trouble (in the order of `Reason`), and how many days
/// past its due date it is when it is overdue.
fn task_trouble(
    task: &Task,
    scheduled: Option<&ScheduledTask>,
    today: Date,
) -> Option<(Vec<Reason>, Option<u32>)> {
    let mut reasons = Vec::new();
    let mut overdue = None;
    if let Some(due) = task.due_date.filter(|d| *d < today) {
        reasons.push(Reason::Overdue);
        overdue = Some((today - due).whole_days().max(1) as u32);
    }
    if task.status == TaskStatus::Blocked {
        reasons.push(Reason::Blocked);
    }
    if let Some(s) = scheduled {
        if task.due_date.is_some_and(|d| s.finish > d) {
            reasons.push(Reason::Late);
        }
        if s.slack_days < -EPS {
            reasons.push(Reason::NoSlack);
        }
    }
    (!reasons.is_empty()).then_some((reasons, overdue))
}

/// How an approaching task chain matters to the task it reaches.
fn task_severity(target: &ScheduledTask, due: Option<Date>, path: &[Uuid]) -> (Severity, i32) {
    let slack = (target.slack_days + EPS).floor() as i32;
    let late = target.slack_days < -EPS || due.is_some_and(|d| target.finish > d);
    // The link into the target is on its driving chain when it is the one that sets its start.
    let drives = path
        .len()
        .checked_sub(2)
        .and_then(|i| path.get(i))
        .is_some_and(|before| target.driver == Some(*before));
    let severity = if late && drives {
        Severity::Hits
    } else if slack <= THREAT_SLACK {
        Severity::Threatens
    } else {
        Severity::Watch
    };
    (severity, slack)
}

fn project_severity(forecast: Option<&ProjectForecast>) -> (Severity, Option<i32>) {
    let Some(f) = forecast else {
        return (Severity::Watch, None);
    };
    if f.late_by_days.is_some() {
        return (Severity::Hits, None);
    }
    let spare = match (f.target_offset, f.finish_offset) {
        (Some(t), Some(fin)) => Some((t - fin + EPS).floor().max(0.0) as i32),
        _ => None,
    };
    match spare {
        Some(s) if s <= THREAT_SLACK => (Severity::Threatens, Some(s)),
        other => (Severity::Watch, other),
    }
}

/// The trouble coming at what is being looked at. `ApproachScope::Me` with no "me" person, or a
/// scope with nothing in it, gives an empty report.
pub fn approaching(w: &ApproachWorld, scope: ApproachScope) -> ApproachReport {
    let Some(inside) = inside_of(w, scope) else {
        return ApproachReport::default();
    };
    let scheduled: HashMap<Uuid, &ScheduledTask> =
        w.schedule.tasks.iter().map(|t| (t.id, t)).collect();
    let forecasts: HashMap<Uuid, &ProjectForecast> = w
        .schedule
        .projects
        .iter()
        .map(|f| (f.project_id, f))
        .collect();
    let task_by_id: HashMap<Uuid, &Task> = w.tasks.iter().map(|t| (t.id, t)).collect();
    let project_by_id: HashMap<Uuid, &Project> = w.projects.iter().map(|p| (p.id, p)).collect();

    // Who blocks whom, and who depends on whom: sorted, so the answer never wobbles.
    let mut blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut needs: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in w.edges.iter().filter(|e| e.archived_at.is_none()) {
        match e.edge_type {
            EdgeType::Blocks => blockers.entry(e.to_id).or_default().push(e.from_id),
            EdgeType::DependsOn => needs.entry(e.from_id).or_default().push(e.to_id),
            _ => {}
        }
    }
    for list in blockers.values_mut().chain(needs.values_mut()) {
        list.sort();
        list.dedup();
    }

    let mut items: Vec<(Option<Uuid>, Approaching)> = Vec::new();

    // ------------------------------------------------------------------ task chains
    let mut starts: Vec<&Task> = w
        .tasks
        .iter()
        .filter(|t| is_open(t) && inside.has(t))
        .collect();
    starts.sort_by_key(|t| t.id);
    for target in starts {
        let Some(target_sched) = scheduled.get(&target.id) else {
            continue;
        };
        let mut seen: HashSet<Uuid> = HashSet::from([target.id]);
        // Each entry: the task and the chain from it to the target (itself first).
        let mut queue: VecDeque<(Uuid, Vec<Uuid>)> = VecDeque::from([(target.id, vec![target.id])]);
        while let Some((current, to_target)) = queue.pop_front() {
            if to_target.len() > MAX_HOPS {
                continue;
            }
            for &pred in blockers.get(&current).into_iter().flatten() {
                if !seen.insert(pred) {
                    continue;
                }
                let Some(task) = task_by_id.get(&pred).filter(|t| is_open(t)) else {
                    continue; // finished work (or a meeting) breaks the chain
                };
                if inside.has(task) {
                    continue; // back inside: that task is a start of its own
                }
                let mut path = vec![pred];
                path.extend(&to_target);
                if let Some((reasons, overdue_days)) =
                    task_trouble(task, scheduled.get(&pred).copied(), w.today)
                {
                    let (severity, slack) = task_severity(target_sched, target.due_date, &path);
                    let label_of = |id: &Uuid| {
                        task_by_id
                            .get(id)
                            .map_or_else(String::new, |t| t.title.clone())
                    };
                    items.push((
                        task.project_id,
                        Approaching {
                            source: summary(NodeType::Task, pred, &task.title),
                            target: summary(NodeType::Task, target.id, &target.title),
                            path: path
                                .iter()
                                .map(|id| summary(NodeType::Task, *id, &label_of(id)))
                                .collect(),
                            reasons,
                            overdue_days,
                            arrives_in: arrives_in(target, target_sched),
                            severity,
                            target_slack: Some(slack),
                        },
                    ));
                }
                queue.push_back((pred, path));
            }
        }
    }

    // --------------------------------------------------------------- project chains
    let mut targets: Vec<Uuid> = inside.targets.iter().copied().collect();
    targets.sort();
    for target in targets {
        let Some(target_project) = project_by_id.get(&target) else {
            continue;
        };
        // The target project's first open task starts the clock.
        let arrives = w
            .schedule
            .tasks
            .iter()
            .filter(|t| t.project_id == Some(target) && !t.done)
            .map(|t| (t.es + EPS).floor().max(0.0) as u32)
            .min()
            .unwrap_or(0);
        let (severity, slack) = project_severity(forecasts.get(&target).copied());
        let mut seen: HashSet<Uuid> = HashSet::from([target]);
        let mut queue: VecDeque<(Uuid, Vec<Uuid>)> = VecDeque::from([(target, vec![target])]);
        while let Some((current, to_target)) = queue.pop_front() {
            if to_target.len() > MAX_PROJECT_HOPS {
                continue;
            }
            for &q in needs.get(&current).into_iter().flatten() {
                if !seen.insert(q) {
                    continue;
                }
                let Some(project) = project_by_id.get(&q) else {
                    continue;
                };
                if inside.targets.contains(&q) {
                    continue;
                }
                let mut path = vec![q];
                path.extend(&to_target);
                let mut reasons = Vec::new();
                if forecasts.get(&q).is_some_and(|f| f.late_by_days.is_some())
                    && !matches!(
                        project.status,
                        ProjectStatus::Done | ProjectStatus::Cancelled | ProjectStatus::Paused
                    )
                {
                    reasons.push(Reason::ProjectLate);
                }
                if w.red_projects.contains(&q) {
                    reasons.push(Reason::ProjectRed);
                }
                if !reasons.is_empty() {
                    let label_of = |id: &Uuid| {
                        project_by_id
                            .get(id)
                            .map_or_else(String::new, |p| p.title.clone())
                    };
                    items.push((
                        Some(q),
                        Approaching {
                            source: summary(NodeType::Project, q, &project.title),
                            target: summary(NodeType::Project, target, &target_project.title),
                            path: path
                                .iter()
                                .map(|id| summary(NodeType::Project, *id, &label_of(id)))
                                .collect(),
                            reasons,
                            overdue_days: None,
                            arrives_in: arrives,
                            severity,
                            target_slack: slack,
                        },
                    ));
                }
                queue.push_back((q, path));
            }
        }
    }

    // -------------------------------------------------------------- group and order
    let order = |a: &Approaching, b: &Approaching| {
        a.severity
            .cmp(&b.severity)
            .then(a.arrives_in.cmp(&b.arrives_in))
            .then_with(|| a.source.label.cmp(&b.source.label))
            .then_with(|| a.target.label.cmp(&b.target.label))
            .then_with(|| a.source.node.id.cmp(&b.source.node.id))
            .then_with(|| a.target.node.id.cmp(&b.target.node.id))
    };
    let mut by_project: BTreeMap<Option<Uuid>, Vec<Approaching>> = BTreeMap::new();
    for (project, item) in items {
        by_project.entry(project).or_default().push(item);
    }
    let mut groups: Vec<ApproachGroup> = by_project
        .into_iter()
        .map(|(project, mut items)| {
            items.sort_by(order);
            ApproachGroup {
                source_project: project
                    .and_then(|p| project_by_id.get(&p))
                    .map(|p| summary(NodeType::Project, p.id, &p.title)),
                items,
            }
        })
        .collect();
    let worst = |g: &ApproachGroup| g.items.first().map(|i| (i.severity, i.arrives_in));
    groups.sort_by(|a, b| {
        worst(a).cmp(&worst(b)).then_with(|| {
            let label = |g: &ApproachGroup| g.source_project.as_ref().map(|p| p.label.clone());
            label(a).cmp(&label(b))
        })
    });

    let loud: Vec<&Approaching> = groups
        .iter()
        .flat_map(|g| g.items.iter())
        .filter(|i| i.severity != Severity::Watch)
        .collect();
    let headline = (!loud.is_empty()).then(|| Headline {
        count: loud.len() as u32,
        first_in_days: loud.iter().map(|i| i.arrives_in).min().unwrap_or(0),
        worst: loud
            .iter()
            .map(|i| i.severity)
            .min()
            .unwrap_or(Severity::Watch),
    });
    ApproachReport { groups, headline }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::compute;
    use minimap_types::{ScheduleScope, WorkWeek};
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    // Monday 2027-03-01.
    const TODAY: Date = date!(2027 - 03 - 01);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, project: Option<u128>, days: f64) -> Task {
        Task {
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
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            confirmed_on: None,
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
            confirmed_on: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn edge(edge_type: EdgeType, from: (NodeType, u128), to: (NodeType, u128)) -> Edge {
        Edge {
            id: Uuid::from_u128(5_000 + from.1 * 100 + to.1),
            edge_type,
            from_type: from.0,
            from_id: id(from.1),
            to_type: to.0,
            to_id: id(to.1),
            attrs: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn blocks(from: u128, to: u128) -> Edge {
        edge(
            EdgeType::Blocks,
            (NodeType::Task, from),
            (NodeType::Task, to),
        )
    }

    fn depends(from: u128, to: u128) -> Edge {
        edge(
            EdgeType::DependsOn,
            (NodeType::Project, from),
            (NodeType::Project, to),
        )
    }

    struct World {
        tasks: Vec<Task>,
        edges: Vec<Edge>,
        projects: Vec<Project>,
        red: HashSet<Uuid>,
        me: Option<Uuid>,
    }

    impl World {
        fn new(tasks: Vec<Task>, edges: Vec<Edge>, projects: Vec<Project>) -> Self {
            Self {
                tasks,
                edges,
                projects,
                red: HashSet::new(),
                me: None,
            }
        }

        fn report(&self, scope: ApproachScope) -> ApproachReport {
            let schedule = compute(
                &self.tasks,
                &self.edges,
                &self.projects,
                TODAY,
                WorkWeek::MON_FRI,
                ScheduleScope::Portfolio,
            )
            .unwrap();
            approaching(
                &ApproachWorld {
                    tasks: &self.tasks,
                    edges: &self.edges,
                    projects: &self.projects,
                    schedule: &schedule,
                    red_projects: &self.red,
                    self_id: self.me,
                    today: TODAY,
                },
                scope,
            )
        }
    }

    fn items(r: &ApproachReport) -> Vec<&Approaching> {
        r.groups.iter().flat_map(|g| g.items.iter()).collect()
    }

    fn path_labels(a: &Approaching) -> Vec<&str> {
        a.path.iter().map(|n| n.label.as_str()).collect()
    }

    /// Project 10 ("Here") has task 2 blocked by task 1 of project 11 ("There").
    fn two_projects(there: Task, here_days: f64) -> World {
        World::new(
            vec![there, task(2, "Pen test", Some(10), here_days)],
            vec![blocks(1, 2)],
            vec![project(10, "Here", None), project(11, "There", None)],
        )
    }

    #[test]
    fn an_overdue_task_elsewhere_is_trouble_with_its_days() {
        let mut t = task(1, "Gateway auth", Some(11), 2.0);
        t.due_date = Some(date!(2027 - 02 - 26));
        let r = two_projects(t, 2.0).report(ApproachScope::Project(id(10)));
        let all = items(&r);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].reasons[0], Reason::Overdue);
        assert_eq!(all[0].overdue_days, Some(3));
        assert_eq!(path_labels(all[0]), ["Gateway auth", "Pen test"]);
        assert_eq!(all[0].target.label, "Pen test");
        // It comes from There, not Here.
        assert_eq!(r.groups[0].source_project.as_ref().unwrap().label, "There");
    }

    #[test]
    fn a_blocked_task_is_trouble_and_arrives_when_the_target_starts() {
        let mut t = task(1, "Gateway auth", Some(11), 2.0);
        t.status = TaskStatus::Blocked;
        let r = two_projects(t, 2.0).report(ApproachScope::Project(id(10)));
        let a = items(&r)[0].clone();
        assert_eq!(a.reasons, [Reason::Blocked]);
        // Gateway auth takes two days, so Pen test starts on the third working day: in 2 days.
        assert_eq!(a.arrives_in, 2);
    }

    #[test]
    fn a_task_scheduled_after_its_due_date_is_late_and_one_without_slack_has_no_slack() {
        // Four days of work due after two.
        let mut t = task(1, "Gateway auth", Some(11), 4.0);
        t.due_date = Some(date!(2027 - 03 - 02));
        let mut w = two_projects(t, 1.0);
        let r = w.report(ApproachScope::Project(id(10)));
        assert!(items(&r)[0].reasons.contains(&Reason::Late));
        // A target that cannot be met gives it negative slack.
        w.projects[1].target_date = Some(date!(2027 - 03 - 02));
        let r = w.report(ApproachScope::Project(id(10)));
        let reasons = &items(&r)[0].reasons;
        assert!(reasons.contains(&Reason::Late) && reasons.contains(&Reason::NoSlack));
        // The reasons come in the order of the enum.
        let mut sorted = reasons.clone();
        sorted.sort_by_key(|r| *r as u8);
        assert_eq!(*reasons, sorted);
    }

    #[test]
    fn healthy_work_elsewhere_is_not_trouble_and_nothing_shows() {
        let t = task(1, "Gateway auth", Some(11), 2.0);
        let r = two_projects(t, 2.0).report(ApproachScope::Project(id(10)));
        assert!(r.groups.is_empty() && r.headline.is_none());
    }

    #[test]
    fn finished_work_and_meetings_are_never_sources_and_break_chains() {
        let mut done = task(1, "Done", Some(11), 2.0);
        done.status = TaskStatus::Done;
        done.completed_at = Some(OffsetDateTime::UNIX_EPOCH);
        done.due_date = Some(date!(2020 - 01 - 01));
        let r = two_projects(done, 2.0).report(ApproachScope::Project(id(10)));
        assert!(r.groups.is_empty());
        // A chain through a finished task is cut: 1 (blocked) -> 3 (done) -> 2.
        let mut a = task(1, "A", Some(11), 1.0);
        a.status = TaskStatus::Blocked;
        let mut b = task(3, "B", Some(11), 1.0);
        b.status = TaskStatus::Done;
        b.completed_at = Some(OffsetDateTime::UNIX_EPOCH);
        let w = World::new(
            vec![a, b, task(2, "Pen test", Some(10), 1.0)],
            vec![blocks(1, 3), blocks(3, 2)],
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        assert!(w.report(ApproachScope::Project(id(10))).groups.is_empty());
        // A blocked meeting does not count.
        let mut m = task(1, "Sync", Some(11), 1.0);
        m.task_type = Some("meeting".into());
        m.start_minute = Some(600);
        m.due_date = Some(TODAY);
        m.status = TaskStatus::Blocked;
        let r = two_projects(m, 1.0).report(ApproachScope::Project(id(10)));
        assert!(r.groups.is_empty());
    }

    #[test]
    fn a_chain_that_comes_back_inside_stops_at_the_first_inside_task() {
        // 1 (There, blocked) -> 3 (Here) -> 4 (There) -> 2 (Here): from Here's point of view
        // the trouble reaches 3 directly, and 4 is only the way to 2.
        let mut a = task(1, "A", Some(11), 1.0);
        a.status = TaskStatus::Blocked;
        let w = World::new(
            vec![
                a,
                task(3, "Mid", Some(10), 1.0),
                task(4, "Out again", Some(11), 1.0),
                task(2, "End", Some(10), 1.0),
            ],
            vec![blocks(1, 3), blocks(3, 4), blocks(4, 2)],
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        let all = items(&r);
        assert_eq!(all.len(), 1);
        assert_eq!(path_labels(all[0]), ["A", "Mid"]);
    }

    #[test]
    fn the_shortest_chain_is_kept_and_the_walk_stops_at_the_hop_limit() {
        // Two ways from trouble T to target X: direct, and through M.
        let mut t = task(1, "T", Some(11), 1.0);
        t.status = TaskStatus::Blocked;
        let w = World::new(
            vec![
                t.clone(),
                task(3, "M", Some(11), 1.0),
                task(2, "X", Some(10), 1.0),
            ],
            vec![blocks(1, 2), blocks(1, 3), blocks(3, 2)],
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        assert_eq!(path_labels(items(&r)[0]), ["T", "X"]);

        // A line of MAX_HOPS + 1 outside tasks: only the ones within MAX_HOPS are reached.
        let mut tasks = vec![task(100, "Target", Some(10), 1.0)];
        let mut edges = Vec::new();
        for k in 1..=(MAX_HOPS as u128 + 1) {
            let mut x = task(100 + k, &format!("X{k}"), Some(11), 1.0);
            x.status = TaskStatus::Blocked;
            tasks.push(x);
            edges.push(blocks(100 + k, 100 + k - 1));
        }
        let w = World::new(
            tasks,
            edges,
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        let longest = items(&r).iter().map(|a| a.path.len()).max().unwrap();
        assert_eq!(longest, MAX_HOPS + 1);
        assert_eq!(items(&r).len(), MAX_HOPS);
    }

    #[test]
    fn only_the_driving_predecessor_hits_and_slack_decides_the_rest() {
        // Pen test needs A (1 day) and B (5 days): B drives it. Both are blocked, outside.
        let mut a = task(1, "A", Some(11), 1.0);
        a.status = TaskStatus::Blocked;
        let mut b = task(3, "B", Some(11), 5.0);
        b.status = TaskStatus::Blocked;
        let w = World::new(
            vec![a, b, task(2, "Pen test", Some(10), 1.0)],
            vec![blocks(1, 2), blocks(3, 2)],
            // A target that Pen test misses, so it is late.
            vec![
                project(10, "Here", Some(date!(2027 - 03 - 03))),
                project(11, "There", None),
            ],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        let severity = |name: &str| {
            items(&r)
                .into_iter()
                .find(|a| a.source.label == name)
                .unwrap()
                .severity
        };
        assert_eq!(severity("B"), Severity::Hits);
        // A does not hold it up, but it has no slack either: a threat.
        assert_eq!(severity("A"), Severity::Threatens);
        let h = r.headline.unwrap();
        assert_eq!((h.count, h.worst), (2, Severity::Hits));
    }

    #[test]
    fn plenty_of_slack_is_only_worth_watching_and_never_in_the_headline() {
        let mut t = task(1, "Gateway auth", Some(11), 1.0);
        t.status = TaskStatus::Blocked;
        let w = World::new(
            vec![t, task(2, "Pen test", Some(10), 1.0)],
            vec![blocks(1, 2)],
            vec![
                project(10, "Here", Some(date!(2027 - 04 - 30))),
                project(11, "There", None),
            ],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        let a = items(&r)[0].clone();
        assert_eq!(a.severity, Severity::Watch);
        assert!(a.target_slack.unwrap() > THREAT_SLACK);
        assert!(r.headline.is_none(), "{:?}", r.headline);
    }

    #[test]
    fn a_project_that_depends_on_a_late_project_hears_about_it() {
        // Here (target tight) depends on There, whose forecast is late.
        let w = World::new(
            vec![
                task(1, "Slow", Some(11), 6.0),
                task(2, "Work", Some(10), 1.0),
            ],
            vec![depends(10, 11)],
            vec![
                project(10, "Here", Some(date!(2027 - 03 - 03))),
                project(11, "There", Some(date!(2027 - 03 - 02))),
            ],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        let all = items(&r);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].reasons, [Reason::ProjectLate]);
        assert_eq!(all[0].source.node.node_type, NodeType::Project);
        assert_eq!(path_labels(all[0]), ["There", "Here"]);
        // Red health counts too, even when the forecast is fine.
        let mut calm = World::new(
            vec![task(2, "Work", Some(10), 1.0)],
            vec![depends(10, 11)],
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        calm.red.insert(id(11));
        let r = calm.report(ApproachScope::Project(id(10)));
        assert_eq!(items(&r)[0].reasons, [Reason::ProjectRed]);
        // And a chain of dependencies is followed, up to three links.
        let mut far = World::new(
            vec![task(2, "Work", Some(10), 1.0)],
            vec![depends(10, 11), depends(11, 12)],
            vec![
                project(10, "Here", None),
                project(11, "Middle", None),
                project(12, "Far", None),
            ],
        );
        far.red.insert(id(12));
        let r = far.report(ApproachScope::Project(id(10)));
        assert_eq!(path_labels(items(&r)[0]), ["Far", "Middle", "Here"]);
    }

    #[test]
    fn my_scope_is_the_tasks_assigned_to_me_and_says_nothing_without_a_me() {
        let me = id(900);
        let mut blocked = task(1, "Gateway auth", Some(11), 1.0);
        blocked.status = TaskStatus::Blocked;
        let assign = edge(
            EdgeType::AssignedTo,
            (NodeType::Task, 2),
            (NodeType::Person, 900),
        );
        let mut w = World::new(
            vec![blocked, task(2, "Mine", Some(10), 1.0)],
            vec![blocks(1, 2), assign],
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        // No "me": nothing.
        assert!(w.report(ApproachScope::Me).groups.is_empty());
        w.me = Some(me);
        let r = w.report(ApproachScope::Me);
        assert_eq!(items(&r).len(), 1);
        assert_eq!(items(&r)[0].target.label, "Mine");
        // Someone else's task is not mine.
        w.me = Some(id(901));
        assert!(w.report(ApproachScope::Me).groups.is_empty());
    }

    #[test]
    fn an_objectives_scope_is_the_projects_that_feed_it() {
        let mut blocked = task(1, "Gateway auth", Some(11), 1.0);
        blocked.status = TaskStatus::Blocked;
        let feeds = edge(
            EdgeType::ContributesTo,
            (NodeType::Project, 10),
            (NodeType::Objective, 50),
        );
        let w = World::new(
            vec![blocked, task(2, "Pen test", Some(10), 1.0)],
            vec![blocks(1, 2), feeds],
            vec![project(10, "Here", None), project(11, "There", None)],
        );
        let r = w.report(ApproachScope::Objective(id(50)));
        assert_eq!(items(&r).len(), 1);
        // An objective nothing feeds hears nothing.
        assert!(w.report(ApproachScope::Objective(id(51))).groups.is_empty());
    }

    #[test]
    fn groups_and_items_are_ordered_worst_and_soonest_first() {
        let mut a = task(1, "A", Some(11), 1.0);
        a.status = TaskStatus::Blocked;
        let mut b = task(3, "B", Some(12), 1.0);
        b.status = TaskStatus::Blocked;
        let w = World::new(
            vec![
                a,
                b,
                task(2, "X", Some(10), 1.0),
                task(4, "Y", Some(10), 1.0),
            ],
            // A reaches X directly; B reaches Y through a second task of the same project.
            vec![blocks(1, 2), blocks(3, 4)],
            vec![
                project(10, "Here", Some(date!(2027 - 03 - 02))),
                project(11, "There", None),
                project(12, "Elsewhere", None),
            ],
        );
        let r = w.report(ApproachScope::Project(id(10)));
        assert_eq!(r.groups.len(), 2);
        // Ties on severity and time fall back to the project name.
        let names: Vec<&str> = r
            .groups
            .iter()
            .map(|g| g.source_project.as_ref().unwrap().label.as_str())
            .collect();
        assert_eq!(names, ["Elsewhere", "There"]);
        // The same input gives the same report.
        assert_eq!(r, w.report(ApproachScope::Project(id(10))));
    }

    proptest! {
        // Every reported chain is made of real `blocks` links, starts outside and ends inside,
        // is at most MAX_HOPS + 1 long, and the same input always gives the same report.
        #[test]
        fn chains_are_real_short_and_stable(
            n in 3usize..9,
            links in proptest::collection::vec((0usize..8, 0usize..8), 0..14),
            blocked in proptest::collection::vec(any::<bool>(), 9),
            inside in proptest::collection::vec(any::<bool>(), 9),
        ) {
            let tasks: Vec<Task> = (0..n)
                .map(|i| {
                    let mut t = task(
                        i as u128 + 1,
                        &format!("t{i}"),
                        Some(if inside[i] { 10 } else { 11 }),
                        1.0,
                    );
                    if blocked[i] {
                        t.status = TaskStatus::Blocked;
                    }
                    t
                })
                .collect();
            // Only forward links (lower to higher): never a loop.
            let mut seen = HashSet::new();
            let edges: Vec<Edge> = links
                .iter()
                .filter(|(a, b)| a < b && *b < n && seen.insert((*a, *b)))
                .map(|(a, b)| blocks(*a as u128 + 1, *b as u128 + 1))
                .collect();
            let w = World::new(
                tasks,
                edges.clone(),
                vec![project(10, "Here", None), project(11, "There", None)],
            );
            let first = w.report(ApproachScope::Project(id(10)));
            prop_assert_eq!(&first, &w.report(ApproachScope::Project(id(10))));
            let real: HashSet<(Uuid, Uuid)> = edges.iter().map(|e| (e.from_id, e.to_id)).collect();
            let project_of = |node: &NodeSummary| {
                w.tasks.iter().find(|t| t.id == node.node.id).and_then(|t| t.project_id)
            };
            for a in items(&first) {
                prop_assert!(a.path.len() <= MAX_HOPS + 1 && a.path.len() >= 2);
                for pair in a.path.windows(2) {
                    prop_assert!(real.contains(&(pair[0].node.id, pair[1].node.id)));
                }
                prop_assert_eq!(project_of(a.path.first().unwrap()), Some(id(11)));
                prop_assert_eq!(project_of(a.path.last().unwrap()), Some(id(10)));
                // Nothing in the middle is inside (the walk stops there).
                for mid in &a.path[..a.path.len() - 1] {
                    prop_assert_eq!(project_of(mid), Some(id(11)));
                }
            }
        }
    }
}
