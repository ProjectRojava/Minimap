//! Impact analysis: "what if this slips?" Pure and read-only.
//!
//! A scenario holds slips on tasks or projects. A slip holds a task back: its earliest start
//! becomes the baseline start plus the slip (a project slip does that to all its open tasks).
//! The scheduler (`schedule`) is then re-run and compared with the baseline, so the ripple
//! through `blocks` links, slack, targets and cross-project work is exactly what the schedule
//! says. Two things go beyond the task schedule:
//!
//! - **`depends_on` between projects** is treated as finish-to-start: if the project it needs
//!   finishes `d` days later and the plan left `s` days of room between the two, the dependent
//!   project's work starts `max(0, d - s)` days later (repeated until nothing changes).
//! - The report adds **objectives** (a contributing project or task finishes later) and
//!   **people** (their assigned tasks move).
//!
//! At every task, `incoming = absorbed + delay`: what arrived is either soaked up by slack or
//! passed on as a later finish. Nothing ever moves earlier.

use std::collections::{BTreeMap, HashMap, HashSet};

use minimap_types::{
    ApplyPreview, DateChange, Edge, EdgeType, ImpactObjective, ImpactPerson, ImpactPersonTask,
    ImpactProject, ImpactReport, ImpactSummary, ImpactTask, NodeRef, NodeSummary, NodeType,
    Objective, Person, Project, ProjectRoute, RouteBar, Schedule, ScheduleScope, ScheduledTask,
    Slip, Task, TaskStatus, Turn, TurnKind, Uuid, WorkWeek, MAX_TURNS,
};
use time::Date;

use crate::schedule::{
    chain_of, compute, compute_with, date_of, end_index, working_index, ScheduleError,
};

const EPS: f64 = 1e-9;
/// More rounds than any plan needs (project dependencies are acyclic); a safety stop.
const MAX_ROUNDS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImpactError {
    #[error("Pick something that slips")]
    Empty,
    #[error("A slip must be at least 1 working day")]
    NoDays,
    #[error("That {0} doesn't exist (or is archived)")]
    Unknown(NodeType),
    #[error("Only tasks and projects can slip, not a {0}")]
    WrongType(NodeType),
    #[error("\"{0}\" is finished or cancelled, so it can't slip")]
    Closed(String),
    #[error("The blocks links form a loop, so the work can't be scheduled")]
    Cycle,
}

impl From<ScheduleError> for ImpactError {
    fn from(_: ScheduleError) -> Self {
        ImpactError::Cycle
    }
}

/// Everything the analysis reads. All of it is the active (non-archived) data.
pub struct World<'a> {
    pub tasks: &'a [Task],
    pub edges: &'a [Edge],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    pub people: &'a [Person],
    pub today: Date,
    pub work_week: WorkWeek,
}

/// Working days by which `finish` is past `due` (the end of the due day), if it is.
pub fn late_working_days(week: WorkWeek, finish: Date, due: Date) -> Option<u32> {
    let d = end_index(week, finish) - end_index(week, due);
    (d > 0).then_some(d as u32)
}

struct Scenario {
    base: Schedule,
    scen: Schedule,
    /// Extra working days each open task is held, from the slips themselves.
    direct: HashMap<Uuid, f64>,
    /// Extra working days each project's open tasks are held, from `depends_on` knock-on.
    shift: HashMap<Uuid, f64>,
}

fn is_open(t: &Task) -> bool {
    !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
}

fn by_id(s: &Schedule) -> HashMap<Uuid, &ScheduledTask> {
    s.tasks.iter().map(|t| (t.id, t)).collect()
}

/// Earliest start / latest finish of a project's open tasks.
fn span(s: &Schedule) -> (HashMap<Uuid, f64>, HashMap<Uuid, f64>) {
    let mut start: HashMap<Uuid, f64> = HashMap::new();
    let mut finish: HashMap<Uuid, f64> = HashMap::new();
    for t in s.tasks.iter().filter(|t| !t.done) {
        if let Some(p) = t.project_id {
            let a = start.entry(p).or_insert(f64::INFINITY);
            *a = a.min(t.es);
            let b = finish.entry(p).or_insert(f64::NEG_INFINITY);
            *b = b.max(t.ef);
        }
    }
    (start, finish)
}

