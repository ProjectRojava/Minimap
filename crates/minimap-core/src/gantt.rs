//! The Gantt screen (spec 57). Pure.
//!
//! * [`build`] arranges the portfolio schedule as the tree the screen draws: objectives, their
//!   projects, their tasks (sub-tasks right after their parent), the `blocks` / `depends_on`
//!   links between what is drawn, and one shared axis.
//! * [`plan_drag`] turns a drag of a task's bar into dates. A drag writes dates *and* the
//!   estimate (the schedule is driven by the estimate, so a bar can only end where its length
//!   says): the middle moves the start and the due date together, the left edge sets the start,
//!   the right edge sets the due date. The tasks that follow are pushed later by exactly as much as
//!   they moved, in their **due dates only**; nothing is ever pulled earlier and start dates are
//!   never touched (pinning them would freeze work the schedule should keep moving).

use std::collections::{HashMap, HashSet};

use minimap_types::{
    Band, DragEdge, DragPreview, DragRefusal, DuePush, EarliestReason, Edge, EdgeType, GanttData,
    GanttGroup, GanttLink, GanttLinkKind, GanttObjective, GanttProject, GanttRef, GanttTask,
    MovedBar, MyScope, Objective, Person, Project, ProjectEffect, ProjectStatus, Schedule,
    ScheduleScope, ScheduledTask, Task, TaskDrag, TaskStatus, Uuid, WorkWeek, Zone,
};
use time::Date;

use crate::{
    impact::World,
    schedule::{chain_of, compute, date_of, end_index, working_index, ScheduleError},
    zone,
};

const EPS: f64 = 1e-9;
/// Working days of empty calendar after the last bar, so there is room to drag into.
const PADDING: i64 = 25;
/// Most working days the axis covers.
const MAX_DAYS: i64 = 1500;

/// Everything [`build`] reads. All of it is the active (non-archived) data.
pub struct BuildInput<'a> {
    pub tasks: &'a [Task],
    pub edges: &'a [Edge],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    /// For the names of the people tasks are assigned to (spec 58).
    pub people: &'a [Person],
    pub today: Date,
    pub work_week: WorkWeek,
    pub zone_narrow_days: u32,
    /// My view: only what is in it.
    pub scope: Option<&'a MyScope>,
}

// ------------------------------------------------------------------------ the tree

/// Whether a project needs a look (spec 58): it is not finished, and it is late, has no room left
/// before its target, or has blocked work. The Gantt opens such projects on its own and the
/// *Needs a look* filter keeps them.
pub fn needs_a_look(
    status: ProjectStatus,
    late_by_days: Option<u32>,
    zone: &Zone,
    blocked: u32,
) -> bool {
    let finished = matches!(status, ProjectStatus::Done | ProjectStatus::Cancelled);
    let no_room = matches!(zone, Zone::Inside { room: 0, .. } | Zone::Outside { .. });
    !finished && (late_by_days.is_some() || no_room || blocked > 0)
}

