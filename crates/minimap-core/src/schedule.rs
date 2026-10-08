//! Critical-path scheduling over `blocks` links (CLAUDE.md section 5.3). Pure.
//!
//! Time is counted in **working days** (the Settings work week, Mon-Fri by default) from today's working day; see
//! `minimap_types::schedule`. The forward pass runs over every open task, so cross-project
//! `blocks` links delay the tasks they should whatever the scope. The backward pass runs over
//! the scope only, against each project's deadline: its target date, or (with none) its own
//! projected finish. Slack is `LS - ES` and is **negative** when a target can't be met.
//!
//! A task is *critical* when it has the least slack in its project: zero when there is no
//! target, and the chain that decides the finish when the target is generous or unrealistic.

use std::collections::{BTreeMap, HashMap};

use minimap_types::{
    Edge, EdgeType, Project, ProjectForecast, Schedule, ScheduleScope, ScheduledTask, Task,
    TaskStatus, Uuid, WorkWeek,
};
use petgraph::{algo::toposort, graph::DiGraph, visit::EdgeRef, Direction};
use time::Date;

const EPS: f64 = 1e-9;
/// Most working days an axis can span, so one far-future start date can't produce a huge reply.
const MAX_DAYS: i64 = 1500;
/// 2000-01-03, a Monday: working day 0 of the calendar arithmetic.
const EPOCH_JD: i64 = 2_451_547;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("the blocks links form a loop, so the work can't be scheduled")]
    Cycle,
}

// ------------------------------------------------------------------- calendar

/// Index of the first working day on or after `d` (a day off maps to the next working day).
pub(crate) fn working_index(week: WorkWeek, d: Date) -> i64 {
    let n = i64::from(d.to_julian_day()) - EPOCH_JD;
    n.div_euclid(7) * i64::from(week.days_per_week())
        + i64::from(week.before(n.rem_euclid(7) as u8))
}

/// Index just past the last working day on or before `d`: the end of that day.
pub(crate) fn end_index(week: WorkWeek, d: Date) -> i64 {
    let n = i64::from(d.to_julian_day()) - EPOCH_JD;
    working_index(week, d) + i64::from(week.contains(n.rem_euclid(7) as u8))
}

/// The date of working day `index`.
pub(crate) fn date_of(week: WorkWeek, index: i64) -> Date {
    let k = i64::from(week.days_per_week());
    let weekday = i64::from(week.nth(index.rem_euclid(k) as u32));
    let jd = EPOCH_JD + index.div_euclid(k) * 7 + weekday;
    i32::try_from(jd)
        .ok()
        .and_then(|jd| Date::from_julian_day(jd).ok())
        .unwrap_or(Date::MAX)
}

/// The date a task ending at offset `ef` last works on (at least its start day).
fn finish_index(es: f64, ef: f64) -> i64 {
    let start = (es + EPS).floor() as i64;
    ((ef - EPS).ceil() as i64 - 1).max(start)
}

// ------------------------------------------------------------------ the engine

struct Node<'a> {
    task: &'a Task,
    duration: f64,
    unestimated: bool,
    done: bool,
    es: f64,
    ef: f64,
    ls: f64,
    lf: f64,
    in_scope: bool,
}

fn lag_of(e: &Edge) -> f64 {
    e.attrs
        .get("lag_days")
        .and_then(|v| v.as_u64())
        .map_or(0.0, |n| n as f64)
}

/// Schedules the open work. `tasks` and `edges` are the active ones; cancelled tasks are left
/// out (and anything linked only through them no longer constrains anything).
pub fn compute(
    tasks: &[Task],
    edges: &[Edge],
    projects: &[Project],
    today: Date,
    week: WorkWeek,
    scope: ScheduleScope,
) -> Result<Schedule, ScheduleError> {
    compute_with(tasks, edges, projects, today, week, scope, &HashMap::new())
}

