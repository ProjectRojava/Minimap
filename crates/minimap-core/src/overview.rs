//! The portfolio overview (spec 15): health of every project and objective, the top risks, and
//! who is overloaded (from `capacity`). Pure; the stale waiting-ons are added by the command (they come from the
//! waiting-on rules).

use std::collections::{BTreeMap, HashMap};

use minimap_types::{
    Date, Edge, EdgeType, Health, HealthLevel, HealthThresholds, NodeRef, NodeSummary, NodeType,
    Objective, ObjectiveHealthRow, ObjectiveStatus, OverloadedPerson, OverviewCounts, Person,
    PortfolioOverview, Project, ProjectHealthRow, ProjectStatus, RiskItem, RiskKind, Schedule,
    ScheduleScope, Task, TaskStatus, Uuid,
};

use crate::{
    capacity,
    health::{
        objective_health, project_health, risk_score, task_health, Contribution, ProjectFacts,
        TargetFacts,
    },
    impact::late_working_days,
    schedule::compute,
};

const EPS: f64 = 1e-9;
/// Risks listed on the overview.
pub const TOP_RISKS: usize = 5;
/// People listed as overloaded.
const MAX_OVERLOADED: usize = 8;

pub struct OverviewWorld<'a> {
    pub tasks: &'a [Task],
    pub edges: &'a [Edge],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    pub people: &'a [Person],
    pub today: Date,
    pub hours_per_day: f64,
    pub thresholds: HealthThresholds,
    /// More open tasks than this flags a person as overloaded.
    pub task_limit: u32,
}

fn is_open(t: &Task) -> bool {
    t.archived_at.is_none() && !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
}

fn weight_of(e: &Edge) -> f64 {
    e.attrs
        .get("weight")
        .and_then(|w| w.as_f64())
        .unwrap_or(1.0)
}

fn summary(node_type: NodeType, id: Uuid, label: &str) -> NodeSummary {
    NodeSummary {
        node: NodeRef::new(node_type, id),
        label: label.to_owned(),
        archived: false,
    }
}

fn level_rank(l: HealthLevel) -> u8 {
    match l {
        HealthLevel::Red => 0,
        HealthLevel::Amber => 1,
        HealthLevel::Green => 2,
        HealthLevel::Idle => 3,
    }
}