fn scenario(world: &World, slips: &[Slip]) -> Result<Scenario, ImpactError> {
    if slips.is_empty() {
        return Err(ImpactError::Empty);
    }
    let base = compute(
        world.tasks,
        world.edges,
        world.projects,
        world.today,
        world.work_week,
        ScheduleScope::Portfolio,
    )?;
    let base_tasks = by_id(&base);

    let mut direct: HashMap<Uuid, f64> = HashMap::new();
    for slip in slips {
        if slip.days == 0 {
            return Err(ImpactError::NoDays);
        }
        let days = f64::from(slip.days);
        match slip.node.node_type {
            NodeType::Task => {
                let task = world
                    .tasks
                    .iter()
                    .find(|t| t.id == slip.node.id && t.archived_at.is_none())
                    .ok_or(ImpactError::Unknown(NodeType::Task))?;
                if !is_open(task) {
                    return Err(ImpactError::Closed(task.title.clone()));
                }
                *direct.entry(task.id).or_insert(0.0) += days;
            }
            NodeType::Project => {
                if !world.projects.iter().any(|p| p.id == slip.node.id) {
                    return Err(ImpactError::Unknown(NodeType::Project));
                }
                for t in base_tasks
                    .values()
                    .filter(|t| !t.done && t.project_id == Some(slip.node.id))
                {
                    *direct.entry(t.id).or_insert(0.0) += days;
                }
            }
            other => return Err(ImpactError::WrongType(other)),
        }
    }

    // Project dependencies: A depends_on B (A needs B first).
    let needs: Vec<(Uuid, Uuid)> = world
        .edges
        .iter()
        .filter(|e| {
            e.edge_type == EdgeType::DependsOn
                && e.archived_at.is_none()
                && e.from_type == NodeType::Project
        })
        .map(|e| (e.from_id, e.to_id))
        .collect();
    let (base_start, base_finish) = span(&base);
    let mut shift: HashMap<Uuid, f64> = HashMap::new();
    let mut scen = base.clone();
    for _ in 0..MAX_ROUNDS {
        let mut not_before: HashMap<Uuid, f64> = HashMap::new();
        for t in base_tasks.values().filter(|t| !t.done) {
            let held = direct.get(&t.id).copied().unwrap_or(0.0).max(
                t.project_id
                    .and_then(|p| shift.get(&p))
                    .copied()
                    .unwrap_or(0.0),
            );
            if held > EPS {
                not_before.insert(t.id, t.es + held);
            }
        }
        scen = compute_with(
            world.tasks,
            world.edges,
            world.projects,
            world.today,
            world.work_week,
            ScheduleScope::Portfolio,
            &not_before,
        )?;
        let (_, scen_finish) = span(&scen);
        let mut changed = false;
        for (a, b) in &needs {
            let (Some(sa), Some(fb), Some(nb)) =
                (base_start.get(a), base_finish.get(b), scen_finish.get(b))
            else {
                continue;
            };
            let moved = (nb - fb).max(0.0);
            let room = (sa - fb).max(0.0);
            let need = moved - room;
            if need > shift.get(a).copied().unwrap_or(0.0) + EPS {
                shift.insert(*a, need);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(Scenario {
        base,
        scen,
        direct,
        shift,
    })
}

fn clean(x: f64) -> f64 {
    if x.abs() < EPS {
        0.0
    } else {
        // Remove float dust (0.1 + 0.2 style) without hiding half days.
        (x * 1e6).round() / 1e6
    }
}

/// Runs the scenario and reports what it moves.
pub fn analyze(world: &World, slips: &[Slip]) -> Result<ImpactReport, ImpactError> {
    let sc = scenario(world, slips)?;
    let base = by_id(&sc.base);
    let scen = by_id(&sc.scen);
    let task_by_id: HashMap<Uuid, &Task> = world.tasks.iter().map(|t| (t.id, t)).collect();
    let project_title: HashMap<Uuid, &str> = world
        .projects
        .iter()
        .map(|p| (p.id, p.title.as_str()))
        .collect();

    // Open tasks' blocking predecessors.
    let mut preds: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in world
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks && e.archived_at.is_none())
    {
        if base.get(&e.from_id).is_some_and(|t| !t.done)
            && base.get(&e.to_id).is_some_and(|t| !t.done)
        {
            preds.entry(e.to_id).or_default().push(e.from_id);
        }
    }

    // Per-task delay, then (in the order of the schedule) what reached it.
    let delay: HashMap<Uuid, f64> = base
        .iter()
        .filter(|(_, t)| !t.done)
        .filter_map(|(id, b)| scen.get(id).map(|s| (*id, clean((s.ef - b.ef).max(0.0)))))
        .collect();

    let mut tasks: Vec<ImpactTask> = Vec::new();
    for b in sc.base.tasks.iter().filter(|t| !t.done) {
        let Some(s) = scen.get(&b.id) else { continue };
        let own = delay.get(&b.id).copied().unwrap_or(0.0);
        let direct = sc.direct.get(&b.id).copied().unwrap_or(0.0);
        let project_shift = b
            .project_id
            .and_then(|p| sc.shift.get(&p))
            .copied()
            .unwrap_or(0.0);
        let from_preds = preds
            .get(&b.id)
            .into_iter()
            .flatten()
            .filter_map(|p| delay.get(p))
            .fold(0.0_f64, |a, d| a.max(*d));
        let incoming = clean(direct.max(project_shift).max(from_preds));
        if incoming <= EPS {
            continue;
        }
        let due = task_by_id.get(&b.id).and_then(|t| t.due_date);
        let late_before = due.and_then(|d| late_working_days(world.work_week, b.finish, d));
        let late_after = due.and_then(|d| late_working_days(world.work_week, s.finish, d));
        tasks.push(ImpactTask {
            id: b.id,
            title: b.title.clone(),
            project_id: b.project_id,
            project_title: b
                .project_id
                .and_then(|p| project_title.get(&p))
                .map(|t| (*t).to_owned()),
            direct: direct > EPS,
            incoming_days: incoming,
            absorbed_days: clean((incoming - own).max(0.0)),
            delay_days: own,
            old_start: b.start,
            new_start: s.start,
            old_finish: b.finish,
            new_finish: s.finish,
            due_date: due,
            late_before,
            late_after,
            newly_late: late_after.is_some() && late_before.is_none(),
        });
    }
    tasks.sort_by(|a, b| {
        b.direct
            .cmp(&a.direct)
            .then_with(|| a.old_start.cmp(&b.old_start))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });

    // Projects.
    let base_forecast: HashMap<Uuid, &minimap_types::ProjectForecast> =
        sc.base.projects.iter().map(|f| (f.project_id, f)).collect();
    let slipped_projects: Vec<Uuid> = slips
        .iter()
        .filter(|s| s.node.node_type == NodeType::Project)
        .map(|s| s.node.id)
        .collect();
    let mut projects: Vec<ImpactProject> = Vec::new();
    for after in &sc.scen.projects {
        let Some(before) = base_forecast.get(&after.project_id) else {
            continue;
        };
        let delay = match (before.finish_offset, after.finish_offset) {
            (Some(a), Some(b)) if after.open_tasks > 0 => clean((b - a).max(0.0)),
            _ => 0.0,
        };
        let direct = slipped_projects.contains(&after.project_id);
        if delay <= EPS && !direct {
            continue;
        }
        let affected = tasks
            .iter()
            .filter(|t| t.project_id == Some(after.project_id) && t.delay_days > EPS)
            .count() as u32;
        projects.push(ImpactProject {
            id: after.project_id,
            title: after.title.clone(),
            direct,
            delay_days: delay,
            affected_tasks: affected,
            old_finish: before.projected_finish,
            new_finish: after.projected_finish,
            target_date: after.target_date,
            late_before: before.late_by_days,
            late_after: after.late_by_days,
            newly_late: after.late_by_days.is_some() && before.late_by_days.is_none(),
        });
    }
    projects.sort_by(|a, b| {
        b.delay_days
            .total_cmp(&a.delay_days)
            .then_with(|| a.title.cmp(&b.title))
    });

    // Objectives: the latest finish among what contributes to them.
    let after_forecast: HashMap<Uuid, &minimap_types::ProjectForecast> =
        sc.scen.projects.iter().map(|f| (f.project_id, f)).collect();
    struct Contribution {
        name: String,
        old: (f64, Date),
        new: (f64, Date),
    }
    let mut contributions: BTreeMap<Uuid, Vec<Contribution>> = BTreeMap::new();
    for e in world.edges.iter().filter(|e| {
        e.edge_type == EdgeType::ContributesTo
            && e.archived_at.is_none()
            && e.to_type == NodeType::Objective
    }) {
        let item = match e.from_type {
            NodeType::Project => {
                match (
                    base_forecast.get(&e.from_id),
                    after_forecast.get(&e.from_id),
                ) {
                    (Some(b), Some(a)) if b.open_tasks > 0 => {
                        match (
                            b.finish_offset,
                            b.projected_finish,
                            a.finish_offset,
                            a.projected_finish,
                        ) {
                            (Some(bo), Some(bd), Some(ao), Some(ad)) => Some(Contribution {
                                name: b.title.clone(),
                                old: (bo, bd),
                                new: (ao, ad),
                            }),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            NodeType::Task => match (base.get(&e.from_id), scen.get(&e.from_id)) {
                (Some(b), Some(a)) if !b.done => Some(Contribution {
                    name: b.title.clone(),
                    old: (b.ef, b.finish),
                    new: (a.ef, a.finish),
                }),
                _ => None,
            },
            _ => None,
        };
        if let Some(c) = item {
            contributions.entry(e.to_id).or_default().push(c);
        }
    }
    let mut objectives: Vec<ImpactObjective> = Vec::new();
    for o in world.objectives {
        let Some(list) = contributions.get(&o.id) else {
            continue;
        };
        let latest = |pick: fn(&Contribution) -> (f64, Date)| {
            list.iter().map(pick).max_by(|a, b| a.0.total_cmp(&b.0))
        };
        let (Some(old), Some(new)) = (latest(|c| c.old), latest(|c| c.new)) else {
            continue;
        };
        let delay = clean((new.0 - old.0).max(0.0));
        if delay <= EPS {
            continue;
        }
        let late_before = o
            .target_date
            .and_then(|t| late_working_days(world.work_week, old.1, t));
        let late_after = o
            .target_date
            .and_then(|t| late_working_days(world.work_week, new.1, t));
        let mut names: Vec<String> = list
            .iter()
            .filter(|c| c.new.0 > c.old.0 + EPS)
            .map(|c| c.name.clone())
            .collect();
        names.sort();
        objectives.push(ImpactObjective {
            id: o.id,
            title: o.title.clone(),
            delay_days: delay,
            old_finish: Some(old.1),
            new_finish: Some(new.1),
            target_date: o.target_date,
            late_before,
            late_after,
            newly_late: late_after.is_some() && late_before.is_none(),
            contributors: names,
        });
    }
    objectives.sort_by(|a, b| {
        b.delay_days
            .total_cmp(&a.delay_days)
            .then_with(|| a.title.cmp(&b.title))
    });

    // People: whoever is assigned to a task that finishes later.
    let person_name: HashMap<Uuid, &str> = world
        .people
        .iter()
        .map(|p| (p.id, p.name.as_str()))
        .collect();
    let moved: HashMap<Uuid, &ImpactTask> = tasks
        .iter()
        .filter(|t| t.delay_days > EPS)
        .map(|t| (t.id, t))
        .collect();
    let mut by_person: BTreeMap<Uuid, Vec<ImpactPersonTask>> = BTreeMap::new();
    for e in world.edges.iter().filter(|e| {
        e.edge_type == EdgeType::AssignedTo
            && e.archived_at.is_none()
            && e.from_type == NodeType::Task
    }) {
        if let (Some(t), true) = (moved.get(&e.from_id), person_name.contains_key(&e.to_id)) {
            by_person
                .entry(e.to_id)
                .or_default()
                .push(ImpactPersonTask {
                    id: t.id,
                    title: t.title.clone(),
                    delay_days: t.delay_days,
                    new_finish: t.new_finish,
                });
        }
    }
    let mut people: Vec<ImpactPerson> = by_person
        .into_iter()
        .map(|(id, mut list)| {
            list.sort_by(|a, b| {
                b.delay_days
                    .total_cmp(&a.delay_days)
                    .then_with(|| a.title.cmp(&b.title))
            });
            ImpactPerson {
                id,
                name: person_name.get(&id).copied().unwrap_or("").to_owned(),
                delay_days: list.iter().fold(0.0_f64, |a, t| a.max(t.delay_days)),
                tasks: list,
            }
        })
        .collect();
    people.sort_by(|a, b| {
        b.delay_days
            .total_cmp(&a.delay_days)
            .then_with(|| a.name.cmp(&b.name))
    });

    let slip_labels: Vec<(NodeSummary, u32)> = slips
        .iter()
        .map(|s| {
            let label = match s.node.node_type {
                NodeType::Task => task_by_id.get(&s.node.id).map(|t| t.title.clone()),
                _ => project_title.get(&s.node.id).map(|t| (*t).to_owned()),
            }
            .unwrap_or_default();
            (
                NodeSummary {
                    node: s.node,
                    label,
                    archived: false,
                },
                s.days,
            )
        })
        .collect();
    let summary = ImpactSummary {
        moved_tasks: tasks.iter().filter(|t| t.delay_days > EPS).count() as u32,
        absorbing_tasks: tasks.iter().filter(|t| t.delay_days <= EPS).count() as u32,
        newly_late_tasks: tasks.iter().filter(|t| t.newly_late).count() as u32,
        newly_late_projects: projects.iter().filter(|p| p.newly_late).count() as u32,
        newly_late_objectives: objectives.iter().filter(|o| o.newly_late).count() as u32,
        people: people.len() as u32,
        worst_delay_days: tasks.iter().fold(0.0_f64, |a, t| a.max(t.delay_days)),
    };
    let routes = project_routes(
        world,
        &sc,
        &Reached {
            tasks: &tasks,
            delay: &delay,
            preds: &preds,
        },
        &projects,
        &base_forecast,
    );
    Ok(ImpactReport {
        slips: slip_labels,
        summary,
        tasks,
        projects,
        objectives,
        people,
        routes,
    })
}

/// A project a route is drawn for: what the report says about its finish.
struct Head {
    id: Uuid,
    title: String,
    old_finish: Option<Date>,
    new_finish: Option<Date>,
    target_date: Option<Date>,
    delay_days: f64,
    late_after: Option<u32>,
    newly_late: bool,
}

/// What the report knows about how the slip travelled, for the routes.
struct Reached<'a> {
    tasks: &'a [ImpactTask],
    /// Each open task's own finish delay.
    delay: &'a HashMap<Uuid, f64>,
    /// Open tasks' blocking predecessors.
    preds: &'a HashMap<Uuid, Vec<Uuid>>,
}

/// The route to the finish of every affected project, before and after (spec 46): the critical
/// chain each time, and a turn-by-turn list of where the slip enters, is absorbed, passes on and
/// arrives.
fn project_routes(
    world: &World,
    sc: &Scenario,
    reached: &Reached,
    projects: &[ImpactProject],
    base_forecast: &HashMap<Uuid, &minimap_types::ProjectForecast>,
) -> Vec<ProjectRoute> {
    let base = by_id(&sc.base);
    let scen = by_id(&sc.scen);
    let week = world.work_week;
    let t0 = working_index(week, world.today);
    let summary = |id: Uuid, title: &str| NodeSummary {
        node: NodeRef::new(NodeType::Task, id),
        label: title.to_owned(),
        archived: false,
    };
    // The projects the slip changes, as the report lists them, then those it only reached and
    // that absorbed it (their route is unchanged, and that is worth saying).
    let mut heads: Vec<Head> = projects
        .iter()
        .map(|p| Head {
            id: p.id,
            title: p.title.clone(),
            old_finish: p.old_finish,
            new_finish: p.new_finish,
            target_date: p.target_date,
            delay_days: p.delay_days,
            late_after: p.late_after,
            newly_late: p.newly_late,
        })
        .collect();
    let listed: HashSet<Uuid> = heads.iter().map(|h| h.id).collect();
    let after_forecast: HashMap<Uuid, &minimap_types::ProjectForecast> =
        sc.scen.projects.iter().map(|f| (f.project_id, f)).collect();
    let mut absorbed: Vec<Head> = reached
        .tasks
        .iter()
        .filter_map(|t| t.project_id)
        .collect::<HashSet<Uuid>>()
        .into_iter()
        .filter(|id| !listed.contains(id))
        .filter_map(|id| {
            let (b, a) = (base_forecast.get(&id)?, after_forecast.get(&id)?);
            Some(Head {
                id,
                title: a.title.clone(),
                old_finish: b.projected_finish,
                new_finish: a.projected_finish,
                target_date: a.target_date,
                delay_days: 0.0,
                late_after: a.late_by_days,
                newly_late: false,
            })
        })
        .collect();
    absorbed.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)));
    heads.extend(absorbed);
    heads
        .iter()
        .map(|p| {
            let before = chain_of(&sc.base, p.id);
            let after = chain_of(&sc.scen, p.id);
            let on_before: HashSet<Uuid> = before.iter().copied().collect();
            let on_after: HashSet<Uuid> = after.iter().copied().collect();
            let mine: HashMap<Uuid, &ImpactTask> = reached
                .tasks
                .iter()
                .filter(|t| t.project_id == Some(p.id))
                .map(|t| (t.id, t))
                .collect();
            let shift = sc.shift.get(&p.id).copied().unwrap_or(0.0);
            let delayed_pred = |id: &Uuid| {
                reached.preds.get(id).is_some_and(|ps| {
                    ps.iter()
                        .any(|q| reached.delay.get(q).is_some_and(|d| *d > EPS))
                })
            };

            // The tasks that matter to the route: what the slip reached, and what joined or left it.
            let mut ids: Vec<Uuid> = mine.keys().copied().collect();
            ids.extend(on_before.symmetric_difference(&on_after).copied());
            ids.sort();
            ids.dedup();
            let mut steps: Vec<(f64, Turn)> = Vec::new();
            for id in &ids {
                let (Some(b), Some(s)) = (base.get(id), scen.get(id)) else {
                    continue;
                };
                let hit = mine.get(id).copied();
                let (incoming, absorbed, delay) = hit.map_or((0.0, 0.0, 0.0), |t| {
                    (t.incoming_days, t.absorbed_days, t.delay_days)
                });
                let starts = hit.is_some_and(|t| t.direct || (shift > EPS && !delayed_pred(id)));
                let kind = if starts {
                    TurnKind::Slips
                } else if on_after.contains(id) && !on_before.contains(id) {
                    TurnKind::JoinsRoute
                } else if on_before.contains(id) && !on_after.contains(id) {
                    TurnKind::LeavesRoute
                } else if hit.is_none() {
                    continue;
                } else if delay > EPS && absorbed <= EPS {
                    TurnKind::PassesOn
                } else if absorbed > EPS && delay <= EPS {
                    TurnKind::Absorbs
                } else {
                    TurnKind::PartlyAbsorbs
                };
                steps.push((
                    s.es,
                    Turn {
                        task: summary(*id, &b.title),
                        kind,
                        incoming_days: incoming,
                        absorbed_days: absorbed,
                        delay_days: delay,
                        new_finish: s.finish,
                        also: 0,
                    },
                ));
            }
            steps.sort_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then_with(|| a.1.task.label.cmp(&b.1.task.label))
                    .then_with(|| a.1.task.node.id.cmp(&b.1.task.node.id))
            });
            let mut turns: Vec<Turn> = steps.into_iter().map(|(_, t)| t).collect();

            // The last step is always where the project ends up.
            let last = sc
                .scen
                .tasks
                .iter()
                .filter(|t| !t.done && t.project_id == Some(p.id))
                .max_by(|a, b| {
                    a.ef.total_cmp(&b.ef)
                        .then_with(|| b.title.cmp(&a.title))
                        .then_with(|| b.id.cmp(&a.id))
                });
            if let Some(last) = last {
                // A task that is both where the slip starts and where the project ends keeps its
                // first step and gets the arrival after it; any other last task just arrives.
                turns.retain(|t| t.task.node.id != last.id || t.kind == TurnKind::Slips);
                turns.push(Turn {
                    task: summary(last.id, &last.title),
                    kind: TurnKind::Arrives,
                    incoming_days: p.delay_days,
                    absorbed_days: 0.0,
                    delay_days: p.delay_days,
                    new_finish: p.new_finish.unwrap_or(last.finish),
                    also: 0,
                });
            }

            // A run of steps that only pass the same delay on is one step ("and 2 more").
            let mut folded: Vec<Turn> = Vec::new();
            for turn in turns {
                match folded.last_mut() {
                    Some(prev)
                        if prev.kind == TurnKind::PassesOn
                            && turn.kind == TurnKind::PassesOn
                            && (prev.delay_days - turn.delay_days).abs() <= EPS =>
                    {
                        prev.also += 1;
                        prev.new_finish = turn.new_finish;
                    }
                    _ => folded.push(turn),
                }
            }
            // A long route keeps its first and last steps and says how many it left out.
            if folded.len() > MAX_TURNS {
                let keep_end = 3;
                let keep_start = MAX_TURNS - keep_end - 1;
                let hidden = folded.len() - keep_start - keep_end;
                let anchor = folded[keep_start].clone();
                let tail = folded.split_off(folded.len() - keep_end);
                folded.truncate(keep_start);
                folded.push(Turn {
                    kind: TurnKind::More,
                    incoming_days: 0.0,
                    absorbed_days: 0.0,
                    delay_days: 0.0,
                    also: hidden as u32,
                    ..anchor
                });
                folded.extend(tail);
            }

            // What to draw: both routes and everything the slip reached in the project.
            let mut drawn: Vec<Uuid> = before
                .iter()
                .chain(after.iter())
                .copied()
                .chain(mine.keys().copied())
                .collect();
            drawn.sort();
            drawn.dedup();
            let mut bars: Vec<RouteBar> = drawn
                .iter()
                .filter_map(|id| {
                    let (b, s) = (base.get(id)?, scen.get(id)?);
                    Some(RouteBar {
                        id: *id,
                        title: b.title.clone(),
                        old_es: b.es,
                        old_ef: b.ef,
                        new_es: s.es,
                        new_ef: s.ef,
                        on_old_route: on_before.contains(id),
                        on_new_route: on_after.contains(id),
                    })
                })
                .collect();
            bars.sort_by(|a, b| {
                a.new_es
                    .total_cmp(&b.new_es)
                    .then_with(|| a.title.cmp(&b.title))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let target_offset = base_forecast.get(&p.id).and_then(|f| f.target_offset);
            let mut lo: i64 = 0;
            let mut hi: i64 = 1;
            for b in &bars {
                lo = lo.min(b.old_es.min(b.new_es).floor() as i64);
                hi = hi.max(b.old_ef.max(b.new_ef).ceil() as i64);
            }
            if let Some(t) = target_offset {
                lo = lo.min(t.floor() as i64 - 1);
                hi = hi.max(t.ceil() as i64);
            }
            let days = (lo..hi).map(|k| date_of(week, t0 + k)).collect();
            ProjectRoute {
                project_id: p.id,
                title: p.title.clone(),
                changed: before != after,
                before,
                after,
                old_finish: p.old_finish,
                new_finish: p.new_finish,
                target_date: p.target_date,
                delay_days: p.delay_days,
                late_after: p.late_after,
                newly_late: p.newly_late,
                turns: folded,
                bars,
                first_offset: lo,
                days,
                target_offset,
            }
        })
        .collect()
}

/// What recording the scenario in the plan would change: the tasks the slips hold back (and
/// those a `depends_on` project shift holds back) get a start date, and their due dates move by
/// the same number of working days. Tasks that are merely pushed along by `blocks` links are
/// left alone: the schedule moves them by itself, and pinning them would freeze them.
/// Targets are never changed. After applying these, the baseline schedule equals the scenario.
pub fn apply_plan(world: &World, slips: &[Slip]) -> Result<ApplyPreview, ImpactError> {
    let sc = scenario(world, slips)?;
    let base = by_id(&sc.base);
    let week = world.work_week;
    let t0 = working_index(week, world.today);
    let mut changes = Vec::new();
    for t in world
        .tasks
        .iter()
        .filter(|t| is_open(t) && t.archived_at.is_none())
    {
        let Some(b) = base.get(&t.id) else { continue };
        let held = sc.direct.get(&t.id).copied().unwrap_or(0.0).max(
            t.project_id
                .and_then(|p| sc.shift.get(&p))
                .copied()
                .unwrap_or(0.0),
        );
        if held <= EPS {
            continue;
        }
        let new_start = date_of(week, t0 + (b.es + held + EPS).floor() as i64);
        let new_due = t
            .due_date
            .map(|d| date_of(week, end_index(week, d) - 1 + held.ceil() as i64));
        changes.push(DateChange {
            task_id: t.id,
            title: t.title.clone(),
            old_start: t.start_date,
            new_start,
            old_due: t.due_date,
            new_due,
        });
    }
    changes.sort_by(|a, b| {
        a.new_start
            .cmp(&b.new_start)
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(ApplyPreview { changes })
}

/// A slip on a task or project, for callers building a scenario.
pub fn slip(node: NodeRef, days: u32) -> Slip {
    Slip { node, days }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{ObjectiveStatus, ProjectStatus};
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    // 2027-03-01 is a Monday.
    const MON: Date = date!(2027 - 03 - 01);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, days: f64) -> Task {
        Task {
            order_step: None,
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
            project_id: None,
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

    fn in_project(mut t: Task, p: u128) -> Task {
        t.project_id = Some(id(p));
        t
    }

    fn edge(kind: EdgeType, ft: NodeType, from: u128, tt: NodeType, to: u128) -> Edge {
        Edge {
            id: Uuid::from_u128(10_000 + from * 1000 + to + u128::from(kind as u8) * 7),
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

    fn objective(n: u128, title: &str, target: Option<Date>) -> Objective {
        Objective {
            confirmed_on: None,

            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            id: id(n),
            title: title.into(),
            description: String::new(),
            target_date: target,
            status: ObjectiveStatus::OnTrack,
            priority: 3,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

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

    fn slip_task(n: u128, days: u32) -> Slip {
        Slip {
            node: NodeRef::new(NodeType::Task, id(n)),
            days,
        }
    }

    fn slip_project(n: u128, days: u32) -> Slip {
        Slip {
            node: NodeRef::new(NodeType::Project, id(n)),
            days,
        }
    }

    struct Fixture {
        tasks: Vec<Task>,
        edges: Vec<Edge>,
        projects: Vec<Project>,
        objectives: Vec<Objective>,
        people: Vec<Person>,
    }

    impl Fixture {
        fn new(tasks: Vec<Task>, edges: Vec<Edge>) -> Self {
            Fixture {
                tasks,
                edges,
                projects: vec![],
                objectives: vec![],
                people: vec![],
            }
        }

        fn world(&self) -> World<'_> {
            World {
                tasks: &self.tasks,
                edges: &self.edges,
                projects: &self.projects,
                objectives: &self.objectives,
                people: &self.people,
                today: MON,
                work_week: WorkWeek::MON_FRI,
            }
        }

        fn report(&self, slips: &[Slip]) -> ImpactReport {
            analyze(&self.world(), slips).unwrap()
        }
    }

    fn row<'a>(r: &'a ImpactReport, title: &str) -> &'a ImpactTask {
        r.tasks
            .iter()
            .find(|t| t.title == title)
            .unwrap_or_else(|| panic!("{title} not in report"))
    }

    // ----------------------------------------------------------- hand-built

    #[test]
    fn a_slip_on_a_tight_chain_passes_all_the_way_down() {
        let f = Fixture::new(
            vec![task(1, "A", 2.0), task(2, "B", 3.0), task(3, "C", 1.0)],
            vec![blocks(1, 2), blocks(2, 3)],
        );
        let r = f.report(&[slip_task(1, 2)]);
        let a = row(&r, "A");
        assert!(a.direct);
        assert_eq!(
            (a.incoming_days, a.absorbed_days, a.delay_days),
            (2.0, 0.0, 2.0)
        );
        assert_eq!(
            (a.old_finish, a.new_finish),
            (date!(2027 - 03 - 02), date!(2027 - 03 - 04))
        );
        for t in ["B", "C"] {
            let b = row(&r, t);
            assert!(!b.direct);
            assert_eq!(
                (b.incoming_days, b.absorbed_days, b.delay_days),
                (2.0, 0.0, 2.0),
                "{t}"
            );
        }
        assert_eq!(row(&r, "C").old_finish, date!(2027 - 03 - 08));
        assert_eq!(row(&r, "C").new_finish, date!(2027 - 03 - 10));
        assert_eq!(r.summary.moved_tasks, 3);
        assert_eq!(r.summary.absorbing_tasks, 0);
        assert_eq!(r.summary.worst_delay_days, 2.0);
        // Direct first, then by start.
        let order: Vec<&str> = r.tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(order, ["A", "B", "C"]);
    }

    #[test]
    fn slack_absorbs_a_slip_and_the_remainder_passes_on() {
        // A(3) -> B(2), A -> C(4), B -> D(1), C -> D: B has 2 days of slack before D.
        let f = Fixture::new(
            vec![
                task(1, "A", 3.0),
                task(2, "B", 2.0),
                task(3, "C", 4.0),
                task(4, "D", 1.0),
            ],
            vec![blocks(1, 2), blocks(1, 3), blocks(2, 4), blocks(3, 4)],
        );
        // 1 day: B moves, D absorbs it entirely (and is listed as where the slip stopped).
        let r = f.report(&[slip_task(2, 1)]);
        let d = row(&r, "D");
        assert_eq!(
            (d.incoming_days, d.absorbed_days, d.delay_days),
            (1.0, 1.0, 0.0)
        );
        assert_eq!(d.old_finish, d.new_finish);
        assert_eq!((r.summary.moved_tasks, r.summary.absorbing_tasks), (1, 1));
        assert!(
            r.tasks.iter().all(|t| t.title != "C"),
            "C isn't downstream of B"
        );
        // 3 days: 2 absorbed, 1 passed on.
        let r = f.report(&[slip_task(2, 3)]);
        let d = row(&r, "D");
        assert_eq!(
            (d.incoming_days, d.absorbed_days, d.delay_days),
            (3.0, 2.0, 1.0)
        );
        assert_eq!(r.summary.worst_delay_days, 3.0);
        // The critical chain has no slack: the whole slip lands.
        let r = f.report(&[slip_task(3, 2)]);
        assert_eq!(row(&r, "D").delay_days, 2.0);
    }

    #[test]
    fn lag_and_start_dates_are_slack_too() {
        // B can't start until Thursday anyway, so a day of slip on A (which ends Monday)
        // costs nothing; four days does.
        let mut b = task(2, "B", 1.0);
        b.start_date = Some(date!(2027 - 03 - 04));
        let f = Fixture::new(vec![task(1, "A", 1.0), b], vec![blocks(1, 2)]);
        assert_eq!(row(&f.report(&[slip_task(1, 1)]), "B").delay_days, 0.0);
        let r = f.report(&[slip_task(1, 4)]);
        assert_eq!(row(&r, "B").delay_days, 2.0);
        assert_eq!(row(&r, "B").absorbed_days, 2.0);
    }

    #[test]
    fn projects_objectives_and_people_follow_the_tasks() {
        // P: Design(2) blocks Q: Build(3). Build's project Q is due exactly on time.
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "Design", 2.0), 10),
                in_project(task(2, "Build", 3.0), 11),
            ],
            vec![
                blocks(1, 2),
                edge(
                    EdgeType::ContributesTo,
                    NodeType::Project,
                    11,
                    NodeType::Objective,
                    20,
                ),
                edge(
                    EdgeType::AssignedTo,
                    NodeType::Task,
                    2,
                    NodeType::Person,
                    30,
                ),
                edge(
                    EdgeType::AssignedTo,
                    NodeType::Task,
                    1,
                    NodeType::Person,
                    31,
                ),
            ],
        );
        // Build finishes Friday 2027-03-05; target the same day; the objective has a looser one.
        f.projects = vec![
            project(10, "P", None),
            project(11, "Q", Some(date!(2027 - 03 - 05))),
        ];
        f.objectives = vec![objective(20, "Launch", Some(date!(2027 - 03 - 05)))];
        f.people = vec![person(30, "Priya"), person(31, "Raj")];
        let r = f.report(&[slip_task(1, 2)]);

        let q = r.projects.iter().find(|p| p.title == "Q").unwrap();
        assert_eq!(q.delay_days, 2.0);
        assert_eq!(
            (q.old_finish, q.new_finish),
            (Some(date!(2027 - 03 - 05)), Some(date!(2027 - 03 - 09)))
        );
        assert_eq!(
            (q.late_before, q.late_after, q.newly_late),
            (None, Some(2), true)
        );
        let p = r.projects.iter().find(|p| p.title == "P").unwrap();
        assert_eq!((p.delay_days, p.affected_tasks), (2.0, 1));

        let o = &r.objectives[0];
        assert_eq!((o.title.as_str(), o.delay_days), ("Launch", 2.0));
        assert_eq!(
            (o.late_before, o.late_after, o.newly_late),
            (None, Some(2), true)
        );
        assert_eq!(o.contributors, ["Q"]);

        let names: Vec<(&str, f64)> = r
            .people
            .iter()
            .map(|p| (p.name.as_str(), p.delay_days))
            .collect();
        assert_eq!(names, [("Priya", 2.0), ("Raj", 2.0)]);
        assert_eq!(r.people[0].tasks[0].title, "Build");
        assert_eq!(r.summary.newly_late_projects, 1);
        assert_eq!(r.summary.newly_late_objectives, 1);
        assert_eq!(r.summary.people, 2);
    }

    #[test]
    fn tasks_with_due_dates_are_flagged_when_they_become_late() {
        let mut a = task(1, "A", 1.0);
        a.due_date = Some(date!(2027 - 03 - 02));
        let mut b = task(2, "B", 1.0);
        b.due_date = Some(date!(2027 - 03 - 01));
        let f = Fixture::new(vec![a, b], vec![blocks(1, 2)]);
        let r = f.report(&[slip_task(1, 2)]);
        // A finishes Mon, due Tue: slipping 2 makes it end Wed (1 day late).
        assert_eq!(
            (row(&r, "A").late_before, row(&r, "A").late_after),
            (None, Some(1))
        );
        assert!(row(&r, "A").newly_late);
        // B was already late (Tue finish vs Mon due) and gets later.
        assert_eq!(
            (row(&r, "B").late_before, row(&r, "B").late_after),
            (Some(1), Some(3))
        );
        assert!(!row(&r, "B").newly_late);
        assert_eq!(r.summary.newly_late_tasks, 1);
    }

    #[test]
    fn several_slips_combine_and_the_same_task_adds_up() {
        let f = Fixture::new(
            vec![
                task(1, "A", 1.0),
                task(2, "B", 1.0),
                task(3, "C", 1.0),
                task(4, "Lone", 1.0),
            ],
            vec![blocks(1, 3), blocks(2, 3)],
        );
        // A and B both feed C: the later of the two pushes it.
        let r = f.report(&[slip_task(1, 2), slip_task(2, 3)]);
        assert_eq!(row(&r, "C").delay_days, 3.0);
        assert!(r.tasks.iter().all(|t| t.title != "Lone"));
        // The same task slipped twice slips by the sum.
        let r = f.report(&[slip_task(1, 2), slip_task(1, 3)]);
        assert_eq!(row(&r, "A").delay_days, 5.0);
        assert_eq!(r.slips.len(), 2);
        assert_eq!(r.slips[0].0.label, "A");
    }

    #[test]
    fn slipping_a_project_holds_back_all_its_open_work() {
        let mut done = task(3, "Done", 1.0);
        done.status = TaskStatus::Done;
        done.completed_at = Some(date!(2027 - 02 - 26).midnight().assume_utc());
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "A", 2.0), 10),
                in_project(task(2, "B", 1.0), 10),
                in_project(done, 10),
                in_project(task(4, "Other", 1.0), 11),
            ],
            vec![],
        );
        f.projects = vec![project(10, "P", None), project(11, "Q", None)];
        let r = f.report(&[slip_project(10, 3)]);
        assert!(row(&r, "A").direct && row(&r, "B").direct);
        assert_eq!(row(&r, "A").delay_days, 3.0);
        assert!(r
            .tasks
            .iter()
            .all(|t| t.title != "Done" && t.title != "Other"));
        let p = r.projects.iter().find(|p| p.title == "P").unwrap();
        assert!(p.direct);
        assert_eq!(p.delay_days, 3.0);
        assert!(r.projects.iter().all(|p| p.title != "Q"));
    }

    #[test]
    fn a_dependent_project_waits_only_once_the_room_between_them_runs_out() {
        // Q depends on P. P finishes at the end of Wednesday (3 days); Q can't start until
        // next Monday (offset 5), so there are 2 working days of room.
        let mut q = in_project(task(2, "Q work", 1.0), 11);
        q.start_date = Some(date!(2027 - 03 - 08));
        let mut f = Fixture::new(
            vec![in_project(task(1, "P work", 3.0), 10), q],
            vec![edge(
                EdgeType::DependsOn,
                NodeType::Project,
                11,
                NodeType::Project,
                10,
            )],
        );
        f.projects = vec![project(10, "P", None), project(11, "Q", None)];
        // 2 days of slip fit in the room: Q is untouched.
        let r = f.report(&[slip_task(1, 2)]);
        assert!(r.tasks.iter().all(|t| t.title != "Q work"));
        assert!(r.projects.iter().all(|p| p.title != "Q"));
        // 5 days: 3 more than the room, so Q's work starts 3 days later.
        let r = f.report(&[slip_task(1, 5)]);
        let w = row(&r, "Q work");
        assert_eq!(
            (w.incoming_days, w.delay_days, w.absorbed_days),
            (3.0, 3.0, 0.0)
        );
        assert!(!w.direct);
        assert_eq!(
            r.projects
                .iter()
                .find(|p| p.title == "Q")
                .unwrap()
                .delay_days,
            3.0
        );
    }

    #[test]
    fn dependencies_chain_through_several_projects() {
        // C needs B needs A, no room anywhere: slipping A by 2 moves all three.
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "a", 1.0), 10),
                in_project(task(2, "b", 1.0), 11),
                in_project(task(3, "c", 1.0), 12),
            ],
            vec![
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
            ],
        );
        f.projects = vec![
            project(10, "A", None),
            project(11, "B", None),
            project(12, "C", None),
        ];
        let r = f.report(&[slip_task(1, 2)]);
        assert_eq!(row(&r, "b").delay_days, 2.0);
        assert_eq!(row(&r, "c").delay_days, 2.0);
    }

    #[test]
    fn bad_scenarios_are_refused_in_plain_words() {
        let mut done = task(2, "Shipped", 1.0);
        done.status = TaskStatus::Done;
        done.completed_at = Some(date!(2027 - 02 - 26).midnight().assume_utc());
        let mut f = Fixture::new(vec![task(1, "A", 1.0), done], vec![]);
        f.projects = vec![project(10, "P", None)];
        let w = f.world();
        let err = |s: &[Slip]| analyze(&w, s).unwrap_err();
        assert_eq!(err(&[]), ImpactError::Empty);
        assert_eq!(err(&[slip_task(1, 0)]), ImpactError::NoDays);
        assert_eq!(
            err(&[slip_task(99, 1)]),
            ImpactError::Unknown(NodeType::Task)
        );
        assert_eq!(
            err(&[slip_project(99, 1)]),
            ImpactError::Unknown(NodeType::Project)
        );
        assert_eq!(
            err(&[slip_task(2, 1)]),
            ImpactError::Closed("Shipped".into())
        );
        let person = Slip {
            node: NodeRef::new(NodeType::Person, id(1)),
            days: 1,
        };
        assert_eq!(err(&[person]), ImpactError::WrongType(NodeType::Person));
        assert!(err(&[slip_task(2, 1)]).to_string().contains("Shipped"));
        // A loop in the plan is reported, not hung on.
        let looped = Fixture::new(
            vec![task(1, "A", 1.0), task(2, "B", 1.0)],
            vec![blocks(1, 2), blocks(2, 1)],
        );
        assert_eq!(
            analyze(&looped.world(), &[slip_task(1, 1)]).unwrap_err(),
            ImpactError::Cycle
        );
    }

    #[test]
    fn nothing_is_reported_for_work_the_slip_cannot_reach() {
        let f = Fixture::new(vec![task(1, "A", 1.0), task(2, "Elsewhere", 1.0)], vec![]);
        let r = f.report(&[slip_task(1, 3)]);
        assert_eq!(r.tasks.len(), 1);
        assert!(r.objectives.is_empty() && r.people.is_empty());
    }

    // ----------------------------------------------------------- applying

    #[test]
    fn apply_pins_the_slipped_tasks_and_moves_their_due_dates() {
        let mut a = task(1, "A", 1.0);
        a.due_date = Some(date!(2027 - 03 - 05)); // Friday
        let f = Fixture::new(vec![a, task(2, "B", 1.0)], vec![blocks(1, 2)]);
        let plan = apply_plan(&f.world(), &[slip_task(1, 2)]).unwrap();
        // Only A is pinned; B is pushed by the schedule on its own.
        assert_eq!(plan.changes.len(), 1);
        let c = &plan.changes[0];
        assert_eq!(c.title, "A");
        assert_eq!(c.old_start, None);
        assert_eq!(c.new_start, date!(2027 - 03 - 03));
        // Friday + 2 working days = Tuesday.
        assert_eq!(c.new_due, Some(date!(2027 - 03 - 09)));
        // No due date, no due change.
        let f = Fixture::new(vec![task(1, "A", 1.0)], vec![]);
        let plan = apply_plan(&f.world(), &[slip_task(1, 1)]).unwrap();
        assert_eq!(plan.changes[0].new_due, None);
    }

    /// Writes the plan's dates into the tasks, as applying would.
    fn applied(tasks: &[Task], plan: &ApplyPreview) -> Vec<Task> {
        let mut out = tasks.to_vec();
        for c in &plan.changes {
            let t = out.iter_mut().find(|t| t.id == c.task_id).unwrap();
            t.start_date = Some(c.new_start);
            t.due_date = c.new_due;
        }
        out
    }

    fn finishes(f: &Fixture, tasks: &[Task]) -> HashMap<Uuid, f64> {
        compute(
            tasks,
            &f.edges,
            &f.projects,
            MON,
            WorkWeek::MON_FRI,
            ScheduleScope::Portfolio,
        )
        .unwrap()
        .tasks
        .iter()
        .map(|t| (t.id, t.ef))
        .collect()
    }

    #[test]
    fn applying_a_project_dependency_shift_reproduces_the_scenario() {
        let mut q = in_project(task(2, "Q work", 1.0), 11);
        q.start_date = Some(date!(2027 - 03 - 08));
        let mut f = Fixture::new(
            vec![in_project(task(1, "P work", 3.0), 10), q],
            vec![edge(
                EdgeType::DependsOn,
                NodeType::Project,
                11,
                NodeType::Project,
                10,
            )],
        );
        f.projects = vec![project(10, "P", None), project(11, "Q", None)];
        let slips = [slip_task(1, 5)];
        let plan = apply_plan(&f.world(), &slips).unwrap();
        // The dependency isn't part of the plain schedule, so the held-back project's task is
        // pinned too, or the knock-on would be lost.
        assert_eq!(plan.changes.len(), 2);
        let sc = scenario(&f.world(), &slips).unwrap();
        let after = finishes(&f, &applied(&f.tasks, &plan));
        for t in &sc.scen.tasks {
            assert!((after[&t.id] - t.ef).abs() < 1e-9, "{}", t.title);
        }
    }

    // --------------------------------------------------------- properties

    #[derive(Debug, Clone)]
    struct Spec {
        durations: Vec<u8>,
        project_of: Vec<bool>,
        edges: Vec<(usize, usize)>,
        slips: Vec<(usize, u32)>,
        project_slip: Option<u32>,
    }

    fn arb_spec() -> impl Strategy<Value = Spec> {
        (2usize..9)
            .prop_flat_map(|n| {
                (
                    proptest::collection::vec(0u8..6, n),
                    proptest::collection::vec(any::<bool>(), n),
                    proptest::collection::vec((0..n, 0..n), 0..14),
                    proptest::collection::vec((0..n, 1u32..8), 1..3),
                    proptest::option::of(1u32..6),
                )
            })
            .prop_map(|(durations, project_of, raw, slips, project_slip)| Spec {
                durations,
                project_of,
                edges: raw.into_iter().filter(|(a, b)| a < b).collect(),
                slips,
                project_slip,
            })
    }

    fn build(spec: &Spec) -> (Fixture, Vec<Slip>) {
        let tasks: Vec<Task> = spec
            .durations
            .iter()
            .enumerate()
            .map(|(i, d)| {
                in_project(
                    task(i as u128 + 1, &format!("T{i}"), f64::from(*d)),
                    if spec.project_of[i] { 10 } else { 11 },
                )
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        let edges = spec
            .edges
            .iter()
            .filter(|e| seen.insert(**e))
            .map(|(a, b)| blocks(*a as u128 + 1, *b as u128 + 1))
            .collect();
        let mut f = Fixture::new(tasks, edges);
        f.projects = vec![project(10, "P", None), project(11, "Q", None)];
        let mut slips: Vec<Slip> = spec
            .slips
            .iter()
            .map(|(i, d)| slip_task(*i as u128 + 1, *d))
            .collect();
        if let Some(d) = spec.project_slip {
            slips.push(slip_project(10, d));
        }
        (f, slips)
    }

    proptest! {
        /// Nothing moves earlier, what arrives is absorbed or passed on, a slip is never
        /// amplified, and tasks the slip can't reach are not reported.
        #[test]
        fn impact_never_moves_work_earlier_and_conserves_the_slip(spec in arb_spec()) {
            let (f, slips) = build(&spec);
            let sc = scenario(&f.world(), &slips).unwrap();
            let base = by_id(&sc.base);
            let scen = by_id(&sc.scen);
            for (id, b) in &base {
                let s = scen[id];
                prop_assert!(s.es >= b.es - 1e-9, "{} start moved earlier", b.title);
                prop_assert!(s.ef >= b.ef - 1e-9, "{} finish moved earlier", b.title);
            }
            let total: f64 = slips.iter().map(|s| f64::from(s.days)).sum();
            let r = analyze(&f.world(), &slips).unwrap();
            let listed: std::collections::HashSet<Uuid> = r.tasks.iter().map(|t| t.id).collect();
            for t in &r.tasks {
                prop_assert!(t.absorbed_days >= -1e-9 && t.delay_days >= -1e-9);
                prop_assert!((t.incoming_days - t.absorbed_days - t.delay_days).abs() < 1e-6,
                    "{}: {} != {} + {}", t.title, t.incoming_days, t.absorbed_days, t.delay_days);
                prop_assert!(t.delay_days <= total + 1e-9, "{} moved {} for a total slip of {}", t.title, t.delay_days, total);
                prop_assert!(t.new_finish >= t.old_finish && t.new_start >= t.old_start);
            }
            for (id, b) in &base {
                if !listed.contains(id) {
                    prop_assert!((scen[id].ef - b.ef).abs() < 1e-9, "{} moved but isn't reported", b.title);
                }
            }
            prop_assert!(r.summary.worst_delay_days <= total + 1e-9);
        }

        /// A bigger slip never moves anything less.
        #[test]
        fn more_slip_never_means_less_delay(spec in arb_spec(), extra in 1u32..5) {
            let (f, slips) = build(&spec);
            let more: Vec<Slip> = slips.iter().map(|s| Slip { days: s.days + extra, ..*s }).collect();
            let a = scenario(&f.world(), &slips).unwrap();
            let b = scenario(&f.world(), &more).unwrap();
            let (sa, sb) = (by_id(&a.scen), by_id(&b.scen));
            for (id, t) in &sa {
                prop_assert!(sb[id].ef >= t.ef - 1e-9);
            }
        }

        /// Writing the apply plan into the tasks and re-running the plain schedule gives the
        /// scenario's finishes: recording the slip really does record it.
        #[test]
        fn applying_the_plan_reproduces_the_scenario(spec in arb_spec()) {
            let (f, slips) = build(&spec);
            let sc = scenario(&f.world(), &slips).unwrap();
            let plan = apply_plan(&f.world(), &slips).unwrap();
            let after = finishes(&f, &applied(&f.tasks, &plan));
            for t in &sc.scen.tasks {
                prop_assert!((after[&t.id] - t.ef).abs() < 1e-9, "{}: {} vs {}", t.title, after[&t.id], t.ef);
            }
        }
    }

    // ------------------------------------------------------------------ routes (spec 46)

    fn route_of<'a>(r: &'a ImpactReport, project: &str) -> &'a ProjectRoute {
        r.routes
            .iter()
            .find(|x| x.title == project)
            .unwrap_or_else(|| panic!("no route for {project}"))
    }

    fn kinds(route: &ProjectRoute) -> Vec<(&str, TurnKind)> {
        route
            .turns
            .iter()
            .map(|t| (t.task.label.as_str(), t.kind))
            .collect()
    }

    /// Tasks 1..=n in project 10, each blocking the next, `days` each.
    fn line(n: u128, days: f64) -> Fixture {
        let names = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"];
        let tasks = (1..=n)
            .map(|k| in_project(task(k, names[k as usize - 1], days), 10))
            .collect();
        let edges = (1..n).map(|k| blocks(k, k + 1)).collect();
        let mut f = Fixture::new(tasks, edges);
        f.projects = vec![project(10, "P", None)];
        f
    }

    #[test]
    fn a_slip_on_a_single_chain_passes_through_and_arrives_later() {
        let r = line(3, 2.0).report(&[slip_task(1, 2)]);
        let route = route_of(&r, "P");
        assert_eq!(route.before, route.after);
        assert_eq!(route.before, [id(1), id(2), id(3)]);
        assert!(!route.changed);
        assert_eq!(
            kinds(route),
            [
                ("A", TurnKind::Slips),
                ("B", TurnKind::PassesOn),
                ("C", TurnKind::Arrives)
            ]
        );
        let arrives = route.turns.last().unwrap();
        assert_eq!(arrives.delay_days, 2.0);
        assert_eq!(Some(arrives.new_finish), route.new_finish);
        assert_ne!(route.old_finish, route.new_finish);
        assert_eq!(route.delay_days, 2.0);
        // The first turn is the slip itself.
        assert_eq!(route.turns[0].incoming_days, 2.0);
    }

    #[test]
    fn runs_of_steps_that_only_pass_the_same_delay_on_fold_into_one() {
        let r = line(5, 1.0).report(&[slip_task(1, 3)]);
        let route = route_of(&r, "P");
        assert_eq!(
            kinds(route),
            [
                ("A", TurnKind::Slips),
                ("B", TurnKind::PassesOn),
                ("E", TurnKind::Arrives)
            ]
        );
        // B stands for B, C and D.
        assert_eq!(route.turns[1].also, 2);
    }

    #[test]
    fn slack_that_soaks_up_part_of_a_slip_is_said_and_a_branch_can_take_over() {
        // A(3) -> B(2) and A -> C(4), both -> D(1): the route is A, C, D and B has 2 days of slack.
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "A", 3.0), 10),
                in_project(task(2, "B", 2.0), 10),
                in_project(task(3, "C", 4.0), 10),
                in_project(task(4, "D", 1.0), 10),
            ],
            vec![blocks(1, 2), blocks(1, 3), blocks(2, 4), blocks(3, 4)],
        );
        f.projects = vec![project(10, "P", None)];
        // Slipping B by 3 puts B on the route and takes A and C off it.
        let r = f.report(&[slip_task(2, 3)]);
        let route = route_of(&r, "P");
        assert!(route.changed);
        assert_eq!(route.before, [id(1), id(3), id(4)]);
        assert_eq!(route.after, [id(2), id(4)]);
        let k = kinds(route);
        assert!(k.contains(&("A", TurnKind::LeavesRoute)), "{k:?}");
        assert!(k.contains(&("C", TurnKind::LeavesRoute)), "{k:?}");
        assert!(k.contains(&("B", TurnKind::Slips)), "{k:?}");
        assert_eq!(k.last(), Some(&("D", TurnKind::Arrives)));
        // D received 3 days and soaked up 2 of them (its own arrival is 1 day later).
        let d = route.turns.last().unwrap();
        assert_eq!(d.delay_days, 1.0);
        // Slipping B by 1 is absorbed by D: the route is unchanged and the finish too.
        let r = f.report(&[slip_task(2, 1)]);
        let route = route_of(&r, "P");
        assert!(!route.changed);
        assert_eq!(route.delay_days, 0.0);
        assert_eq!(route.old_finish, route.new_finish);
        let k = kinds(route);
        assert_eq!(k[0], ("B", TurnKind::Slips));
        assert_eq!(k.last(), Some(&("D", TurnKind::Arrives)));
        assert_eq!(route.turns.last().unwrap().delay_days, 0.0);
    }

    #[test]
    fn a_task_that_absorbs_some_and_passes_on_the_rest_says_how_much_each() {
        // A(3) -> B(2) -> D(1), A -> C(4) -> D, and D -> E(1): D is in the middle of the route.
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "A", 3.0), 10),
                in_project(task(2, "B", 2.0), 10),
                in_project(task(3, "C", 4.0), 10),
                in_project(task(4, "D", 1.0), 10),
                in_project(task(5, "E", 1.0), 10),
            ],
            vec![
                blocks(1, 2),
                blocks(1, 3),
                blocks(2, 4),
                blocks(3, 4),
                blocks(4, 5),
            ],
        );
        f.projects = vec![project(10, "P", None)];
        let r = f.report(&[slip_task(2, 3)]);
        let d = route_of(&r, "P")
            .turns
            .iter()
            .find(|t| t.task.label == "D")
            .unwrap();
        // D is not the last step, so it keeps its own kind: 2 of the 3 days absorbed, 1 passed on.
        assert_eq!(d.kind, TurnKind::PartlyAbsorbs);
        assert_eq!(
            (d.incoming_days, d.absorbed_days, d.delay_days),
            (3.0, 2.0, 1.0)
        );
    }

    #[test]
    fn parallel_critical_branches_keep_only_the_chain_that_ends_the_project() {
        // Two equally long branches into the same last task.
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "A", 2.0), 10),
                in_project(task(2, "B", 2.0), 10),
                in_project(task(3, "C", 1.0), 10),
            ],
            vec![blocks(1, 3), blocks(2, 3)],
        );
        f.projects = vec![project(10, "P", None)];
        let r = f.report(&[slip_task(3, 1)]);
        let route = route_of(&r, "P");
        assert_eq!(route.before.len(), 2);
        assert_eq!(*route.before.last().unwrap(), id(3));
        // Everything critical is still drawn.
        assert!(route.bars.iter().filter(|b| b.on_old_route).count() >= 2);
    }

    #[test]
    fn a_long_route_folds_its_middle_into_a_more_step() {
        // Slipping the project slips every task, so every task is a start.
        let r = line(12, 1.0).report(&[slip_project(10, 2)]);
        let route = route_of(&r, "P");
        assert_eq!(route.turns.len(), MAX_TURNS);
        assert_eq!(route.turns[MAX_TURNS - 4].kind, TurnKind::More);
        // 12 slips and the arrival are 13 steps; the first 4 and the last 3 are kept: 6 left out.
        assert_eq!(route.turns[MAX_TURNS - 4].also, 6);
        assert_eq!(route.turns.last().unwrap().kind, TurnKind::Arrives);
        assert_eq!(route.turns[0].kind, TurnKind::Slips);
    }

    #[test]
    fn a_slip_in_one_project_gives_the_other_project_a_route_too() {
        // P's A blocks Q's B.
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "A", 2.0), 10),
                in_project(task(2, "B", 2.0), 11),
            ],
            vec![blocks(1, 2)],
        );
        f.projects = vec![project(10, "P", None), project(11, "Q", None)];
        let r = f.report(&[slip_task(1, 3)]);
        assert_eq!(r.routes.len(), r.projects.len());
        let q = route_of(&r, "Q");
        assert_eq!(q.delay_days, 3.0);
        assert_eq!(q.turns.last().unwrap().kind, TurnKind::Arrives);
        assert_eq!(route_of(&r, "P").turns[0].kind, TurnKind::Slips);
        // The routes follow the order of the projects.
        let order: Vec<&str> = r.projects.iter().map(|p| p.title.as_str()).collect();
        let routed: Vec<&str> = r.routes.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(order, routed);
    }

    #[test]
    fn a_dependent_project_starts_its_route_with_the_work_that_was_held_up() {
        let mut q = in_project(task(2, "Q work", 1.0), 11);
        q.start_date = Some(date!(2027 - 03 - 08));
        let mut f = Fixture::new(
            vec![in_project(task(1, "P work", 3.0), 10), q],
            vec![edge(
                EdgeType::DependsOn,
                NodeType::Project,
                11,
                NodeType::Project,
                10,
            )],
        );
        f.projects = vec![project(10, "P", None), project(11, "Q", None)];
        let r = f.report(&[slip_task(1, 5)]);
        let q = route_of(&r, "Q");
        assert_eq!(q.turns[0].task.label, "Q work");
        // A single task is both where it starts and where it ends: it slips, then the project arrives.
        assert_eq!(q.turns.len(), 2);
        assert_eq!(q.turns[0].kind, TurnKind::Slips);
        assert_eq!(q.turns.last().unwrap().kind, TurnKind::Arrives);
        assert_eq!(q.delay_days, 3.0);
    }

    #[test]
    fn a_project_that_only_absorbs_the_slip_still_has_a_route_that_says_no_change() {
        // A(1) -> C(1) and B(5) -> C: slipping A by 2 is absorbed by C (B drives it).
        let mut f = Fixture::new(
            vec![
                in_project(task(1, "A", 1.0), 10),
                in_project(task(2, "B", 5.0), 10),
                in_project(task(3, "C", 1.0), 10),
            ],
            vec![blocks(1, 3), blocks(2, 3)],
        );
        f.projects = vec![project(10, "P", None)];
        let r = f.report(&[slip_task(1, 2)]);
        // The report lists no moved project, but the route says the route held.
        assert!(r.projects.is_empty());
        let route = route_of(&r, "P");
        assert!(!route.changed);
        assert_eq!(route.delay_days, 0.0);
        assert_eq!(route.old_finish, route.new_finish);
    }

    #[test]
    fn the_timeline_data_covers_today_every_bar_and_the_target() {
        let mut f = line(3, 2.0);
        f.projects = vec![project(10, "P", Some(date!(2027 - 03 - 12)))];
        let r = f.report(&[slip_task(1, 2)]);
        let route = route_of(&r, "P");
        assert_eq!(route.first_offset, 0);
        let last_end = route
            .bars
            .iter()
            .map(|b| b.new_ef.max(b.old_ef))
            .fold(0.0, f64::max);
        assert!(route.days.len() as f64 >= last_end);
        assert_eq!(route.target_offset, Some(10.0));
        assert!(route.days.len() >= 10);
        // Each bar moved later, never earlier.
        for b in &route.bars {
            assert!(b.new_es >= b.old_es - 1e-9 && b.new_ef >= b.old_ef - 1e-9);
        }
    }

    proptest! {
        // However the work is linked and slipped, each route's chain is made of real links, every
        // step's delay is never negative, and the project's last step ends where the project does.
        #[test]
        fn routes_are_chains_of_real_links_and_arrive_where_the_project_ends(
            n in 2usize..8,
            links in proptest::collection::vec((0usize..7, 0usize..7), 0..12),
            days in proptest::collection::vec(1.0f64..4.0, 8),
            slip_at in 0usize..7,
            slip_days in 1u32..6,
        ) {
            let tasks: Vec<Task> = (0..n)
                .map(|i| in_project(task(i as u128 + 1, &format!("t{i}"), days[i]), 10))
                .collect();
            let mut seen = std::collections::HashSet::new();
            let edges: Vec<Edge> = links
                .iter()
                .filter(|(a, b)| a < b && *b < n && seen.insert((*a, *b)))
                .map(|(a, b)| blocks(*a as u128 + 1, *b as u128 + 1))
                .collect();
            let mut f = Fixture::new(tasks, edges.clone());
            f.projects = vec![project(10, "P", None)];
            let r = f.report(&[slip_task((slip_at % n) as u128 + 1, slip_days)]);
            let real: std::collections::HashSet<(Uuid, Uuid)> =
                edges.iter().map(|e| (e.from_id, e.to_id)).collect();
            for route in &r.routes {
                for chain in [&route.before, &route.after] {
                    for pair in chain.windows(2) {
                        prop_assert!(real.contains(&(pair[0], pair[1])), "{chain:?}");
                    }
                }
                prop_assert!(route.turns.len() <= MAX_TURNS);
                for t in &route.turns {
                    prop_assert!(t.delay_days >= 0.0);
                }
                if let Some(last) = route.turns.last() {
                    prop_assert_eq!(last.kind, TurnKind::Arrives);
                    if let Some(finish) = route.new_finish {
                        prop_assert_eq!(last.new_finish, finish);
                    }
                }
                prop_assert_eq!(route.changed, route.before != route.after);
            }
        }
    }
}