/// [`compute`] with extra "not before" constraints (working-day offsets) on open tasks. Impact
/// analysis uses them to model a slip: a task held until `baseline start + N`.
pub fn compute_with(
    tasks: &[Task],
    edges: &[Edge],
    projects: &[Project],
    today: Date,
    week: WorkWeek,
    scope: ScheduleScope,
    not_before: &HashMap<Uuid, f64>,
) -> Result<Schedule, ScheduleError> {
    let t0 = working_index(week, today);
    let offset = |d: Date| (working_index(week, d) - t0) as f64;
    let end_offset = |d: Date| (end_index(week, d) - t0) as f64;
    let day_at = |offset: i64| date_of(week, t0 + offset);

    let live: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Cancelled && t.archived_at.is_none())
        .collect();
    let index: HashMap<Uuid, usize> = live.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    let mut graph = DiGraph::<usize, f64>::new();
    let ids: Vec<_> = (0..live.len()).map(|i| graph.add_node(i)).collect();
    for e in edges {
        if e.edge_type != EdgeType::Blocks || e.archived_at.is_some() {
            continue;
        }
        if let (Some(&from), Some(&to)) = (index.get(&e.from_id), index.get(&e.to_id)) {
            graph.add_edge(ids[from], ids[to], lag_of(e));
        }
    }
    let order = toposort(&graph, None).map_err(|_| ScheduleError::Cycle)?;

    let mut nodes: Vec<Node> = live
        .iter()
        .map(|t| Node {
            task: t,
            duration: t.estimate_days.unwrap_or(1.0).max(0.0),
            unestimated: t.estimate_days.is_none(),
            done: t.status == TaskStatus::Done,
            es: 0.0,
            ef: 0.0,
            ls: 0.0,
            lf: 0.0,
            in_scope: match scope {
                ScheduleScope::Portfolio => true,
                ScheduleScope::Project(p) => t.project_id == Some(p),
            },
        })
        .collect();

    // Forward pass: as early as predecessors, the start date and today allow.
    for &n in &order {
        let i = n.index();
        if nodes[i].done {
            let task = nodes[i].task;
            let ef = task.completed_at.map_or(1.0, |c| end_offset(c.date()));
            let es = task
                .start_date
                .map(offset)
                .filter(|s| *s < ef)
                .unwrap_or(ef - nodes[i].duration);
            nodes[i].es = es;
            nodes[i].ef = ef;
            continue;
        }
        let mut es: f64 = 0.0;
        if let Some(s) = nodes[i].task.start_date {
            es = es.max(offset(s));
        }
        if let Some(nb) = not_before.get(&nodes[i].task.id) {
            es = es.max(*nb);
        }
        for e in graph.edges_directed(n, Direction::Incoming) {
            es = es.max(nodes[e.source().index()].ef + *e.weight());
        }
        nodes[i].es = es;
        nodes[i].ef = es + nodes[i].duration;
    }

    // Each project (and the inbox) is judged against its own deadline.
    let project_by_id: HashMap<Uuid, &Project> = projects.iter().map(|p| (p.id, p)).collect();
    let mut finish: BTreeMap<Option<Uuid>, f64> = BTreeMap::new();
    for n in nodes.iter().filter(|n| n.in_scope && !n.done) {
        let f = finish.entry(n.task.project_id).or_insert(f64::NEG_INFINITY);
        *f = f.max(n.ef);
    }
    let deadline = |group: Option<Uuid>| -> f64 {
        group
            .and_then(|p| project_by_id.get(&p))
            .and_then(|p| p.target_date)
            .map(end_offset)
            .unwrap_or_else(|| finish.get(&group).copied().unwrap_or(0.0))
    };

    // Backward pass: as late as successors and the deadline allow.
    for &n in order.iter().rev() {
        let i = n.index();
        if nodes[i].done || !nodes[i].in_scope {
            nodes[i].ls = nodes[i].es;
            nodes[i].lf = nodes[i].ef;
            continue;
        }
        let mut lf = deadline(nodes[i].task.project_id);
        for e in graph.edges_directed(n, Direction::Outgoing) {
            let s = &nodes[e.target().index()];
            if s.in_scope && !s.done {
                lf = lf.min(s.ls - *e.weight());
            }
        }
        nodes[i].lf = lf;
        nodes[i].ls = lf - nodes[i].duration;
    }

    // Critical: the least slack in the project.
    let mut least: BTreeMap<Option<Uuid>, f64> = BTreeMap::new();
    for n in nodes.iter().filter(|n| n.in_scope && !n.done) {
        let m = least.entry(n.task.project_id).or_insert(f64::INFINITY);
        *m = m.min(n.ls - n.es);
    }
    let is_critical = |n: &Node| -> bool {
        n.in_scope
            && !n.done
            && least
                .get(&n.task.project_id)
                .is_some_and(|m| n.ls - n.es <= m + EPS)
    };

    let mut out: Vec<ScheduledTask> = nodes
        .iter()
        .filter(|n| n.in_scope)
        .map(|n| {
            let t = n.task;
            let slack = n.ls - n.es;
            let start_idx = (n.es + EPS).floor() as i64;
            let latest_start_idx = (n.ls + EPS).floor() as i64;
            ScheduledTask {
                id: t.id,
                title: t.title.clone(),
                project_id: t.project_id,
                project_title: t
                    .project_id
                    .and_then(|p| project_by_id.get(&p))
                    .map(|p| p.title.clone()),
                status: t.status,
                done: n.done,
                unestimated: n.unestimated && !n.done,
                duration_days: n.duration,
                es: n.es,
                ef: n.ef,
                start: day_at(start_idx),
                finish: day_at(finish_index(n.es, n.ef)),
                ls: n.ls,
                lf: n.lf,
                latest_start: day_at(latest_start_idx),
                latest_finish: day_at(finish_index(n.ls, n.lf).max(latest_start_idx)),
                slack_days: slack,
                critical: is_critical(n),
                late_by_days: (slack < -EPS).then(|| (-slack - EPS).ceil().max(1.0) as u32),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.es.total_cmp(&b.es)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });

    // Forecast per project.
    let mut project_ids: Vec<Uuid> = out.iter().filter_map(|t| t.project_id).collect();
    if let ScheduleScope::Project(p) = scope {
        project_ids.push(p);
    }
    project_ids.sort();
    project_ids.dedup();
    let forecasts: Vec<ProjectForecast> = project_ids
        .into_iter()
        .map(|pid| {
            let mine = || out.iter().filter(move |t| t.project_id == Some(pid));
            let open: Vec<&ScheduledTask> = mine().filter(|t| !t.done).collect();
            let last_task = if open.is_empty() {
                latest_finish(mine())
            } else {
                latest_finish(open.iter().copied())
            };
            let project = project_by_id.get(&pid);
            let target = project.and_then(|p| p.target_date);
            let finish_offset = last_task.map(|t| t.ef);
            let target_offset = target.map(end_offset);
            let late = match (finish_offset, target_offset) {
                (Some(f), Some(t)) if !open.is_empty() && f > t + EPS => {
                    Some((f - t - EPS).ceil().max(1.0) as u32)
                }
                _ => None,
            };
            ProjectForecast {
                project_id: pid,
                title: project.map_or_else(|| "Unknown project".to_owned(), |p| p.title.clone()),
                projected_finish: last_task.map(|t| t.finish),
                finish_offset,
                target_date: target,
                target_offset,
                late_by_days: late,
                open_tasks: open.len() as u32,
                unestimated_tasks: open.iter().filter(|t| t.unestimated).count() as u32,
                critical_tasks: open.iter().filter(|t| t.critical).count() as u32,
            }
        })
        .collect();

    // The axis: every bar, today, and the targets (one day of margin).
    let mut lo: i64 = 0;
    let mut hi: i64 = 1;
    for t in &out {
        lo = lo.min(t.es.floor() as i64);
        hi = hi.max(t.ef.ceil() as i64);
    }
    for f in &forecasts {
        if let Some(t) = f.target_offset {
            lo = lo.min(t.floor() as i64 - 1);
            hi = hi.max(t.ceil() as i64);
        }
    }
    hi = (hi + 1).min(lo + MAX_DAYS);
    let days = (lo..hi).map(day_at).collect();

    Ok(Schedule {
        scope,
        today,
        first_offset: lo,
        days,
        tasks: out,
        projects: forecasts,
    })
}