pub fn build(w: &OverviewWorld) -> PortfolioOverview {
    let mut warnings = Vec::new();
    let schedule: Option<Schedule> = match compute(
        w.tasks,
        w.edges,
        w.projects,
        w.today,
        ScheduleScope::Portfolio,
    ) {
        Ok(s) => Some(s),
        Err(e) => {
            warnings.push(format!(
                "{e}. Lateness and workload can't be judged until the loop is removed."
            ));
            None
        }
    };
    let forecast: HashMap<Uuid, &minimap_types::ProjectForecast> = schedule
        .iter()
        .flat_map(|s| s.projects.iter())
        .map(|f| (f.project_id, f))
        .collect();
    let scheduled: HashMap<Uuid, &minimap_types::ScheduledTask> = schedule
        .iter()
        .flat_map(|s| s.tasks.iter())
        .map(|t| (t.id, t))
        .collect();

    // ------------------------------------------------------------- projects
    let mut tasks_of: HashMap<Uuid, Vec<&Task>> = HashMap::new();
    for t in w.tasks.iter().filter(|t| is_open(t)) {
        if let Some(p) = t.project_id {
            tasks_of.entry(p).or_default().push(t);
        }
    }
    let overdue = |t: &Task| t.due_date.is_some_and(|d| d < w.today);
    let blocked = |t: &Task| t.status == TaskStatus::Blocked;

    let mut rows: BTreeMap<Uuid, ProjectHealthRow> = BTreeMap::new();
    for p in w.projects.iter().filter(|p| p.archived_at.is_none()) {
        let open: &[&Task] = tasks_of.get(&p.id).map_or(&[], Vec::as_slice);
        let f = forecast.get(&p.id);
        let n_overdue = open.iter().filter(|t| overdue(t)).count() as u32;
        let n_blocked = open.iter().filter(|t| blocked(t)).count() as u32;
        let risky = open.iter().filter(|t| overdue(t) || blocked(t)).count() as u32;
        let unestimated = open.iter().filter(|t| t.estimate_days.is_none()).count() as u32;
        let late_days = f.and_then(|f| f.late_by_days);
        let spare_days = match (
            f.and_then(|f| f.target_offset),
            f.and_then(|f| f.finish_offset),
        ) {
            (Some(t), Some(fin)) if late_days.is_none() => {
                Some((t - fin + EPS).floor().max(0.0) as u32)
            }
            _ => None,
        };
        let facts = ProjectFacts {
            status: p.status,
            open_tasks: open.len() as u32,
            overdue: n_overdue,
            blocked: n_blocked,
            risky,
            unestimated,
            target: p.target_date,
            projected_finish: f.and_then(|f| f.projected_finish),
            late_days,
            spare_days,
        };
        rows.insert(
            p.id,
            ProjectHealthRow {
                project: summary(NodeType::Project, p.id, &p.title),
                status: p.status,
                priority: p.priority,
                health: project_health(&facts, &w.thresholds),
                projected_finish: facts.projected_finish,
                target_date: p.target_date,
                open_tasks: facts.open_tasks,
                overdue_tasks: n_overdue,
                blocked_tasks: n_blocked,
                unestimated_tasks: unestimated,
                weight: None,
            },
        );
    }

    // ----------------------------------------------------------- objectives
    struct Feed<'a> {
        edge: &'a Edge,
    }
    let mut feeds: BTreeMap<Uuid, Vec<Feed>> = BTreeMap::new();
    let mut feeds_objective: HashMap<Uuid, Vec<Uuid>> = HashMap::new(); // project -> objectives
    for e in w.edges.iter().filter(|e| {
        e.edge_type == EdgeType::ContributesTo
            && e.archived_at.is_none()
            && e.to_type == NodeType::Objective
    }) {
        feeds.entry(e.to_id).or_default().push(Feed { edge: e });
        if e.from_type == NodeType::Project {
            feeds_objective.entry(e.from_id).or_default().push(e.to_id);
        }
    }
    let task_by_id: HashMap<Uuid, &Task> = w.tasks.iter().map(|t| (t.id, t)).collect();

    let mut objectives: Vec<ObjectiveHealthRow> = Vec::new();
    for o in w.objectives.iter().filter(|o| o.archived_at.is_none()) {
        let mut contributions: Vec<Contribution> = Vec::new();
        let mut nested: Vec<ProjectHealthRow> = Vec::new();
        // The latest finish among what feeds the objective (offset, date).
        let mut latest: Option<(f64, Date)> = None;
        for feed in feeds.get(&o.id).into_iter().flatten() {
            let weight = weight_of(feed.edge);
            match feed.edge.from_type {
                NodeType::Project => {
                    let Some(row) = rows.get(&feed.edge.from_id) else {
                        continue;
                    };
                    let mut shown = row.clone();
                    shown.weight = Some(weight);
                    nested.push(shown);
                    let score = match (row.status, row.health.level) {
                        (ProjectStatus::Done, _) => Some(100),
                        (ProjectStatus::Cancelled | ProjectStatus::Paused, _) => None,
                        (_, HealthLevel::Idle) => None,
                        _ => Some(row.health.score),
                    };
                    if let Some(score) = score.filter(|_| weight > 0.0) {
                        contributions.push(Contribution {
                            name: row.project.label.clone(),
                            weight,
                            score,
                            lead_reason: row
                                .health
                                .reasons
                                .first()
                                .filter(|r| r.level != HealthLevel::Green)
                                .map(|r| r.text.clone()),
                        });
                    }
                    if let Some(f) = forecast.get(&feed.edge.from_id) {
                        if let (Some(off), Some(date), true) =
                            (f.finish_offset, f.projected_finish, f.open_tasks > 0)
                        {
                            if latest.is_none_or(|(l, _)| off > l) {
                                latest = Some((off, date));
                            }
                        }
                    }
                }
                NodeType::Task => {
                    let Some(t) = task_by_id.get(&feed.edge.from_id) else {
                        continue;
                    };
                    if t.archived_at.is_some() || t.status == TaskStatus::Cancelled || weight <= 0.0
                    {
                        continue;
                    }
                    if t.status == TaskStatus::Done {
                        contributions.push(Contribution {
                            name: t.title.clone(),
                            weight,
                            score: 100,
                            lead_reason: None,
                        });
                        continue;
                    }
                    let h = task_health(t.due_date, overdue(t), blocked(t));
                    contributions.push(Contribution {
                        name: t.title.clone(),
                        weight,
                        score: h.score,
                        lead_reason: h
                            .reasons
                            .first()
                            .filter(|r| r.level != HealthLevel::Green)
                            .map(|r| r.text.clone()),
                    });
                    if let Some(s) = scheduled.get(&t.id) {
                        if latest.is_none_or(|(l, _)| s.ef > l) {
                            latest = Some((s.ef, s.finish));
                        }
                    }
                }
                _ => {}
            }
        }
        nested.sort_by(|a, b| {
            level_rank(a.health.level)
                .cmp(&level_rank(b.health.level))
                .then(a.health.score.cmp(&b.health.score))
                .then(a.priority.cmp(&b.priority))
                .then_with(|| a.project.label.cmp(&b.project.label))
        });
        let target = match (o.target_date, latest) {
            (Some(target), Some((_, finish))) => Some(TargetFacts {
                target,
                finish,
                late_days: late_working_days(finish, target),
            }),
            _ => None,
        };
        objectives.push(ObjectiveHealthRow {
            objective: summary(NodeType::Objective, o.id, &o.title),
            status: o.status,
            priority: o.priority,
            target_date: o.target_date,
            health: objective_health(
                o.status == ObjectiveStatus::Done,
                &contributions,
                target.as_ref(),
                &w.thresholds,
            ),
            projects: nested,
        });
    }
    objectives.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| match (a.target_date, b.target_date) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.objective.label.cmp(&b.objective.label))
    });

    // Active projects that feed no objective.
    let mut unlinked: Vec<ProjectHealthRow> = rows
        .values()
        .filter(|r| !feeds_objective.contains_key(&r.project.node.id))
        .filter(|r| !matches!(r.status, ProjectStatus::Done | ProjectStatus::Cancelled))
        .cloned()
        .collect();
    unlinked.sort_by(|a, b| {
        level_rank(a.health.level)
            .cmp(&level_rank(b.health.level))
            .then(a.health.score.cmp(&b.health.score))
            .then(a.priority.cmp(&b.priority))
            .then_with(|| a.project.label.cmp(&b.project.label))
    });

    // --------------------------------------------------------------- risks
    let objective_by_id: HashMap<Uuid, &Objective> =
        w.objectives.iter().map(|o| (o.id, o)).collect();
    let mut risks: Vec<RiskItem> = Vec::new();
    for r in rows.values() {
        if !matches!(r.health.level, HealthLevel::Amber | HealthLevel::Red) {
            continue;
        }
        // A project counts at the priority of the most important live objective it feeds.
        let mut priority = r.priority;
        let mut via = None;
        for oid in feeds_objective
            .get(&r.project.node.id)
            .into_iter()
            .flatten()
        {
            if let Some(o) = objective_by_id.get(oid) {
                if o.archived_at.is_none()
                    && o.status != ObjectiveStatus::Done
                    && o.priority < priority
                {
                    priority = o.priority;
                    via = Some(o.title.clone());
                }
            }
        }
        risks.push(RiskItem {
            kind: RiskKind::Project,
            node: r.project.clone(),
            health: r.health.clone(),
            risk_score: risk_score(r.health.score, priority, RiskKind::Project),
            priority,
            via_objective: via,
        });
    }
    for t in w.tasks.iter().filter(|t| is_open(t) && t.priority <= 2) {
        let (is_overdue, is_blocked) = (overdue(t), blocked(t));
        if !is_overdue && !is_blocked {
            continue;
        }
        let h: Health = task_health(t.due_date, is_overdue, is_blocked);
        risks.push(RiskItem {
            kind: RiskKind::Task,
            node: summary(NodeType::Task, t.id, &t.title),
            risk_score: risk_score(h.score, t.priority, RiskKind::Task),
            health: h,
            priority: t.priority,
            via_objective: None,
        });
    }
    risks.sort_by(|a, b| {
        b.risk_score
            .total_cmp(&a.risk_score)
            .then(a.health.score.cmp(&b.health.score))
            .then_with(|| a.node.label.cmp(&b.node.label))
            .then_with(|| a.node.node.id.cmp(&b.node.node.id))
    });
    let more_risks = risks.len().saturating_sub(TOP_RISKS) as u32;
    risks.truncate(TOP_RISKS);

    // ---------------------------------------------------------- overloaded
    // Over capacity this week or next (the same figures as the Capacity screen), or too many
    // open tasks.
    let capacity = capacity::compute(&capacity::CapacityInput {
        tasks: w.tasks,
        edges: w.edges,
        projects: w.projects,
        people: w.people,
        today: w.today,
        hours_per_day: w.hours_per_day,
        from: None,
        to: None,
        weeks: Some(2),
        task_limit: w.task_limit,
    });
    let mut overloaded: Vec<OverloadedPerson> = capacity
        .people
        .iter()
        .filter(|p| p.peak_pct > 100.0 + EPS || p.over_task_limit)
        .map(|p| OverloadedPerson {
            person: p.person.clone(),
            load_pct: p.peak_pct,
            peak_week: p.peak_week,
            open_tasks: p.active_tasks,
            over_task_limit: p.over_task_limit,
        })
        .collect();
    overloaded.truncate(MAX_OVERLOADED);

    let count = |l: HealthLevel| rows.values().filter(|r| r.health.level == l).count() as u32;
    PortfolioOverview {
        today: w.today,
        counts: OverviewCounts {
            red: count(HealthLevel::Red),
            amber: count(HealthLevel::Amber),
            green: count(HealthLevel::Green),
            idle: count(HealthLevel::Idle),
        },
        objectives,
        unlinked_projects: unlinked,
        risks,
        more_risks,
        overloaded,
        stale_waiting: Vec::new(),
        warnings,
        thresholds: w.thresholds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{level_of_score, signal_score};
    use minimap_types::ProjectStatus;
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    // 2027-03-01 is a Monday.
    const MON: Date = date!(2027 - 03 - 01);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, days: Option<f64>, project: Option<u128>) -> Task {
        Task {
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: project.map(id),
            status: TaskStatus::Todo,
            estimate_days: days,
            start_date: None,
            due_date: None,
            completed_at: None,
            priority: 3,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn project(n: u128, title: &str, target: Option<Date>, priority: u8) -> Project {
        Project {
            id: id(n),
            title: title.into(),
            slug: title.to_lowercase(),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: target,
            status: ProjectStatus::Active,
            priority,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn objective(n: u128, title: &str, target: Option<Date>, priority: u8) -> Objective {
        Objective {
            id: id(n),
            title: title.into(),
            description: String::new(),
            target_date: target,
            status: ObjectiveStatus::OnTrack,
            priority,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn person(n: u128, name: &str, hours: f64) -> Person {
        Person {
            id: id(n),
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: hours,
            is_self: false,
            notes: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn edge(
        kind: EdgeType,
        ft: NodeType,
        from: u128,
        tt: NodeType,
        to: u128,
        attrs: serde_json::Value,
    ) -> Edge {
        Edge {
            id: Uuid::from_u128(50_000 + from * 1000 + to + u128::from(kind as u8)),
            edge_type: kind,
            from_type: ft,
            from_id: id(from),
            to_type: tt,
            to_id: id(to),
            attrs,
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn blocks(a: u128, b: u128) -> Edge {
        edge(
            EdgeType::Blocks,
            NodeType::Task,
            a,
            NodeType::Task,
            b,
            serde_json::json!({}),
        )
    }

    fn feeds(project: u128, objective: u128, weight: Option<f64>) -> Edge {
        edge(
            EdgeType::ContributesTo,
            NodeType::Project,
            project,
            NodeType::Objective,
            objective,
            weight.map_or(
                serde_json::json!({}),
                |w| serde_json::json!({ "weight": w }),
            ),
        )
    }

    fn assigned(task: u128, person: u128, pct: Option<u64>) -> Edge {
        edge(
            EdgeType::AssignedTo,
            NodeType::Task,
            task,
            NodeType::Person,
            person,
            pct.map_or(
                serde_json::json!({}),
                |p| serde_json::json!({ "allocation_pct": p }),
            ),
        )
    }

    #[derive(Default)]
    struct World_ {
        tasks: Vec<Task>,
        edges: Vec<Edge>,
        projects: Vec<Project>,
        objectives: Vec<Objective>,
        people: Vec<Person>,
    }

    impl World_ {
        fn overview(&self) -> PortfolioOverview {
            self.overview_with(HealthThresholds::default())
        }

        fn overview_with(&self, thresholds: HealthThresholds) -> PortfolioOverview {
            build(&OverviewWorld {
                tasks: &self.tasks,
                edges: &self.edges,
                projects: &self.projects,
                objectives: &self.objectives,
                people: &self.people,
                today: MON,
                hours_per_day: 8.0,
                thresholds,
                task_limit: 10,
            })
        }
    }

    fn project_row<'a>(o: &'a PortfolioOverview, title: &str) -> &'a ProjectHealthRow {
        o.objectives
            .iter()
            .flat_map(|x| x.projects.iter())
            .chain(o.unlinked_projects.iter())
            .find(|p| p.project.label == title)
            .unwrap_or_else(|| panic!("no {title}"))
    }

    /// The seeded "at risk" project: 8 days of chained work against a 5-day target, one task
    /// overdue and one without an estimate.
    fn at_risk() -> World_ {
        let mut overdue = task(1, "Design", Some(4.0), Some(10));
        overdue.due_date = Some(date!(2027 - 02 - 26));
        World_ {
            tasks: vec![
                overdue,
                task(2, "Build", Some(4.0), Some(10)),
                task(3, "Docs", None, Some(10)),
                task(4, "Spare", Some(1.0), Some(10)),
            ],
            edges: vec![blocks(1, 2)],
            projects: vec![project(10, "Launch", Some(date!(2027 - 03 - 05)), 3)],
            objectives: vec![],
            people: vec![],
        }
    }

    #[test]
    fn an_at_risk_project_is_amber_or_red_with_the_right_reasons() {
        let o = at_risk().overview();
        let p = project_row(&o, "Launch");
        // 8 working days of chained work ends Wed 2027-03-10: 3 working days past Friday 03-05.
        assert_eq!(p.projected_finish, Some(date!(2027 - 03 - 10)));
        assert_eq!(p.health.level, HealthLevel::Amber);
        let texts: Vec<&str> = p.health.reasons.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                // Late is the worst signal; 25% overdue is amber, 25% unestimated is fine but listed.
                "projected 3 working days late (target 2027-03-05)",
                "1 task overdue (1 of 4 open)",
                "1 of 4 open tasks have no estimate",
            ]
        );
        assert_eq!(
            (
                p.open_tasks,
                p.overdue_tasks,
                p.blocked_tasks,
                p.unestimated_tasks
            ),
            (4, 1, 0, 1)
        );
        // Stricter thresholds make the same facts red.
        let strict = HealthThresholds {
            late_red_days: 3,
            ..HealthThresholds::default()
        };
        let o = at_risk().overview_with(strict);
        assert_eq!(project_row(&o, "Launch").health.level, HealthLevel::Red);
        assert_eq!(o.thresholds, strict);
        assert_eq!((o.counts.red, o.counts.amber), (1, 0));
    }

    #[test]
    fn an_on_track_project_is_green_and_idle_ones_are_counted_apart() {
        let mut w = World_ {
            tasks: vec![
                task(1, "A", Some(2.0), Some(10)),
                task(2, "B", Some(1.0), Some(11)),
            ],
            projects: vec![
                project(10, "Fine", Some(date!(2027 - 03 - 12)), 3),
                project(11, "Shipped", None, 3),
            ],
            ..Default::default()
        };
        w.projects[1].status = ProjectStatus::Done;
        let o = w.overview();
        let fine = project_row(&o, "Fine");
        assert_eq!(fine.health.level, HealthLevel::Green);
        assert!(fine.health.reasons[0]
            .text
            .starts_with("on track: projected 2027-03-02"));
        assert!(fine.health.reasons[0]
            .text
            .contains("8 working days to spare"));
        assert_eq!(
            (o.counts.green, o.counts.idle, o.counts.red, o.counts.amber),
            (1, 1, 0, 0)
        );
        // Done projects aren't listed as unlinked work.
        assert!(o
            .unlinked_projects
            .iter()
            .all(|p| p.project.label != "Shipped"));
    }

    // ------------------------------------------------------------ objectives

    #[test]
    fn objectives_nest_their_projects_worst_first_with_weights() {
        let mut w = at_risk();
        w.projects
            .push(project(11, "Calm", Some(date!(2027 - 03 - 31)), 3));
        w.tasks.push(task(5, "Easy", Some(1.0), Some(11)));
        w.objectives = vec![objective(20, "Launch EU", Some(date!(2027 - 03 - 31)), 1)];
        w.edges.push(feeds(10, 20, Some(0.7)));
        w.edges.push(feeds(11, 20, Some(0.3)));
        let o = w.overview();
        assert_eq!(o.objectives.len(), 1);
        let obj = &o.objectives[0];
        let names: Vec<&str> = obj
            .projects
            .iter()
            .map(|p| p.project.label.as_str())
            .collect();
        assert_eq!(names, ["Launch", "Calm"], "worst first");
        assert_eq!(obj.projects[0].weight, Some(0.7));
        // 0.7 x amber score + 0.3 x 100.
        assert!(
            obj.health.score < 100 && obj.health.score > 25,
            "{}",
            obj.health.score
        );
        assert!(o.unlinked_projects.is_empty());
        assert_eq!(obj.status, ObjectiveStatus::OnTrack);
    }

    #[test]
    fn a_red_project_keeps_its_objective_from_looking_green() {
        let mut w = at_risk();
        w.projects.push(project(11, "Calm", None, 3));
        w.projects.push(project(12, "Calm too", None, 3));
        w.tasks.push(task(5, "x", Some(1.0), Some(11)));
        w.tasks.push(task(6, "y", Some(1.0), Some(12)));
        w.objectives = vec![objective(20, "Goal", None, 2)];
        for p in [10, 11, 12] {
            w.edges.push(feeds(p, 20, None));
        }
        let strict = HealthThresholds {
            late_red_days: 3,
            ..HealthThresholds::default()
        };
        let o = w.overview_with(strict);
        let obj = &o.objectives[0];
        // Average of (red, 100, 100) would be green; the red project caps it at amber.
        assert_eq!(obj.health.level, HealthLevel::Amber);
        assert!(obj.health.reasons[0]
            .text
            .starts_with("Launch is red: projected 3 working days late"));
    }

    #[test]
    fn an_objectives_own_target_is_judged_against_what_feeds_it() {
        let mut w = at_risk();
        // The project finishes Wed 2027-03-10; the objective wants it by Fri 03-05.
        w.objectives = vec![objective(20, "Goal", Some(date!(2027 - 03 - 05)), 3)];
        w.edges.push(feeds(10, 20, None));
        let o = w.overview();
        let h = &o.objectives[0].health;
        assert!(
            h.reasons.iter().any(|r| r
                .text
                .contains("3 working days after its target 2027-03-05")),
            "{:?}",
            h.reasons
        );
        // Without a target on the objective there's no such reason.
        w.objectives[0].target_date = None;
        assert!(!w.overview().objectives[0]
            .health
            .reasons
            .iter()
            .any(|r| r.text.contains("after its target")));
    }

    #[test]
    fn objectives_with_nothing_active_or_marked_done_are_idle_and_ordering_is_by_priority() {
        let mut w = World_ {
            objectives: vec![
                objective(20, "Later", Some(date!(2027 - 09 - 01)), 2),
                objective(21, "Sooner", Some(date!(2027 - 06 - 01)), 2),
                objective(22, "Top", None, 1),
                objective(23, "Finished", None, 3),
            ],
            ..Default::default()
        };
        w.objectives[3].status = ObjectiveStatus::Done;
        let o = w.overview();
        let order: Vec<&str> = o
            .objectives
            .iter()
            .map(|x| x.objective.label.as_str())
            .collect();
        assert_eq!(order, ["Top", "Sooner", "Later", "Finished"]);
        assert!(o
            .objectives
            .iter()
            .all(|x| x.health.level == HealthLevel::Idle));
        assert_eq!(o.objectives[3].health.reasons[0].text, "Marked done");
    }

    #[test]
    fn task_contributions_count_too() {
        let mut overdue = task(1, "Sign contract", Some(1.0), None);
        overdue.due_date = Some(date!(2027 - 02 - 25));
        let w = World_ {
            tasks: vec![overdue],
            objectives: vec![objective(20, "Goal", None, 3)],
            edges: vec![edge(
                EdgeType::ContributesTo,
                NodeType::Task,
                1,
                NodeType::Objective,
                20,
                serde_json::json!({}),
            )],
            ..Default::default()
        };
        let h = &w.overview().objectives[0].health;
        assert_eq!(
            h.level,
            HealthLevel::Red,
            "its only contributor is an overdue (red) task"
        );
        assert!(h.reasons[0]
            .text
            .starts_with("Sign contract is red: overdue"));
    }

    // ----------------------------------------------------------------- risks

    #[test]
    fn risks_rank_severity_times_importance_and_the_objective_lends_its_priority() {
        // Two equally late projects; one feeds a top-priority objective, so it ranks first.
        let late = |n: u128, title: &str, priority: u8| {
            (
                project(n, title, Some(date!(2027 - 03 - 01)), priority),
                task(n * 10, &format!("{title} work"), Some(3.0), Some(n)),
            )
        };
        let (pa, ta) = late(10, "Plain", 3);
        let (pb, tb) = late(11, "Strategic", 3);
        let w = World_ {
            tasks: vec![ta, tb],
            projects: vec![pa, pb],
            objectives: vec![objective(20, "North star", None, 1)],
            edges: vec![feeds(11, 20, None)],
            ..Default::default()
        };
        let o = w.overview();
        let names: Vec<&str> = o.risks.iter().map(|r| r.node.label.as_str()).collect();
        assert_eq!(names, ["Strategic", "Plain"]);
        assert_eq!(o.risks[0].priority, 1);
        assert_eq!(o.risks[0].via_objective.as_deref(), Some("North star"));
        assert_eq!(o.risks[1].via_objective, None);
        assert!(o.risks[0].risk_score > o.risks[1].risk_score);
    }

    #[test]
    fn only_trouble_is_a_risk_and_the_list_is_capped_at_five() {
        let mut w = World_::default();
        for n in 0..8u128 {
            w.projects.push(project(
                10 + n,
                &format!("P{n}"),
                Some(date!(2027 - 03 - 01)),
                3,
            ));
            w.tasks.push(task(
                100 + n,
                &format!("t{n}"),
                Some(2.0 + n as f64),
                Some(10 + n),
            ));
        }
        w.projects
            .push(project(50, "Healthy", Some(date!(2027 - 12 - 01)), 1));
        w.tasks.push(task(500, "fine", Some(1.0), Some(50)));
        let o = w.overview();
        assert_eq!(o.risks.len(), TOP_RISKS);
        assert_eq!(o.more_risks, 3);
        assert!(o.risks.iter().all(|r| r.node.label != "Healthy"));
        assert!(o
            .risks
            .windows(2)
            .all(|p| p[0].risk_score >= p[1].risk_score));
        // The latest-running project (most days late) is the worst.
        assert_eq!(o.risks[0].node.label, "P7");
    }

    #[test]
    fn a_top_priority_task_that_is_overdue_or_blocked_is_a_risk_but_ranks_below_a_red_project() {
        let mut hot = task(91, "Renew certificate", Some(1.0), None);
        hot.priority = 1;
        hot.due_date = Some(date!(2027 - 02 - 20));
        let mut low = task(92, "Tidy wiki", Some(1.0), None);
        low.priority = 4;
        low.due_date = Some(date!(2027 - 02 - 20));
        let mut blocked = task(93, "Waiting on legal", Some(1.0), None);
        blocked.priority = 2;
        blocked.status = TaskStatus::Blocked;
        let mut w = at_risk();
        w.tasks.extend([hot, low, blocked]);
        let strict = HealthThresholds {
            late_red_days: 3,
            ..HealthThresholds::default()
        };
        let o = w.overview_with(strict);
        let order: Vec<(&str, RiskKind)> = o
            .risks
            .iter()
            .map(|r| (r.node.label.as_str(), r.kind))
            .collect();
        assert_eq!(order[0], ("Launch", RiskKind::Project), "{order:?}");
        assert!(order.contains(&("Renew certificate", RiskKind::Task)));
        assert!(order.contains(&("Waiting on legal", RiskKind::Task)));
        assert!(
            !order.iter().any(|(n, _)| *n == "Tidy wiki"),
            "low-priority tasks aren't risks"
        );
        let hot_pos = order
            .iter()
            .position(|(n, _)| *n == "Renew certificate")
            .unwrap();
        let blocked_pos = order
            .iter()
            .position(|(n, _)| *n == "Waiting on legal")
            .unwrap();
        assert!(hot_pos < blocked_pos, "overdue P1 outranks blocked P2");
    }

    // ------------------------------------------------------------ overloaded

    #[test]
    fn people_over_capacity_in_the_next_week_are_flagged() {
        // Priya works 40h (5 days a week at 8h). Two parallel tasks of 3 and 4 days both start
        // today (Monday): this week holds 3 + 4 = 7 days of 5 = 140%.
        let w = World_ {
            tasks: vec![
                task(1, "A", Some(3.0), None),
                task(2, "B", Some(4.0), None),
                task(3, "C", Some(1.0), None),
            ],
            people: vec![
                person(30, "Priya", 40.0),
                person(31, "Raj", 40.0),
                person(32, "Part-timer", 20.0),
            ],
            edges: vec![
                assigned(1, 30, None),
                assigned(2, 30, None),
                assigned(3, 31, None),
                // Half-time on a 4-day task: 2 days of work for the 20h person (capacity 2.5).
                assigned(2, 32, Some(50)),
            ],
            ..Default::default()
        };
        let o = w.overview();
        assert_eq!(o.overloaded.len(), 1);
        let p = &o.overloaded[0];
        assert_eq!(p.person.label, "Priya");
        assert_eq!((p.load_pct, p.open_tasks), (140.0, 2));
        assert_eq!(p.peak_week, Some(date!(2027 - 03 - 01)));
        assert!(!p.over_task_limit);
    }

    #[test]
    fn work_beyond_the_next_week_and_part_time_capacity_are_handled() {
        let mut far = task(1, "Far off", Some(3.0), None);
        far.start_date = Some(date!(2027 - 03 - 15)); // two weeks out
        let w = World_ {
            tasks: vec![far, task(2, "Big", Some(10.0), None)],
            people: vec![person(30, "Priya", 40.0), person(31, "Half", 20.0)],
            edges: vec![assigned(1, 30, None), assigned(2, 31, None)],
            ..Default::default()
        };
        let o = w.overview();
        // Priya's only task starts in two weeks: outside this week and next. Half's 10-day task
        // fills both weeks (5 days) against a 2.5-day capacity: 200%.
        assert_eq!(o.overloaded.len(), 1);
        assert_eq!(
            (
                o.overloaded[0].person.label.as_str(),
                o.overloaded[0].load_pct
            ),
            ("Half", 200.0)
        );
        // Exactly at capacity is not overloaded.
        let w = World_ {
            tasks: vec![task(1, "Full week", Some(5.0), None)],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![assigned(1, 30, None)],
            ..Default::default()
        };
        assert!(w.overview().overloaded.is_empty());
    }

    #[test]
    fn too_many_open_tasks_flags_a_person_even_without_estimates_or_load() {
        let mut w = World_ {
            people: vec![person(30, "Busy", 40.0), person(31, "Fine", 40.0)],
            ..Default::default()
        };
        // 11 tiny tasks (limit is 10) for one person, 10 for the other.
        for i in 0..11u128 {
            w.tasks.push(task(100 + i, &format!("b{i}"), None, None));
            w.edges.push(assigned(100 + i, 30, Some(1)));
        }
        for i in 0..10u128 {
            w.tasks.push(task(200 + i, &format!("f{i}"), None, None));
            w.edges.push(assigned(200 + i, 31, Some(1)));
        }
        let o = w.overview();
        assert_eq!(o.overloaded.len(), 1);
        assert_eq!(o.overloaded[0].person.label, "Busy");
        assert!(o.overloaded[0].over_task_limit);
        assert_eq!(o.overloaded[0].open_tasks, 11);
    }

    // -------------------------------------------------------------- robustness

    #[test]
    fn a_loop_in_the_plan_is_a_warning_not_a_failure() {
        let w = World_ {
            tasks: vec![
                task(1, "A", Some(1.0), Some(10)),
                task(2, "B", Some(1.0), Some(10)),
            ],
            edges: vec![blocks(1, 2), blocks(2, 1)],
            projects: vec![project(10, "P", Some(date!(2027 - 03 - 02)), 3)],
            ..Default::default()
        };
        let o = w.overview();
        assert_eq!(o.warnings.len(), 1);
        assert!(o.warnings[0].contains("loop"));
        // Health still works from what doesn't need dates.
        assert_ne!(project_row(&o, "P").health.level, HealthLevel::Idle);
        assert!(o.overloaded.is_empty());
    }

    #[test]
    fn archived_things_are_not_shown_and_empty_plans_are_fine() {
        let mut w = World_ {
            projects: vec![project(10, "Gone", None, 3)],
            objectives: vec![objective(20, "Gone too", None, 3)],
            ..Default::default()
        };
        w.projects[0].archived_at = Some(OffsetDateTime::UNIX_EPOCH);
        w.objectives[0].archived_at = Some(OffsetDateTime::UNIX_EPOCH);
        let o = w.overview();
        assert!(o.objectives.is_empty() && o.unlinked_projects.is_empty() && o.risks.is_empty());
        let empty = World_::default().overview();
        assert_eq!((empty.counts.red, empty.counts.green), (0, 0));
        assert!(empty.warnings.is_empty());
    }

    #[test]
    fn lookups_find_a_projects_row_and_an_objectives_row() {
        let mut w = at_risk();
        w.objectives = vec![objective(20, "Goal", None, 3)];
        w.edges.push(feeds(10, 20, None));
        w.projects.push(project(11, "Loose", None, 3));
        w.tasks.push(task(9, "t", Some(1.0), Some(11)));
        let o = w.overview();
        assert!(o.project_health(id(10)).is_some());
        assert!(o.project_health(id(11)).is_some());
        assert!(o.project_health(id(99)).is_none());
        assert!(o.objective_health(id(20)).is_some());
    }

    proptest! {
        /// More trouble never scores better, and the level always follows the thresholds.
        #[test]
        fn signal_scores_fall_with_the_value_and_levels_follow_the_thresholds(
            a in 1u32..50, extra in 0u32..50, v1 in 0.0f64..300.0, dv in 0.0f64..100.0,
        ) {
            let (amber, red) = (f64::from(a), f64::from(a + extra));
            let (lo, hi) = (v1, v1 + dv);
            prop_assert!(signal_score(hi, amber, red) <= signal_score(lo, amber, red));
            let level = level_of_score(signal_score(lo, amber, red));
            let want = if lo < amber { HealthLevel::Green } else if lo < red { HealthLevel::Amber } else { HealthLevel::Red };
            prop_assert_eq!(level, want, "v={} amber={} red={}", lo, amber, red);
        }

        /// Whatever the plan looks like, rows are internally consistent.
        #[test]
        fn overview_rows_are_consistent(
            durations in proptest::collection::vec(0u8..8, 1..8),
            late_target in 0u32..30,
            overdue_mask in proptest::collection::vec(any::<bool>(), 8),
        ) {
            let target = MON + time::Duration::days(i64::from(late_target));
            let mut w = World_ { projects: vec![project(10, "P", Some(target), 3)], ..Default::default() };
            for (i, d) in durations.iter().enumerate() {
                let mut t = task(i as u128 + 1, &format!("T{i}"), Some(f64::from(*d)), Some(10));
                if overdue_mask[i] {
                    t.due_date = Some(date!(2027 - 02 - 20));
                }
                w.tasks.push(t);
            }
            let o = w.overview();
            let p = project_row(&o, "P");
            prop_assert_eq!(level_of_score(p.health.score), p.health.level);
            prop_assert!(p.health.reasons.windows(2).all(|r| level_rank(r[0].level) <= level_rank(r[1].level)));
            prop_assert!(p.overdue_tasks as usize <= durations.len());
            prop_assert_eq!(p.open_tasks as usize, durations.len());
            prop_assert!(o.risks.len() <= TOP_RISKS);
            prop_assert!(o.risks.iter().all(|r| matches!(r.health.level, HealthLevel::Amber | HealthLevel::Red)));
            prop_assert!(o.risks.windows(2).all(|r| r[0].risk_score >= r[1].risk_score));
        }
    }
}