pub fn build(input: &BuildInput) -> Result<GanttData, ScheduleError> {
    let week = input.work_week;
    let t0 = working_index(week, input.today);
    let offset = |d: Date| (working_index(week, d) - t0) as f64;
    let end_offset = |d: Date| (end_index(week, d) - t0) as f64;
    let schedule = compute(
        input.tasks,
        input.edges,
        input.projects,
        input.today,
        week,
        ScheduleScope::Portfolio,
    )?;
    let task_by_id: HashMap<Uuid, &Task> = input.tasks.iter().map(|t| (t.id, t)).collect();
    let forecast_by_project: HashMap<Uuid, &minimap_types::ProjectForecast> = schedule
        .projects
        .iter()
        .map(|f| (f.project_id, f))
        .collect();

    // Objectives in the order they are drawn: dated ones soonest first, then those without a date,
    // then the ongoing ones.
    let mut objectives: Vec<&Objective> = input
        .objectives
        .iter()
        .filter(|o| o.archived_at.is_none())
        .filter(|o| input.scope.is_none_or(|s| s.objectives.contains_key(&o.id)))
        .collect();
    objectives.sort_by(|a, b| {
        (
            a.ongoing,
            a.target_date.is_none(),
            a.target_date,
            &a.title,
            a.id,
        )
            .cmp(&(
                b.ongoing,
                b.target_date.is_none(),
                b.target_date,
                &b.title,
                b.id,
            ))
    });
    let rank: HashMap<Uuid, usize> = objectives
        .iter()
        .enumerate()
        .map(|(i, o)| (o.id, i))
        .collect();

    // The objectives each project serves, in drawing order.
    let mut serves: HashMap<Uuid, Vec<usize>> = HashMap::new();
    for e in input.edges {
        if e.edge_type == EdgeType::ContributesTo && e.archived_at.is_none() {
            if let Some(&r) = rank.get(&e.to_id) {
                if input.projects.iter().any(|p| p.id == e.from_id) {
                    serves.entry(e.from_id).or_default().push(r);
                }
            }
        }
    }
    for list in serves.values_mut() {
        list.sort_unstable();
        list.dedup();
    }

    // Who each task is assigned to.
    let mut assignees: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in input.edges {
        if e.edge_type == EdgeType::AssignedTo && e.archived_at.is_none() {
            assignees.entry(e.from_id).or_default().push(e.to_id);
        }
    }
    for list in assignees.values_mut() {
        list.sort_unstable();
        list.dedup();
    }

    // Sub-task → parent (organisation only).
    let parent_of: HashMap<Uuid, Uuid> = input
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::SubtaskOf && e.archived_at.is_none())
        .map(|e| (e.from_id, e.to_id))
        .collect();

    let mut projects: Vec<GanttProject> = Vec::new();
    for p in input
        .projects
        .iter()
        .filter(|p| p.archived_at.is_none())
        .filter(|p| input.scope.is_none_or(|s| s.projects.contains_key(&p.id)))
    {
        let mut mine: Vec<&ScheduledTask> = schedule
            .tasks
            .iter()
            .filter(|t| t.project_id == Some(p.id))
            .filter(|t| input.scope.is_none_or(|s| s.tasks.contains_key(&t.id)))
            .collect();
        mine.sort_by(|a, b| {
            a.es.total_cmp(&b.es)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        });
        let present: HashSet<Uuid> = mine.iter().map(|t| t.id).collect();
        let parent_here = |id: Uuid| parent_of.get(&id).copied().filter(|p| present.contains(p));
        let to_row = |s: &ScheduledTask| -> Option<GanttTask> {
            let t = task_by_id.get(&s.id)?;
            Some(GanttTask {
                schedule: s.clone(),
                start_date: t.start_date,
                due_date: t.due_date,
                due_offset: t.due_date.map(end_offset),
                estimate_days: t.estimate_days,
                priority: t.priority,
                parent: parent_here(s.id),
                overdue: !s.done && t.due_date.is_some_and(|d| d < input.today),
                assignees: assignees.get(&s.id).cloned().unwrap_or_default(),
            })
        };
        let mut rows: Vec<GanttTask> = Vec::new();
        for s in mine.iter().filter(|s| parent_here(s.id).is_none()) {
            rows.extend(to_row(s));
            for child in mine.iter().filter(|c| parent_here(c.id) == Some(s.id)) {
                rows.extend(to_row(child));
            }
        }

        let open: Vec<&GanttTask> = rows.iter().filter(|t| !t.schedule.done).collect();
        let span_of: Vec<&GanttTask> = if open.is_empty() {
            rows.iter().collect()
        } else {
            open.clone()
        };
        let forecast = forecast_by_project.get(&p.id).copied();
        let band = Band {
            work_from: span_of.iter().map(|t| t.schedule.es).reduce(f64::min),
            work_to: span_of.iter().map(|t| t.schedule.ef).reduce(f64::max),
            start_date: p.start_date,
            start_offset: p.start_date.map(offset),
            target_date: p.target_date,
            target_offset: p.target_date.map(end_offset),
            late_by_days: forecast.and_then(|f| f.late_by_days),
        };
        let zone = forecast.map_or(Zone::None, |f| {
            zone::of(f, p.status, input.today, week, input.zone_narrow_days)
        });
        let also_under = serves
            .get(&p.id)
            .map(|list| {
                list.iter()
                    .skip(1)
                    .map(|&r| GanttRef {
                        id: objectives[r].id,
                        title: objectives[r].title.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let blocked_tasks = open
            .iter()
            .filter(|t| t.schedule.status == TaskStatus::Blocked)
            .count() as u32;
        let needs_a_look = needs_a_look(p.status, band.late_by_days, &zone, blocked_tasks);
        projects.push(GanttProject {
            id: p.id,
            title: p.title.clone(),
            slug: p.slug.clone(),
            status: p.status,
            band,
            zone,
            open_tasks: open.len() as u32,
            blocked_tasks,
            done_tasks: (rows.len() - open.len()) as u32,
            also_under,
            chain: chain_of(&schedule, p.id),
            tasks: rows,
            needs_a_look,
        });
    }
    // Projects in a group: the soonest to start first.
    projects.sort_by(|a, b| {
        let key = |p: &GanttProject| p.band.work_from.or(p.band.start_offset);
        key(a)
            .unwrap_or(f64::INFINITY)
            .total_cmp(&key(b).unwrap_or(f64::INFINITY))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut buckets: Vec<Vec<GanttProject>> = vec![Vec::new(); objectives.len() + 1];
    for p in projects {
        let at = serves
            .get(&p.id)
            .and_then(|l| l.first().copied())
            .unwrap_or(objectives.len());
        buckets[at].push(p);
    }
    let mut groups: Vec<GanttGroup> = Vec::new();
    for (i, projects) in buckets.into_iter().enumerate() {
        let objective = objectives.get(i).copied();
        if objective.is_none() && projects.is_empty() {
            continue;
        }
        let work_from = projects
            .iter()
            .filter_map(|p| p.band.work_from)
            .reduce(f64::min);
        let work_to = projects
            .iter()
            .filter_map(|p| p.band.work_to)
            .reduce(f64::max);
        let target = objective.filter(|o| !o.ongoing).and_then(|o| o.target_date);
        let target_offset = target.map(end_offset);
        let late_by_days = match (work_to, target_offset) {
            (Some(w), Some(t)) if w > t + EPS => Some((w - t - EPS).ceil().max(1.0) as u32),
            _ => None,
        };
        groups.push(GanttGroup {
            objective: objective.map(|o| GanttObjective {
                id: o.id,
                title: o.title.clone(),
                status: o.status,
                ongoing: o.ongoing,
            }),
            band: Band {
                work_from,
                work_to,
                start_date: None,
                start_offset: None,
                target_date: target,
                target_offset,
                late_by_days,
            },
            projects,
        });
    }

    // Links between what is drawn.
    let drawn_tasks: HashSet<Uuid> = groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .flat_map(|p| p.tasks.iter().map(|t| t.schedule.id))
        .collect();
    let drawn_projects: HashSet<Uuid> = groups
        .iter()
        .flat_map(|g| g.projects.iter().map(|p| p.id))
        .collect();
    let mut links: Vec<GanttLink> = input
        .edges
        .iter()
        .filter(|e| e.archived_at.is_none())
        .filter_map(|e| {
            let lag = e
                .attrs
                .get("lag_days")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            match e.edge_type {
                EdgeType::Blocks
                    if drawn_tasks.contains(&e.from_id) && drawn_tasks.contains(&e.to_id) =>
                {
                    Some(GanttLink {
                        kind: GanttLinkKind::Blocks,
                        from: e.from_id,
                        to: e.to_id,
                        lag_days: lag,
                    })
                }
                EdgeType::DependsOn
                    if drawn_projects.contains(&e.from_id) && drawn_projects.contains(&e.to_id) =>
                {
                    // `depends_on` points at what is needed; the arrow goes from it to the project.
                    Some(GanttLink {
                        kind: GanttLinkKind::DependsOn,
                        from: e.to_id,
                        to: e.from_id,
                        lag_days: 0,
                    })
                }
                _ => None,
            }
        })
        .collect();
    links.sort_by_key(|l| (l.from, l.to));

    // One axis for every row: bars, pinned starts, targets and today, with room to drag into.
    let mut lo: i64 = 0;
    let mut hi: i64 = 1;
    let mut take = |x: f64| {
        lo = lo.min(x.floor() as i64);
        hi = hi.max(x.ceil() as i64);
    };
    for g in &groups {
        for b in std::iter::once(&g.band).chain(g.projects.iter().map(|p| &p.band)) {
            for x in [b.work_from, b.work_to, b.start_offset, b.target_offset]
                .into_iter()
                .flatten()
            {
                take(x);
            }
        }
        for t in g.projects.iter().flat_map(|p| p.tasks.iter()) {
            take(t.schedule.es);
            take(t.schedule.ef + t.schedule.slack_days.max(0.0));
            if let Some(d) = t.due_offset {
                take(d);
            }
        }
    }
    lo -= 1;
    hi = (hi + PADDING).min(lo + MAX_DAYS);
    let days = (lo..hi).map(|i| date_of(week, t0 + i)).collect();

    // Everyone assigned to something drawn, by name.
    let drawn_people: HashSet<Uuid> = groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .flat_map(|p| p.tasks.iter())
        .flat_map(|t| t.assignees.iter().copied())
        .collect();
    let mut people: Vec<GanttRef> = input
        .people
        .iter()
        .filter(|p| drawn_people.contains(&p.id))
        .map(|p| GanttRef {
            id: p.id,
            title: p.name.clone(),
        })
        .collect();
    people.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));

    Ok(GanttData {
        today: input.today,
        first_offset: lo,
        days,
        groups,
        links,
        people,
    })
}

// ------------------------------------------------------------------- dragging a bar

fn index_of(t: &Task, id: Uuid) -> bool {
    t.id == id
}

/// What dropping `drag` would do, worked out on a copy of the plan; nothing is written. Either a
/// refusal, or the new values for the dragged task, every bar that moves and the due dates that
/// are pushed.
pub fn plan_drag(world: &World, drag: &TaskDrag) -> Result<DragPreview, ScheduleError> {
    let week = world.work_week;
    let run = |tasks: &[Task]| {
        compute(
            tasks,
            world.edges,
            world.projects,
            world.today,
            week,
            ScheduleScope::Portfolio,
        )
    };
    let before = run(world.tasks)?;
    let Some(t) = before.tasks.iter().find(|t| t.id == drag.task_id) else {
        return Ok(DragPreview::refused(DragRefusal::NotPlanned));
    };
    let Some(task) = world.tasks.iter().find(|t| index_of(t, drag.task_id)) else {
        return Ok(DragPreview::refused(DragRefusal::NotPlanned));
    };
    if t.done {
        return Ok(DragPreview::refused(DragRefusal::Finished));
    }

    // The bar occupies the working days current_start..=current_end.
    let current_start = working_index(week, t.start);
    let current_end = working_index(week, t.finish).max(current_start);
    let earliest = working_index(week, t.earliest_start);
    let to = working_index(week, drag.to);
    let too_early = || {
        let reason = match t.held_by {
            Some(id) => EarliestReason::BlockedBy {
                task_id: id,
                title: before
                    .tasks
                    .iter()
                    .find(|x| x.id == id)
                    .map_or_else(String::new, |x| x.title.clone()),
            },
            None => {
                let project_start = task
                    .project_id
                    .and_then(|p| world.projects.iter().find(|x| x.id == p))
                    .and_then(|p| p.start_date)
                    .map(|d| working_index(week, d));
                if project_start.is_some_and(|s| s > working_index(week, world.today)) {
                    EarliestReason::ProjectStart
                } else {
                    EarliestReason::Today
                }
            }
        };
        DragPreview::refused(DragRefusal::TooEarly {
            earliest: t.earliest_start,
            reason,
        })
    };

    let mut start_date = None;
    let mut due_date = None;
    let mut estimate = None;
    match drag.edge {
        DragEdge::Move => {
            if to < earliest {
                return Ok(too_early());
            }
            let delta = to - current_start;
            if delta == 0 {
                return Ok(DragPreview::default());
            }
            start_date = Some(date_of(week, to));
            due_date = task
                .due_date
                .map(|d| date_of(week, end_index(week, d) - 1 + delta));
        }
        DragEdge::Start => {
            if to < earliest {
                return Ok(too_early());
            }
            if to > current_end {
                return Ok(DragPreview::refused(DragRefusal::AfterEnd));
            }
            if to == current_start {
                return Ok(DragPreview::default());
            }
            start_date = Some(date_of(week, to));
            estimate = Some((current_end - to + 1) as f64);
        }
        DragEdge::End => {
            if to < current_start {
                return Ok(DragPreview::refused(DragRefusal::BeforeStart));
            }
            if to == current_end {
                return Ok(DragPreview::default());
            }
            due_date = Some(date_of(week, to));
            estimate = Some((to - current_start + 1) as f64);
        }
    }
    // Only what actually changes is reported.
    start_date = start_date.filter(|d| task.start_date != Some(*d));
    due_date = due_date.filter(|d| task.due_date != Some(*d));
    estimate = estimate.filter(|e| task.estimate_days != Some(*e));

    // Run the plan again with the new values.
    let mut tasks = world.tasks.to_vec();
    if let Some(copy) = tasks.iter_mut().find(|x| index_of(x, drag.task_id)) {
        if let Some(d) = start_date {
            copy.start_date = Some(d);
        }
        if let Some(d) = due_date {
            copy.due_date = Some(d);
        }
        if let Some(e) = estimate {
            copy.estimate_days = Some(e);
        }
    }
    let after = run(&tasks)?;
    let (moved, pushes) = compare(world, &before, &after);
    Ok(DragPreview {
        refusal: None,
        start_date,
        due_date,
        estimate_days: estimate,
        moved,
        pushes: pushes
            .into_iter()
            .filter(|p| p.task_id != drag.task_id)
            .collect(),
        projects: project_effects(&before, &after),
    })
}

/// Bars that landed elsewhere, and the due dates of the tasks that got later (never earlier).
fn compare(world: &World, before: &Schedule, after: &Schedule) -> (Vec<MovedBar>, Vec<DuePush>) {
    let week = world.work_week;
    let was: HashMap<Uuid, &ScheduledTask> = before.tasks.iter().map(|t| (t.id, t)).collect();
    let due: HashMap<Uuid, Option<Date>> = world.tasks.iter().map(|t| (t.id, t.due_date)).collect();
    let mut moved = Vec::new();
    let mut pushes = Vec::new();
    for now in &after.tasks {
        let Some(old) = was.get(&now.id) else {
            continue;
        };
        if (now.es - old.es).abs() <= EPS && (now.ef - old.ef).abs() <= EPS {
            continue;
        }
        moved.push(MovedBar {
            task_id: now.id,
            es: now.es,
            ef: now.ef,
            start: now.start,
            finish: now.finish,
        });
        let later = now.ef - old.ef;
        if now.done || later <= EPS {
            continue;
        }
        if let Some(Some(d)) = due.get(&now.id) {
            let days = (later - EPS).ceil().max(1.0) as u32;
            pushes.push(DuePush {
                task_id: now.id,
                title: now.title.clone(),
                project_title: now.project_title.clone(),
                old_due: *d,
                new_due: date_of(week, end_index(week, *d) - 1 + i64::from(days)),
                days,
            });
        }
    }
    (moved, pushes)
}

fn project_effects(before: &Schedule, after: &Schedule) -> Vec<ProjectEffect> {
    let was: HashMap<Uuid, &minimap_types::ProjectForecast> =
        before.projects.iter().map(|f| (f.project_id, f)).collect();
    let mut out: Vec<ProjectEffect> = after
        .projects
        .iter()
        .filter_map(|now| {
            let old = was.get(&now.project_id)?;
            (old.projected_finish != now.projected_finish || old.late_by_days != now.late_by_days)
                .then(|| ProjectEffect {
                    project_id: now.project_id,
                    title: now.title.clone(),
                    finish_before: old.projected_finish,
                    finish_after: now.projected_finish,
                    late_before: old.late_by_days,
                    late_after: now.late_by_days,
                })
        })
        .collect();
    out.sort_by(|a, b| a.title.cmp(&b.title).then(a.project_id.cmp(&b.project_id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeType, ObjectiveStatus, ProjectStatus};
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    // 2027-03-01 is a Monday.
    const MON: Date = date!(2027 - 03 - 01);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, days: f64, project: u128) -> Task {
        Task {
            links: Vec::new(),
            task_type: None,
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            confirmed_on: None,
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: Some(id(project)),
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

    fn project(n: u128, title: &str) -> Project {
        Project {
            confirmed_on: None,
            id: id(n),
            title: title.into(),
            slug: title.to_lowercase(),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: ProjectStatus::Active,
            priority: 3,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn objective(n: u128, title: &str, target: Option<Date>, ongoing: bool) -> Objective {
        Objective {
            id: id(n),
            title: title.into(),
            description: String::new(),
            target_date: target,
            status: ObjectiveStatus::OnTrack,
            priority: 3,
            ongoing,
            review_every_days: None,
            last_reviewed_on: None,
            confirmed_on: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn edge(kind: EdgeType, from: (NodeType, u128), to: (NodeType, u128)) -> Edge {
        Edge {
            id: Uuid::from_u128(10_000 + from.1 * 100 + to.1),
            edge_type: kind,
            from_type: from.0,
            from_id: id(from.1),
            to_type: to.0,
            to_id: id(to.1),
            attrs: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn blocks(a: u128, b: u128) -> Edge {
        edge(EdgeType::Blocks, (NodeType::Task, a), (NodeType::Task, b))
    }

    fn world<'a>(tasks: &'a [Task], edges: &'a [Edge], projects: &'a [Project]) -> World<'a> {
        World {
            tasks,
            edges,
            projects,
            objectives: &[],
            people: &[],
            today: MON,
            work_week: WorkWeek::MON_FRI,
        }
    }

    fn drag(task: u128, edge: DragEdge, to: Date) -> TaskDrag {
        TaskDrag {
            task_id: id(task),
            edge,
            to,
        }
    }

    // --------------------------------------------------------------- the tree

    fn input<'a>(
        tasks: &'a [Task],
        edges: &'a [Edge],
        projects: &'a [Project],
        objectives: &'a [Objective],
    ) -> BuildInput<'a> {
        BuildInput {
            tasks,
            edges,
            projects,
            objectives,
            people: &[],
            today: MON,
            work_week: WorkWeek::MON_FRI,
            zone_narrow_days: 3,
            scope: None,
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

    #[test]
    fn tasks_know_who_they_are_assigned_to_and_the_plan_lists_those_people() {
        let projects = [project(10, "API")];
        let tasks = [task(100, "Build", 2.0, 10), task(101, "Test", 1.0, 10)];
        let edges = [
            edge(
                EdgeType::AssignedTo,
                (NodeType::Task, 100),
                (NodeType::Person, 50),
            ),
            edge(
                EdgeType::AssignedTo,
                (NodeType::Task, 101),
                (NodeType::Person, 51),
            ),
            edge(
                EdgeType::AssignedTo,
                (NodeType::Task, 100),
                (NodeType::Person, 51),
            ),
        ];
        let people = [person(50, "Zoe"), person(51, "Amir"), person(52, "Nobody")];
        let mut i = input(&tasks, &edges, &projects, &[]);
        i.people = &people;
        let d = build(&i).unwrap();
        let p = &d.groups[0].projects[0];
        let of = |n: u128| {
            p.tasks
                .iter()
                .find(|t| t.schedule.id == id(n))
                .unwrap()
                .assignees
                .clone()
        };
        assert_eq!(of(100), [id(50), id(51)]);
        assert_eq!(of(101), [id(51)]);
        let names: Vec<_> = d.people.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(
            names,
            ["Amir", "Zoe"],
            "by name, only people on drawn tasks"
        );
    }

    #[test]
    fn a_project_needs_a_look_when_late_out_of_room_or_blocked() {
        let wide = Zone::Inside {
            remaining: 5,
            room: 4,
            level: minimap_types::ZoneLevel::Wide,
        };
        let none = Zone::Inside {
            remaining: 5,
            room: 0,
            level: minimap_types::ZoneLevel::Closing,
        };
        let over = Zone::Outside {
            remaining: 5,
            over: 2,
        };
        let active = ProjectStatus::Active;
        assert!(!needs_a_look(active, None, &wide, 0));
        assert!(needs_a_look(active, Some(2), &wide, 0), "late");
        assert!(needs_a_look(active, None, &none, 0), "no room");
        assert!(needs_a_look(active, None, &over, 0), "past the target");
        assert!(needs_a_look(active, None, &wide, 1), "blocked work");
        assert!(!needs_a_look(active, None, &Zone::None, 0));
        assert!(
            !needs_a_look(ProjectStatus::Done, Some(2), &over, 3),
            "finished is finished"
        );
        // Built: a blocked task makes its project need a look.
        let projects = [project(10, "API"), project(11, "Docs")];
        let mut stuck = task(100, "Build", 2.0, 10);
        stuck.status = TaskStatus::Blocked;
        let tasks = [stuck, task(101, "Write", 1.0, 11)];
        let d = build(&input(&tasks, &[], &projects, &[])).unwrap();
        let look: Vec<_> = d.groups[0]
            .projects
            .iter()
            .map(|p| (p.title.as_str(), p.needs_a_look))
            .collect();
        assert!(
            look.contains(&("API", true)) && look.contains(&("Docs", false)),
            "{look:?}"
        );
    }

    #[test]
    fn projects_sit_under_their_first_objective_and_say_where_else_they_serve() {
        let objectives = [
            objective(1, "Launch", Some(date!(2027 - 06 - 30)), false),
            objective(2, "Reliability", None, true),
            objective(3, "Grow", Some(date!(2027 - 04 - 30)), false),
        ];
        let projects = [
            project(10, "API"),
            project(11, "Docs"),
            project(12, "Loose"),
        ];
        let tasks = [task(100, "Build", 2.0, 10), task(101, "Write", 1.0, 11)];
        let edges = [
            edge(
                EdgeType::ContributesTo,
                (NodeType::Project, 10),
                (NodeType::Objective, 1),
            ),
            edge(
                EdgeType::ContributesTo,
                (NodeType::Project, 10),
                (NodeType::Objective, 3),
            ),
            edge(
                EdgeType::ContributesTo,
                (NodeType::Project, 11),
                (NodeType::Objective, 2),
            ),
        ];
        let data = build(&input(&tasks, &edges, &projects, &objectives)).unwrap();
        let names: Vec<_> = data
            .groups
            .iter()
            .map(|g| g.objective.as_ref().map(|o| o.title.as_str()))
            .collect();
        // Soonest target first, ongoing last, then the projects with no objective.
        assert_eq!(
            names,
            [Some("Grow"), Some("Launch"), Some("Reliability"), None]
        );
        // The soonest objective wins; the other one is named on the project.
        let api = &data.groups[0].projects[0];
        assert_eq!(api.title, "API");
        assert_eq!(api.also_under.len(), 1);
        assert_eq!(api.also_under[0].title, "Launch");
        // Drawn once.
        assert!(data.groups[1].projects.is_empty());
        assert_eq!(data.groups[3].projects[0].title, "Loose");
        assert!(data.groups[2].objective.as_ref().unwrap().ongoing);
        assert_eq!(data.groups[2].band.target_date, None);
    }

    #[test]
    fn sub_tasks_follow_their_parent_and_links_need_both_ends_drawn() {
        let projects = [project(10, "API"), project(11, "Docs")];
        let tasks = [
            task(1, "Parent", 3.0, 10),
            task(2, "Child", 1.0, 10),
            task(3, "Other", 1.0, 10),
            task(4, "Elsewhere", 1.0, 11),
        ];
        let edges = [
            edge(
                EdgeType::SubtaskOf,
                (NodeType::Task, 2),
                (NodeType::Task, 1),
            ),
            blocks(1, 3),
            blocks(1, 4),
            edge(
                EdgeType::DependsOn,
                (NodeType::Project, 11),
                (NodeType::Project, 10),
            ),
        ];
        let data = build(&input(&tasks, &edges, &projects, &[])).unwrap();
        let api = &data.groups[0]
            .projects
            .iter()
            .find(|p| p.id == id(10))
            .unwrap();
        let rows: Vec<_> = api
            .tasks
            .iter()
            .map(|t| t.schedule.title.as_str())
            .collect();
        assert_eq!(rows, ["Parent", "Child", "Other"]);
        assert_eq!(api.tasks[1].parent, Some(id(1)));
        assert_eq!(data.links.len(), 3);
        let dep = data
            .links
            .iter()
            .find(|l| l.kind == GanttLinkKind::DependsOn)
            .unwrap();
        // The arrow runs from what is needed to the project that needs it.
        assert_eq!((dep.from, dep.to), (id(10), id(11)));
    }

    #[test]
    fn the_axis_covers_the_bars_the_targets_and_some_room() {
        let mut p = project(10, "API");
        p.target_date = Some(date!(2027 - 05 - 28));
        p.start_date = Some(date!(2027 - 02 - 15));
        let tasks = [task(1, "A", 2.0, 10)];
        let data = build(&input(&tasks, &[], &[p], &[])).unwrap();
        let band = data.groups[0].projects[0].band;
        let first = data.first_offset as f64;
        let last = data.first_offset as f64 + data.days.len() as f64;
        assert!(first <= band.start_offset.unwrap() && first <= 0.0);
        assert!(last >= band.target_offset.unwrap() + PADDING as f64 - 1.0);
        assert!(data.days.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(band.work_from, Some(0.0));
        assert_eq!(band.work_to, Some(2.0));
    }

    #[test]
    fn my_view_keeps_only_what_is_in_it() {
        let projects = [project(10, "Mine"), project(11, "Theirs")];
        let tasks = [
            task(1, "Mine1", 1.0, 10),
            task(2, "Mine2", 1.0, 10),
            task(3, "T", 1.0, 11),
        ];
        let mut scope = MyScope::default();
        scope.projects.insert(id(10), Vec::new());
        scope.tasks.insert(id(1), Vec::new());
        let mut i = input(&tasks, &[], &projects, &[]);
        i.scope = Some(&scope);
        let data = build(&i).unwrap();
        let shown: Vec<_> = data
            .groups
            .iter()
            .flat_map(|g| g.projects.iter())
            .flat_map(|p| p.tasks.iter().map(|t| t.schedule.title.clone()))
            .collect();
        assert_eq!(shown, ["Mine1"]);
    }

    #[test]
    fn an_objective_runs_late_when_its_projects_do() {
        let objectives = [objective(1, "Launch", Some(date!(2027 - 03 - 02)), false)];
        let projects = [project(10, "API")];
        let tasks = [task(1, "Long", 5.0, 10)];
        let edges = [edge(
            EdgeType::ContributesTo,
            (NodeType::Project, 10),
            (NodeType::Objective, 1),
        )];
        let data = build(&input(&tasks, &edges, &projects, &objectives)).unwrap();
        assert_eq!(data.groups[0].band.late_by_days, Some(3));
    }

    // ------------------------------------------------------------------- drags

    /// A(2 days) blocks B(2 days) blocks C(1 day); B and C have due dates.
    fn chain() -> (Vec<Task>, Vec<Edge>, Vec<Project>) {
        let a = task(1, "A", 2.0, 10);
        let mut b = task(2, "B", 2.0, 10);
        b.due_date = Some(date!(2027 - 03 - 04)); // Thursday: its last day
        let mut c = task(3, "C", 1.0, 10);
        c.due_date = Some(date!(2027 - 03 - 05));
        (
            vec![a, b, c],
            vec![blocks(1, 2), blocks(2, 3)],
            vec![project(10, "P")],
        )
    }

    #[test]
    fn dragging_a_bar_later_pushes_what_follows_but_only_in_due_dates() {
        let (tasks, edges, projects) = chain();
        let w = world(&tasks, &edges, &projects);
        // A starts Monday (2 days); start it Wednesday: two working days later.
        let p = plan_drag(&w, &drag(1, DragEdge::Move, date!(2027 - 03 - 03))).unwrap();
        assert_eq!(p.refusal, None);
        assert_eq!(p.start_date, Some(date!(2027 - 03 - 03)));
        assert_eq!(p.estimate_days, None);
        // A has no due date, so none is invented.
        assert_eq!(p.due_date, None);
        let pushes: Vec<_> = p
            .pushes
            .iter()
            .map(|x| (x.title.as_str(), x.old_due, x.new_due, x.days))
            .collect();
        assert_eq!(
            pushes,
            [
                ("B", date!(2027 - 03 - 04), date!(2027 - 03 - 08), 2),
                ("C", date!(2027 - 03 - 05), date!(2027 - 03 - 09), 2),
            ]
        );
        assert_eq!(p.moved.len(), 3);
        assert_eq!(p.moved[0].start, date!(2027 - 03 - 03));
        // The project finish moves from Friday 5th to Tuesday 9th.
        assert_eq!(p.projects.len(), 1);
        assert_eq!(p.projects[0].finish_after, Some(date!(2027 - 03 - 09)));
    }

    #[test]
    fn a_pushed_task_moves_by_what_it_really_moved_and_slack_absorbs_the_rest() {
        let (mut tasks, edges, mut projects) = chain();
        // A target a week later than the finish gives the chain slack, but a forward move still
        // moves the bars: the schedule is as early as it can be.
        projects[0].target_date = Some(date!(2027 - 03 - 19));
        // C is pinned to start late, so a one day delay of A doesn't move it.
        tasks[2].start_date = Some(date!(2027 - 03 - 08));
        let w = world(&tasks, &edges, &projects);
        let p = plan_drag(&w, &drag(1, DragEdge::Move, date!(2027 - 03 - 02))).unwrap();
        let titles: Vec<_> = p.pushes.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, ["B"]);
        assert_eq!(p.pushes[0].days, 1);
    }

    #[test]
    fn a_bar_cannot_start_before_what_blocks_it_and_the_reason_names_it() {
        let (tasks, edges, projects) = chain();
        let w = world(&tasks, &edges, &projects);
        // B can start Wednesday at the earliest (after A's two days).
        let p = plan_drag(&w, &drag(2, DragEdge::Move, date!(2027 - 03 - 02))).unwrap();
        match p.refusal.clone() {
            Some(DragRefusal::TooEarly { earliest, reason }) => {
                assert_eq!(earliest, date!(2027 - 03 - 03));
                assert_eq!(
                    reason,
                    EarliestReason::BlockedBy {
                        task_id: id(1),
                        title: "A".into()
                    }
                );
            }
            other => panic!("{other:?}"),
        }
        let text = p.refusal.unwrap().text();
        assert!(
            text.contains("before A is done") && text.contains("Wed 3 Mar"),
            "{text}"
        );
        // And not before today.
        let w = world(&tasks, &edges, &projects);
        let p = plan_drag(&w, &drag(1, DragEdge::Move, date!(2027 - 02 - 26))).unwrap();
        assert!(matches!(
            p.refusal,
            Some(DragRefusal::TooEarly {
                reason: EarliestReason::Today,
                ..
            })
        ));
    }

    #[test]
    fn a_project_start_is_the_reason_when_it_sets_the_earliest_day() {
        let (tasks, edges, mut projects) = chain();
        projects[0].start_date = Some(date!(2027 - 03 - 03));
        let w = world(&tasks, &edges, &projects);
        let p = plan_drag(&w, &drag(1, DragEdge::Move, date!(2027 - 03 - 02))).unwrap();
        assert!(matches!(
            p.refusal,
            Some(DragRefusal::TooEarly {
                reason: EarliestReason::ProjectStart,
                ..
            })
        ));
    }

    #[test]
    fn the_right_edge_sets_the_due_date_and_the_estimate_and_leaves_the_start() {
        let (tasks, edges, projects) = chain();
        let w = world(&tasks, &edges, &projects);
        // B runs Wed-Thu; stretch it to Monday of the next week: 4 working days.
        let p = plan_drag(&w, &drag(2, DragEdge::End, date!(2027 - 03 - 08))).unwrap();
        assert_eq!(p.due_date, Some(date!(2027 - 03 - 08)));
        assert_eq!(p.estimate_days, Some(4.0));
        assert_eq!(p.start_date, None);
        // C follows by the two days B grew.
        assert_eq!(p.pushes.len(), 1);
        assert_eq!(p.pushes[0].title, "C");
        assert_eq!(p.pushes[0].days, 2);
        // Before the start is refused.
        let p = plan_drag(&w, &drag(2, DragEdge::End, date!(2027 - 03 - 02))).unwrap();
        assert_eq!(p.refusal, Some(DragRefusal::BeforeStart));
    }

    #[test]
    fn the_left_edge_sets_the_start_and_the_estimate_and_keeps_the_end() {
        let (tasks, edges, projects) = chain();
        let w = world(&tasks, &edges, &projects);
        // A runs Mon-Tue; pull its start to Tuesday: one day long, still ending Tuesday.
        let p = plan_drag(&w, &drag(1, DragEdge::Start, date!(2027 - 03 - 02))).unwrap();
        assert_eq!(p.start_date, Some(date!(2027 - 03 - 02)));
        assert_eq!(p.estimate_days, Some(1.0));
        assert!(p.pushes.is_empty(), "the end did not move");
        // Past the end is refused; the same day is nothing to do.
        let p = plan_drag(&w, &drag(1, DragEdge::Start, date!(2027 - 03 - 03))).unwrap();
        assert_eq!(p.refusal, Some(DragRefusal::AfterEnd));
        let p = plan_drag(&w, &drag(1, DragEdge::Start, MON)).unwrap();
        assert!(p.is_noop() && p.moved.is_empty());
    }

    #[test]
    fn moving_a_bar_moves_its_due_date_by_the_same_working_days() {
        let (tasks, edges, projects) = chain();
        let w = world(&tasks, &edges, &projects);
        // C (due Friday) moves from Fri to the next Wednesday: 3 working days.
        let p = plan_drag(&w, &drag(3, DragEdge::Move, date!(2027 - 03 - 10))).unwrap();
        assert_eq!(p.start_date, Some(date!(2027 - 03 - 10)));
        assert_eq!(p.due_date, Some(date!(2027 - 03 - 10)));
    }

    #[test]
    fn finished_and_unplanned_work_has_no_bar_to_move() {
        let (mut tasks, edges, projects) = chain();
        tasks[0].status = TaskStatus::Done;
        tasks[0].completed_at = Some(MON.midnight().assume_utc());
        let w = world(&tasks, &edges, &projects);
        let p = plan_drag(&w, &drag(1, DragEdge::Move, date!(2027 - 03 - 03))).unwrap();
        assert_eq!(p.refusal, Some(DragRefusal::Finished));
        let p = plan_drag(&w, &drag(99, DragEdge::Move, MON)).unwrap();
        assert_eq!(p.refusal, Some(DragRefusal::NotPlanned));
    }

    proptest! {
        /// Whatever is dragged, nothing follows earlier, a due date only ever moves later, and a
        /// move lands the bar on the day asked for.
        #[test]
        fn drags_never_pull_anything_earlier(
            edge in 0usize..3,
            which in 1u128..=3,
            offset in 0i64..30,
            durations in proptest::collection::vec(1u32..5, 3),
        ) {
            let mut tasks = vec![
                task(1, "A", f64::from(durations[0]), 10),
                task(2, "B", f64::from(durations[1]), 10),
                task(3, "C", f64::from(durations[2]), 10),
            ];
            for (i, t) in tasks.iter_mut().enumerate() {
                t.due_date = Some(date_of(WorkWeek::MON_FRI, i as i64 + 6));
            }
            let edges = [blocks(1, 2), blocks(2, 3)];
            let projects = [project(10, "P")];
            let w = world(&tasks, &edges, &projects);
            let edge = [DragEdge::Move, DragEdge::Start, DragEdge::End][edge];
            let to = date_of(WorkWeek::MON_FRI, offset);
            let p = plan_drag(&w, &drag(which, edge, to)).unwrap();
            if p.refusal.is_none() {
                for push in &p.pushes {
                    prop_assert!(push.new_due > push.old_due);
                    prop_assert!(push.days >= 1);
                }
                for m in &p.moved {
                    prop_assert!(m.finish >= m.start);
                }
                if edge == DragEdge::Move && !p.is_noop() {
                    let moved = p.moved.iter().find(|m| m.task_id == id(which)).unwrap();
                    prop_assert_eq!(moved.start, to);
                }
            }
        }
    }
}