fn latest_finish<'a>(tasks: impl Iterator<Item = &'a ScheduledTask>) -> Option<&'a ScheduledTask> {
    tasks.max_by(|a, b| a.ef.total_cmp(&b.ef))
}

/// The tasks that decide their project's finish, earliest first.
pub fn critical_path(schedule: &Schedule) -> Vec<ScheduledTask> {
    schedule
        .tasks
        .iter()
        .filter(|t| t.critical)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeType, ProjectStatus};
    use proptest::prelude::*;
    use time::{macros::date, Month, OffsetDateTime};

    // 2027-03-01 is a Monday.
    const MON: Date = date!(2027 - 03 - 01);

    // The tests below mostly use Monday to Friday; these shadow the real functions.
    fn working_index(d: Date) -> i64 {
        super::working_index(WorkWeek::MON_FRI, d)
    }
    fn end_index(d: Date) -> i64 {
        super::end_index(WorkWeek::MON_FRI, d)
    }
    fn date_of(i: i64) -> Date {
        super::date_of(WorkWeek::MON_FRI, i)
    }
    fn compute(
        tasks: &[Task],
        edges: &[Edge],
        projects: &[Project],
        today: Date,
        scope: ScheduleScope,
    ) -> Result<Schedule, ScheduleError> {
        super::compute(tasks, edges, projects, today, WorkWeek::MON_FRI, scope)
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, estimate: Option<f64>) -> Task {
        Task {
            links: Vec::new(),
            task_type: None,
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: None,
            status: TaskStatus::Todo,
            estimate_days: estimate,
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

    fn done_on(mut t: Task, d: Date) -> Task {
        t.status = TaskStatus::Done;
        t.completed_at = Some(d.midnight().assume_utc());
        t
    }

    fn blocks(from: u128, to: u128, lag: Option<u64>) -> Edge {
        Edge {
            id: Uuid::from_u128(1_000 + from * 100 + to),
            edge_type: EdgeType::Blocks,
            from_type: NodeType::Task,
            from_id: id(from),
            to_type: NodeType::Task,
            to_id: id(to),
            attrs: lag.map_or(
                serde_json::json!({}),
                |l| serde_json::json!({ "lag_days": l }),
            ),
            created_at: OffsetDateTime::UNIX_EPOCH,
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

    fn run(tasks: &[Task], edges: &[Edge], projects: &[Project], scope: ScheduleScope) -> Schedule {
        compute(tasks, edges, projects, MON, scope).unwrap()
    }

    fn get<'a>(s: &'a Schedule, title: &str) -> &'a ScheduledTask {
        s.tasks
            .iter()
            .find(|t| t.title == title)
            .unwrap_or_else(|| panic!("no {title}"))
    }

    /// (es, ef, ls, lf, critical)
    fn numbers(s: &Schedule, title: &str) -> (f64, f64, f64, f64, bool) {
        let t = get(s, title);
        (t.es, t.ef, t.ls, t.lf, t.critical)
    }

    // --------------------------------------------------------------- calendar

    #[test]
    fn the_epoch_is_a_monday_and_indices_round_trip() {
        assert_eq!(
            date_of(0),
            Date::from_calendar_date(2000, Month::January, 3).unwrap()
        );
        assert_eq!(date_of(0).weekday(), time::Weekday::Monday);
        for i in -30..60 {
            let d = date_of(i);
            assert!(d.weekday().number_days_from_monday() < 5, "{d}");
            assert_eq!(working_index(d), i);
            assert_eq!(end_index(d), i + 1);
        }
    }

    #[test]
    fn weekends_belong_to_the_next_monday_for_starts_and_the_previous_friday_for_ends() {
        let sat = date!(2027 - 03 - 06);
        let sun = date!(2027 - 03 - 07);
        let mon = date!(2027 - 03 - 08);
        let fri = date!(2027 - 03 - 05);
        assert_eq!(working_index(sat), working_index(mon));
        assert_eq!(working_index(sun), working_index(mon));
        assert_eq!(end_index(sat), end_index(fri));
        assert_eq!(end_index(sun), end_index(fri));
        assert_eq!(end_index(fri), working_index(mon));
    }

    // ------------------------------------------------------------- hand-built

    #[test]
    fn a_diamond_has_the_expected_dates_and_critical_path() {
        // A(3) -> B(2), A -> C(4), B -> D(1), C -> D
        let tasks = [
            task(1, "A", Some(3.0)),
            task(2, "B", Some(2.0)),
            task(3, "C", Some(4.0)),
            task(4, "D", Some(1.0)),
        ];
        let edges = [
            blocks(1, 2, None),
            blocks(1, 3, None),
            blocks(2, 4, None),
            blocks(3, 4, None),
        ];
        let s = run(&tasks, &edges, &[], ScheduleScope::Portfolio);
        assert_eq!(numbers(&s, "A"), (0.0, 3.0, 0.0, 3.0, true));
        assert_eq!(numbers(&s, "B"), (3.0, 5.0, 5.0, 7.0, false));
        assert_eq!(numbers(&s, "C"), (3.0, 7.0, 3.0, 7.0, true));
        assert_eq!(numbers(&s, "D"), (7.0, 8.0, 7.0, 8.0, true));
        assert_eq!(get(&s, "B").slack_days, 2.0);
        let path: Vec<String> = critical_path(&s).into_iter().map(|t| t.title).collect();
        assert_eq!(path, ["A", "C", "D"]);
        // Monday + 3 days of work: A is Mon-Wed; D starts the Wednesday of next week.
        assert_eq!(
            (get(&s, "A").start, get(&s, "A").finish),
            (date!(2027 - 03 - 01), date!(2027 - 03 - 03))
        );
        assert_eq!(
            (get(&s, "D").start, get(&s, "D").finish),
            (date!(2027 - 03 - 10), date!(2027 - 03 - 10))
        );
        assert_eq!(get(&s, "B").latest_finish, date!(2027 - 03 - 09));
    }

    #[test]
    fn work_skips_weekends() {
        let tasks = [task(1, "Two days", Some(2.0))];
        let friday = date!(2027 - 03 - 05);
        let s = compute(&tasks, &[], &[], friday, ScheduleScope::Portfolio).unwrap();
        let t = get(&s, "Two days");
        assert_eq!((t.start, t.finish), (friday, date!(2027 - 03 - 08)));
        // On a Saturday it starts on Monday.
        let s = compute(
            &tasks,
            &[],
            &[],
            date!(2027 - 03 - 06),
            ScheduleScope::Portfolio,
        )
        .unwrap();
        assert_eq!(get(&s, "Two days").start, date!(2027 - 03 - 08));
        assert_eq!(get(&s, "Two days").es, 0.0);
    }

    #[test]
    fn start_dates_hold_a_task_back_but_the_past_does_not() {
        let mut later = task(1, "Later", Some(1.0));
        later.start_date = Some(date!(2027 - 03 - 04)); // Thursday
        let mut earlier = task(2, "Earlier", Some(1.0));
        earlier.start_date = Some(date!(2027 - 01 - 10));
        let s = run(&[later, earlier], &[], &[], ScheduleScope::Portfolio);
        assert_eq!(get(&s, "Later").es, 3.0);
        assert_eq!(
            get(&s, "Earlier").es,
            0.0,
            "tasks start from today at the earliest"
        );
    }

    #[test]
    fn lag_adds_working_days_between_finish_and_start() {
        let tasks = [task(1, "A", Some(1.0)), task(2, "B", Some(1.0))];
        let s = run(
            &tasks,
            &[blocks(1, 2, Some(2))],
            &[],
            ScheduleScope::Portfolio,
        );
        assert_eq!(numbers(&s, "B"), (3.0, 4.0, 3.0, 4.0, true));
        assert_eq!(numbers(&s, "A"), (0.0, 1.0, 0.0, 1.0, true));
    }

    #[test]
    fn unestimated_work_counts_as_one_day_and_is_flagged() {
        let tasks = [
            task(1, "Vague", None),
            task(2, "Milestone", Some(0.0)),
            task(3, "Half", Some(0.5)),
        ];
        let s = run(&tasks, &[], &[], ScheduleScope::Portfolio);
        assert!(get(&s, "Vague").unestimated && get(&s, "Vague").duration_days == 1.0);
        assert!(!get(&s, "Milestone").unestimated && get(&s, "Milestone").duration_days == 0.0);
        // A half day finishes the same day it starts; a milestone sits on its day.
        let half = get(&s, "Half");
        assert_eq!((half.start, half.finish), (MON, MON));
        assert_eq!(get(&s, "Milestone").finish, MON);
    }

    #[test]
    fn fractional_estimates_chain() {
        let tasks = [task(1, "A", Some(0.5)), task(2, "B", Some(1.5))];
        let s = run(&tasks, &[blocks(1, 2, None)], &[], ScheduleScope::Portfolio);
        assert_eq!(numbers(&s, "B"), (0.5, 2.0, 0.5, 2.0, true));
        assert_eq!(get(&s, "B").finish, date!(2027 - 03 - 02));
    }

    #[test]
    fn done_tasks_are_fixed_at_their_actual_dates() {
        // Finished on the Friday before: the successor isn't held up.
        let a = done_on(task(1, "A", Some(2.0)), date!(2027 - 02 - 26));
        let b = task(2, "B", Some(1.0));
        let s = run(
            &[a, b],
            &[blocks(1, 2, None)],
            &[],
            ScheduleScope::Portfolio,
        );
        let a = get(&s, "A");
        assert!(a.done && !a.critical && a.slack_days == 0.0);
        assert_eq!(a.finish, date!(2027 - 02 - 26));
        assert!(a.ef <= 0.0);
        assert_eq!(get(&s, "B").es, 0.0);
        // Finished today: the successor starts the next working day.
        let a = done_on(task(1, "A", Some(2.0)), MON);
        let b = task(2, "B", Some(1.0));
        let s = run(
            &[a, b],
            &[blocks(1, 2, None)],
            &[],
            ScheduleScope::Portfolio,
        );
        assert_eq!(get(&s, "B").es, 1.0);
        // A done task keeps its own start date when it has one.
        let mut a = done_on(task(1, "A", Some(2.0)), date!(2027 - 02 - 26));
        a.start_date = Some(date!(2027 - 02 - 24));
        let s = run(&[a], &[], &[], ScheduleScope::Portfolio);
        assert_eq!(get(&s, "A").start, date!(2027 - 02 - 24));
    }

    #[test]
    fn cancelled_tasks_are_ignored_and_do_not_delay_anything() {
        let mut gone = task(2, "Gone", Some(10.0));
        gone.status = TaskStatus::Cancelled;
        let tasks = [task(1, "A", Some(1.0)), gone, task(3, "B", Some(1.0))];
        let s = run(
            &tasks,
            &[blocks(1, 2, None), blocks(2, 3, None)],
            &[],
            ScheduleScope::Portfolio,
        );
        assert!(s.tasks.iter().all(|t| t.title != "Gone"));
        assert_eq!(get(&s, "B").es, 0.0);
    }

    #[test]
    fn a_loop_is_an_error_not_a_hang() {
        let tasks = [task(1, "A", Some(1.0)), task(2, "B", Some(1.0))];
        let r = compute(
            &tasks,
            &[blocks(1, 2, None), blocks(2, 1, None)],
            &[],
            MON,
            ScheduleScope::Portfolio,
        );
        assert_eq!(r.unwrap_err(), ScheduleError::Cycle);
    }

    // ---------------------------------------------------------- projects, scope

    #[test]
    fn each_project_is_judged_against_its_own_finish() {
        // Two unrelated projects: a 5-day chain and a 2-day task. Both have a critical path.
        let tasks = [
            in_project(task(1, "Long", Some(5.0)), 10),
            in_project(task(2, "Short", Some(2.0)), 11),
        ];
        let s = run(
            &tasks,
            &[],
            &[project(10, "P", None), project(11, "Q", None)],
            ScheduleScope::Portfolio,
        );
        assert!(get(&s, "Long").critical && get(&s, "Short").critical);
        assert_eq!(get(&s, "Short").slack_days, 0.0);
        assert_eq!(s.projects.len(), 2);
        let q = s.projects.iter().find(|p| p.title == "Q").unwrap();
        assert_eq!(q.projected_finish, Some(date!(2027 - 03 - 02)));
        assert_eq!(q.late_by_days, None);
    }

    #[test]
    fn cross_project_links_delay_the_blocked_project_in_every_scope() {
        let tasks = [
            in_project(task(1, "Design", Some(2.0)), 10),
            in_project(task(2, "Build", Some(3.0)), 11),
        ];
        let edges = [blocks(1, 2, None)];
        let projects = [project(10, "P", None), project(11, "Q", None)];
        // Q alone still waits for P's task.
        let q = run(&tasks, &edges, &projects, ScheduleScope::Project(id(11)));
        assert_eq!(q.tasks.len(), 1);
        assert_eq!(get(&q, "Build").es, 2.0);
        assert_eq!(q.projects[0].projected_finish, Some(date!(2027 - 03 - 05)));
        // P alone is judged on its own finish.
        let p = run(&tasks, &edges, &projects, ScheduleScope::Project(id(10)));
        assert_eq!(p.tasks.len(), 1);
        assert_eq!(numbers(&p, "Design"), (0.0, 2.0, 0.0, 2.0, true));
        // In the portfolio, the blocker's deadline can't be later than what it blocks needs.
        let all = run(&tasks, &edges, &projects, ScheduleScope::Portfolio);
        assert_eq!(numbers(&all, "Design").3, 2.0);
        assert_eq!(numbers(&all, "Build"), (2.0, 5.0, 2.0, 5.0, true));
    }

    #[test]
    fn an_unrealistic_target_gives_negative_slack_and_a_late_by() {
        // Five days of chained work, target after 3 working days (end of Wednesday).
        let tasks = [
            in_project(task(1, "A", Some(2.0)), 10),
            in_project(task(2, "B", Some(3.0)), 10),
        ];
        let p = project(10, "P", Some(date!(2027 - 03 - 03)));
        let s = run(
            &tasks,
            &[blocks(1, 2, None)],
            &[p],
            ScheduleScope::Project(id(10)),
        );
        assert_eq!(get(&s, "A").slack_days, -2.0);
        assert_eq!(get(&s, "B").slack_days, -2.0);
        assert_eq!(get(&s, "B").late_by_days, Some(2));
        assert!(get(&s, "A").critical && get(&s, "B").critical);
        let f = &s.projects[0];
        assert_eq!(f.late_by_days, Some(2));
        assert_eq!(f.target_offset, Some(3.0));
        assert_eq!(f.projected_finish, Some(date!(2027 - 03 - 05)));
    }

    #[test]
    fn a_generous_target_gives_positive_slack_and_the_driving_chain_stays_critical() {
        // Chain A(2)->B(3) and a side task C(1); target is the end of next Friday (10 days).
        let tasks = [
            in_project(task(1, "A", Some(2.0)), 10),
            in_project(task(2, "B", Some(3.0)), 10),
            in_project(task(3, "C", Some(1.0)), 10),
        ];
        let p = project(10, "P", Some(date!(2027 - 03 - 12)));
        let s = run(
            &tasks,
            &[blocks(1, 2, None)],
            &[p],
            ScheduleScope::Project(id(10)),
        );
        assert_eq!(get(&s, "B").slack_days, 5.0);
        assert!(get(&s, "A").critical && get(&s, "B").critical);
        assert!(!get(&s, "C").critical);
        assert_eq!(get(&s, "C").slack_days, 9.0);
        assert_eq!(s.projects[0].late_by_days, None);
        assert_eq!(s.projects[0].critical_tasks, 2);
    }

    #[test]
    fn a_target_on_a_weekend_means_the_friday_before() {
        let tasks = [in_project(task(1, "A", Some(5.0)), 10)];
        // Saturday: five working days from Monday end on Friday, which meets it exactly.
        let p = project(10, "P", Some(date!(2027 - 03 - 06)));
        let s = run(&tasks, &[], &[p], ScheduleScope::Portfolio);
        assert_eq!(get(&s, "A").slack_days, 0.0);
        assert_eq!(s.projects[0].late_by_days, None);
    }

    #[test]
    fn forecasts_cover_finished_projects_and_count_unestimated_work() {
        let done = done_on(
            in_project(task(1, "Shipped", Some(2.0)), 10),
            date!(2027 - 02 - 25),
        );
        let s = run(
            &[done],
            &[],
            &[project(10, "P", None)],
            ScheduleScope::Project(id(10)),
        );
        assert_eq!(s.projects[0].projected_finish, Some(date!(2027 - 02 - 25)));
        assert_eq!(s.projects[0].open_tasks, 0);
        assert_eq!(s.projects[0].late_by_days, None);

        let tasks = [
            in_project(task(1, "A", None), 10),
            in_project(task(2, "B", Some(1.0)), 10),
        ];
        let s = run(
            &tasks,
            &[],
            &[project(10, "P", None)],
            ScheduleScope::Project(id(10)),
        );
        assert_eq!(
            (s.projects[0].open_tasks, s.projects[0].unestimated_tasks),
            (2, 1)
        );
        // A project with no tasks still has a forecast in its own scope.
        let s = run(
            &[],
            &[],
            &[project(10, "P", None)],
            ScheduleScope::Project(id(10)),
        );
        assert_eq!(s.projects[0].projected_finish, None);
    }

    #[test]
    fn the_axis_covers_every_bar_today_and_the_target() {
        let tasks = [in_project(task(1, "A", Some(3.0)), 10)];
        let p = project(10, "P", Some(date!(2027 - 03 - 19)));
        let s = run(&tasks, &[], &[p], ScheduleScope::Project(id(10)));
        assert_eq!(s.first_offset, 0);
        assert_eq!(s.days.first(), Some(&MON));
        assert!(s.days.contains(&date!(2027 - 03 - 19)));
        assert!(s
            .days
            .iter()
            .all(|d| d.weekday().number_days_from_monday() < 5));
        // A far-future start can't make the reply huge.
        let mut far = task(2, "Far", Some(1.0));
        far.start_date = Some(date!(2040 - 01 - 01));
        let s = run(&[far], &[], &[], ScheduleScope::Portfolio);
        assert!(s.days.len() <= (MAX_DAYS as usize));
    }

    // -------------------------------------------------------------- properties

    #[derive(Debug, Clone)]
    struct Spec {
        durations: Vec<u8>,
        edges: Vec<(usize, usize, u8)>,
        done_first: bool,
    }

    fn arb_spec() -> impl Strategy<Value = Spec> {
        (2usize..9)
            .prop_flat_map(|n| {
                (
                    proptest::collection::vec(0u8..6, n),
                    proptest::collection::vec((0..n, 0..n, 0u8..3), 0..14),
                    any::<bool>(),
                )
            })
            .prop_map(|(durations, raw, done_first)| Spec {
                durations,
                // Only forward edges (low -> high index), so the graph is acyclic.
                edges: raw
                    .into_iter()
                    .filter_map(|(a, b, lag)| (a < b).then_some((a, b, lag)))
                    .collect(),
                done_first,
            })
    }

    fn build(spec: &Spec) -> (Vec<Task>, Vec<Edge>) {
        let mut tasks: Vec<Task> = spec
            .durations
            .iter()
            .enumerate()
            .map(|(i, d)| {
                in_project(
                    task(i as u128 + 1, &format!("T{i}"), Some(f64::from(*d))),
                    10,
                )
            })
            .collect();
        if spec.done_first {
            tasks[0] = done_on(tasks[0].clone(), date!(2027 - 02 - 26));
        }
        let mut seen = std::collections::HashSet::new();
        let edges = spec
            .edges
            .iter()
            .filter(|(a, b, _)| seen.insert((*a, *b)))
            .map(|(a, b, lag)| blocks(*a as u128 + 1, *b as u128 + 1, Some(u64::from(*lag))))
            .collect();
        (tasks, edges)
    }

    // ------------------------------------------------- other work weeks (spec 23)

    fn week_of(days: &[u8]) -> WorkWeek {
        WorkWeek::from_days(days).unwrap()
    }

    #[test]
    fn a_four_day_week_skips_friday_to_sunday() {
        let w = week_of(&[0, 1, 2, 3]);
        let wed = date!(2027 - 03 - 03);
        let thu = date!(2027 - 03 - 04);
        let fri = date!(2027 - 03 - 05);
        let next_mon = date!(2027 - 03 - 08);
        assert_eq!(
            super::working_index(w, fri),
            super::working_index(w, next_mon)
        );
        assert_eq!(super::end_index(w, fri), super::end_index(w, thu));
        // Wednesday, Thursday, then Monday.
        let start = super::working_index(w, wed);
        assert_eq!(super::date_of(w, start + 2), next_mon);
        assert_eq!(super::date_of(w, start + 1), thu);
    }

    #[test]
    fn a_sunday_to_thursday_week_schedules_on_those_days_only() {
        let w = week_of(&[6, 0, 1, 2, 3]);
        // A task of 3 days starting on a Thursday: Thu, Sun, Mon.
        let thu = date!(2027 - 03 - 04);
        let mut t = task(1, "T", Some(3.0));
        t.start_date = Some(thu);
        let s = super::compute(&[t], &[], &[], thu, w, ScheduleScope::Portfolio).unwrap();
        let t = get(&s, "T");
        assert_eq!(t.start, thu);
        assert_eq!(t.finish, date!(2027 - 03 - 08));
        assert!(s.days.iter().all(|d| w.contains_weekday(d.weekday())));
        // Today is a Friday (a day off): the work starts on Sunday.
        let fri = date!(2027 - 03 - 05);
        let s = super::compute(
            &[task(2, "U", Some(1.0))],
            &[],
            &[],
            fri,
            w,
            ScheduleScope::Portfolio,
        )
        .unwrap();
        assert_eq!(get(&s, "U").start, date!(2027 - 03 - 07));
    }

    #[test]
    fn the_default_week_gives_the_same_schedule_as_before() {
        assert_eq!(WorkWeek::default(), WorkWeek::MON_FRI);
        let tasks = [task(1, "A", Some(7.0))];
        let s = compute(&tasks, &[], &[], MON, ScheduleScope::Portfolio).unwrap();
        // Monday + 7 working days ends on the Tuesday of the following week.
        assert_eq!(get(&s, "A").finish, date!(2027 - 03 - 09));
    }

    proptest! {
        /// Without a target the deadline is the projected finish: nothing has negative slack,
        /// every task fits between its earliest and latest dates, and something is critical.
        #[test]
        fn slack_is_never_negative_without_a_target(spec in arb_spec()) {
            let (tasks, edges) = build(&spec);
            let s = compute(&tasks, &edges, &[project(10, "P", None)], MON, ScheduleScope::Portfolio).unwrap();
            for t in s.tasks.iter().filter(|t| !t.done) {
                prop_assert!(t.slack_days >= -1e-9, "{} {}", t.title, t.slack_days);
                prop_assert!(t.lf >= t.ef - 1e-9);
                prop_assert!(t.ls >= t.es - 1e-9);
                prop_assert!(t.late_by_days.is_none());
            }
            let open = s.tasks.iter().filter(|t| !t.done).count();
            if open > 0 {
                let critical: Vec<&ScheduledTask> = s.tasks.iter().filter(|t| t.critical).collect();
                prop_assert!(!critical.is_empty());
                for c in critical {
                    prop_assert!(c.slack_days.abs() < 1e-9, "critical task with slack {}", c.slack_days);
                }
            }
        }

        /// Earliest times are tight: each task starts exactly when its constraints allow, so
        /// nothing can be moved earlier, and every blocks link is honoured.
        #[test]
        fn earliest_starts_are_tight_and_links_are_honoured(spec in arb_spec()) {
            let (tasks, edges) = build(&spec);
            let s = compute(&tasks, &edges, &[], MON, ScheduleScope::Portfolio).unwrap();
            let by_id: HashMap<Uuid, &ScheduledTask> = s.tasks.iter().map(|t| (t.id, t)).collect();
            for t in s.tasks.iter().filter(|t| !t.done) {
                prop_assert!(t.es >= -1e-9);
                prop_assert!((t.ef - t.es - t.duration_days).abs() < 1e-9);
                let mut bound: f64 = 0.0;
                for e in edges.iter().filter(|e| e.to_id == t.id) {
                    let p = by_id[&e.from_id];
                    let lag = e.attrs["lag_days"].as_u64().unwrap_or(0) as f64;
                    prop_assert!(t.es >= p.ef + lag - 1e-9);
                    bound = bound.max(p.ef + lag);
                }
                prop_assert!((t.es - bound).abs() < 1e-9, "{} starts at {} but is only held until {}", t.title, t.es, bound);
            }
        }

        /// A target only changes slack by the same amount for everything on the driving chain:
        /// moving it later by k days raises the critical tasks' slack by k and nobody is late.
        #[test]
        fn a_later_target_adds_slack_equally(spec in arb_spec(), extra in 1u32..10) {
            let (tasks, edges) = build(&spec);
            let base = compute(&tasks, &edges, &[project(10, "P", None)], MON, ScheduleScope::Portfolio).unwrap();
            let Some(finish) = base.projects.first().and_then(|p| p.finish_offset) else { return Ok(()); };
            // A target exactly at the finish changes nothing; `extra` days later adds `extra`.
            let target = date_of(working_index(MON) + finish.ceil() as i64 - 1 + i64::from(extra));
            let s = compute(&tasks, &edges, &[project(10, "P", Some(target))], MON, ScheduleScope::Portfolio).unwrap();
            for t in s.tasks.iter().filter(|t| t.critical) {
                prop_assert!((t.slack_days - f64::from(extra)).abs() < 1e-9, "{}", t.slack_days);
                prop_assert!(t.late_by_days.is_none());
            }
        }

        #[test]
        fn calendar_arithmetic_round_trips(i in -2000i64..4000) {
            prop_assert_eq!(working_index(date_of(i)), i);
            prop_assert_eq!(end_index(date_of(i)), i + 1);
        }

        /// For any work week: working days map to themselves, days off map to the next working
        /// day (start) or the previous one (end), and the index never goes backwards.
        #[test]
        fn calendar_arithmetic_holds_for_any_work_week(mask in 1u8..128, i in -1500i64..3000, offset in 0i64..20000) {
            let days: Vec<u8> = (0..7).filter(|d| mask & (1 << d) != 0).collect();
            let w = WorkWeek::from_days(&days).unwrap();
            let d = super::date_of(w, i);
            prop_assert!(w.contains_weekday(d.weekday()));
            prop_assert_eq!(super::working_index(w, d), i);
            prop_assert_eq!(super::end_index(w, d), i + 1);
            let any = date!(2010 - 01 - 01) + time::Duration::days(offset);
            let start = super::working_index(w, any);
            prop_assert!(super::date_of(w, start) >= any);
            prop_assert!(super::date_of(w, start - 1) < any);
            let next = any.next_day().unwrap();
            prop_assert!(super::working_index(w, next) >= start);
            let end = super::end_index(w, any);
            prop_assert!(super::date_of(w, end - 1) <= any);
            prop_assert!(super::date_of(w, end) > any);
        }
    }
}
