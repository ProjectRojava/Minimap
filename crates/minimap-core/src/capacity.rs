//! Capacity (spec 17): per person per week, the scheduled working days of their open tasks
//! (weighted by `allocation_pct`) against their weekly capacity. Pure.
//!
//! The schedule (13) says which working days each task occupies; this module slices those
//! windows into Monday-to-Friday weeks. A task of `d` days at allocation `a` contributes
//! `overlap x a / 100` days to each week it touches, so the weekly figures add up to
//! `d x a / 100` over the whole task. Capacity is `weekly_capacity_hours / hours_per_day` days.
//! Above 100% is overloaded. Separately, a person with more open tasks than a configurable limit
//! is flagged, which still works when estimates are missing and the load is only a guess.

use std::collections::HashMap;

use minimap_types::{
    Capacity, CapacityTask, Date, Edge, EdgeType, NodeRef, NodeSummary, NodeType, Person,
    PersonCapacity, Project, ScheduleScope, ScheduledTask, Task, TaskStatus, Uuid, WeekLoad,
};
use time::Duration;

use crate::{schedule, this_week::monday_of};

const EPS: f64 = 1e-9;
/// Weeks shown when the caller doesn't say.
pub const DEFAULT_WEEKS: u32 = 8;
/// Most weeks one reply covers.
pub const MAX_WEEKS: u32 = 52;
/// Working days in a week.
const WEEK_DAYS: f64 = 5.0;

pub struct CapacityInput<'a> {
    pub tasks: &'a [Task],
    pub edges: &'a [Edge],
    pub projects: &'a [Project],
    pub people: &'a [Person],
    pub today: Date,
    pub hours_per_day: f64,
    /// Any date in the first week (today's week when `None`).
    pub from: Option<Date>,
    /// Any date in the last week; otherwise `weeks` weeks from `from`.
    pub to: Option<Date>,
    pub weeks: Option<u32>,
    /// More open tasks than this flags a person.
    pub task_limit: u32,
}

/// Working days of overlap between a task's window `[es, ef)` and a week's `[lo, hi)`.
pub fn overlap_days(es: f64, ef: f64, lo: f64, hi: f64) -> f64 {
    (ef.min(hi) - es.max(lo)).max(0.0)
}

fn is_open(t: &Task) -> bool {
    t.archived_at.is_none() && !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
}

fn allocation_of(e: &Edge) -> f64 {
    e.attrs
        .get("allocation_pct")
        .and_then(|a| a.as_f64())
        .unwrap_or(100.0)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub fn compute(input: &CapacityInput) -> Capacity {
    let today = input.today;
    let first = monday_of(input.from.unwrap_or(today));
    let count = match input.to {
        Some(to) if monday_of(to) >= first => ((monday_of(to) - first).whole_days() / 7 + 1) as u32,
        Some(_) => 1,
        None => input.weeks.unwrap_or(DEFAULT_WEEKS).max(1),
    }
    .min(MAX_WEEKS);
    let week_starts: Vec<Date> = (0..count)
        .map(|i| first + Duration::days(i64::from(i) * 7))
        .collect();
    let span = Duration::days(i64::from(count) * 7);

    let mut warnings = Vec::new();
    let schedule = match schedule::compute(
        input.tasks,
        input.edges,
        input.projects,
        today,
        ScheduleScope::Portfolio,
    ) {
        Ok(s) => Some(s),
        Err(e) => {
            warnings.push(format!(
                "{e}. Weekly load can't be worked out until the loop is removed; open task counts still work."
            ));
            None
        }
    };
    let scheduled: HashMap<Uuid, &ScheduledTask> = schedule
        .iter()
        .flat_map(|s| s.tasks.iter())
        .map(|t| (t.id, t))
        .collect();
    let task_by_id: HashMap<Uuid, &Task> = input.tasks.iter().map(|t| (t.id, t)).collect();
    let project_title: HashMap<Uuid, &str> = input
        .projects
        .iter()
        .map(|p| (p.id, p.title.as_str()))
        .collect();

    // Each person's open assigned tasks with their allocation.
    let mut assigned: HashMap<Uuid, Vec<(&Task, f64)>> = HashMap::new();
    for e in input.edges.iter().filter(|e| {
        e.edge_type == EdgeType::AssignedTo
            && e.archived_at.is_none()
            && e.from_type == NodeType::Task
    }) {
        if let Some(t) = task_by_id.get(&e.from_id).filter(|t| is_open(t)) {
            assigned
                .entry(e.to_id)
                .or_default()
                .push((t, allocation_of(e)));
        }
    }

    let t0 = schedule::working_index(today);
    let mut people: Vec<PersonCapacity> = Vec::new();
    for p in input.people.iter().filter(|p| p.archived_at.is_none()) {
        let mine = assigned.get(&p.id).map_or(&[][..], Vec::as_slice);
        let capacity_days = if input.hours_per_day > 0.0 {
            p.weekly_capacity_hours / input.hours_per_day
        } else {
            0.0
        };
        let weeks: Vec<WeekLoad> = week_starts
            .iter()
            .map(|ws| {
                let lo = (schedule::working_index(*ws) - t0) as f64;
                let hi = lo + WEEK_DAYS;
                let mut tasks: Vec<CapacityTask> = Vec::new();
                for (t, alloc) in mine {
                    let Some(s) = scheduled.get(&t.id).filter(|s| !s.done) else {
                        continue;
                    };
                    let days = overlap_days(s.es, s.ef, lo, hi) * alloc / 100.0;
                    if days <= EPS {
                        continue;
                    }
                    tasks.push(CapacityTask {
                        id: t.id,
                        title: t.title.clone(),
                        project_title: t
                            .project_id
                            .and_then(|pid| project_title.get(&pid))
                            .map(|n| (*n).to_owned()),
                        days: (days * 100.0).round() / 100.0,
                        allocation_pct: alloc.round().max(0.0) as u32,
                        start: s.start,
                        finish: s.finish,
                        unestimated: s.unestimated,
                    });
                }
                tasks.sort_by(|a, b| {
                    b.days
                        .total_cmp(&a.days)
                        .then_with(|| a.title.cmp(&b.title))
                        .then_with(|| a.id.cmp(&b.id))
                });
                let load_days: f64 = tasks.iter().map(|t| t.days).sum();
                let load_pct = if capacity_days > 0.0 {
                    round1(load_days / capacity_days * 100.0)
                } else {
                    0.0
                };
                WeekLoad {
                    week_start: *ws,
                    load_days: (load_days * 100.0).round() / 100.0,
                    capacity_days,
                    load_pct,
                    over: load_pct > 100.0 + EPS,
                    tasks,
                }
            })
            .collect();
        let peak = weeks
            .iter()
            .filter(|w| w.load_pct > 0.0)
            .max_by(|a, b| a.load_pct.total_cmp(&b.load_pct));
        let active = mine.len() as u32;
        people.push(PersonCapacity {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, p.id),
                label: p.name.clone(),
                archived: false,
            },
            weekly_capacity_hours: p.weekly_capacity_hours,
            peak_pct: peak.map_or(0.0, |w| w.load_pct),
            peak_week: peak.map(|w| w.week_start),
            over_weeks: weeks.iter().filter(|w| w.over).count() as u32,
            active_tasks: active,
            over_task_limit: active > input.task_limit,
            unestimated_tasks: mine
                .iter()
                .filter(|(t, _)| t.estimate_days.is_none())
                .count() as u32,
            weeks,
        });
    }
    let flagged = |p: &PersonCapacity| p.over_weeks > 0 || p.over_task_limit;
    people.sort_by(|a, b| {
        flagged(b)
            .cmp(&flagged(a))
            .then_with(|| b.peak_pct.total_cmp(&a.peak_pct))
            .then_with(|| a.person.label.cmp(&b.person.label))
    });

    Capacity {
        today,
        current_week_start: monday_of(today),
        from: first,
        to: first + span - Duration::days(1),
        prev_from: first - span,
        next_from: first + span,
        weeks: week_starts,
        people,
        task_limit: input.task_limit,
        hours_per_day: input.hours_per_day,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::ProjectStatus;
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    // Monday 2027-03-01.
    const MON: Date = date!(2027 - 03 - 01);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, title: &str, days: Option<f64>) -> Task {
        Task {
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: None,
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

    fn assigned(task: u128, person: u128, pct: Option<u64>) -> Edge {
        Edge {
            id: Uuid::from_u128(70_000 + task * 1000 + person),
            edge_type: EdgeType::AssignedTo,
            from_type: NodeType::Task,
            from_id: id(task),
            to_type: NodeType::Person,
            to_id: id(person),
            attrs: pct.map_or(
                serde_json::json!({}),
                |p| serde_json::json!({ "allocation_pct": p }),
            ),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn blocks(a: u128, b: u128) -> Edge {
        Edge {
            id: Uuid::from_u128(80_000 + a * 1000 + b),
            edge_type: EdgeType::Blocks,
            from_type: NodeType::Task,
            from_id: id(a),
            to_type: NodeType::Task,
            to_id: id(b),
            attrs: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    #[derive(Default)]
    struct Fixture {
        tasks: Vec<Task>,
        edges: Vec<Edge>,
        people: Vec<Person>,
        projects: Vec<Project>,
    }

    impl Fixture {
        fn capacity(&self, today: Date, weeks: Option<u32>) -> Capacity {
            self.with(today, None, None, weeks, 10)
        }

        fn with(
            &self,
            today: Date,
            from: Option<Date>,
            to: Option<Date>,
            weeks: Option<u32>,
            limit: u32,
        ) -> Capacity {
            compute(&CapacityInput {
                tasks: &self.tasks,
                edges: &self.edges,
                projects: &self.projects,
                people: &self.people,
                today,
                hours_per_day: 8.0,
                from,
                to,
                weeks,
                task_limit: limit,
            })
        }
    }

    fn of<'a>(c: &'a Capacity, name: &str) -> &'a PersonCapacity {
        c.people
            .iter()
            .find(|p| p.person.label == name)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    fn pcts(p: &PersonCapacity) -> Vec<f64> {
        p.weeks.iter().map(|w| w.load_pct).collect()
    }

    #[test]
    fn a_task_spanning_two_weeks_is_split_between_them() {
        // 8 days starting Monday: all of week one (5 days = 100%, not over) and 3 days of week two.
        let f = Fixture {
            tasks: vec![task(1, "Big", Some(8.0))],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![assigned(1, 30, None)],
            ..Default::default()
        };
        let c = f.capacity(MON, Some(3));
        let p = of(&c, "Priya");
        assert_eq!(pcts(p), [100.0, 60.0, 0.0]);
        assert_eq!(p.weeks[0].load_days, 5.0);
        assert_eq!(p.weeks[1].load_days, 3.0);
        assert!(!p.weeks[0].over, "exactly 100% is full, not over");
        assert_eq!(
            (p.over_weeks, p.peak_pct, p.peak_week),
            (0, 100.0, Some(MON))
        );
        // The cell lists what makes it up.
        let t = &p.weeks[1].tasks[0];
        assert_eq!(
            (t.title.as_str(), t.days, t.allocation_pct),
            ("Big", 3.0, 100)
        );
        assert_eq!((t.start, t.finish), (MON, date!(2027 - 03 - 10)));
        assert!(p.weeks[2].tasks.is_empty());
    }

    #[test]
    fn parallel_work_adds_up_and_over_100_is_flagged() {
        // Today is Wednesday: this week holds three days of each of two parallel tasks.
        let wed = date!(2027 - 03 - 03);
        let f = Fixture {
            tasks: vec![
                task(1, "A", Some(3.0)),
                task(2, "B", Some(4.0)),
                task(3, "C", Some(1.0)),
            ],
            people: vec![person(30, "Priya", 40.0), person(31, "Raj", 40.0)],
            edges: vec![
                assigned(1, 30, None),
                assigned(2, 30, None),
                assigned(3, 31, None),
            ],
            ..Default::default()
        };
        let c = f.capacity(wed, Some(2));
        let p = of(&c, "Priya");
        assert_eq!(pcts(p), [120.0, 20.0]);
        assert!(p.weeks[0].over && !p.weeks[1].over);
        assert_eq!((p.over_weeks, p.peak_week), (1, Some(MON)));
        // Biggest contributor first (equal here, so by title).
        let titles: Vec<&str> = p.weeks[0].tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["A", "B"]);
        // Overloaded people come first.
        assert_eq!(c.people[0].person.label, "Priya");
        assert_eq!(of(&c, "Raj").peak_pct, 20.0);
    }

    #[test]
    fn allocation_and_part_time_capacity_change_the_percentage() {
        let f = Fixture {
            tasks: vec![task(1, "Four days", Some(4.0))],
            people: vec![
                person(30, "Half time on it", 40.0),
                person(31, "Part-timer", 20.0),
                person(32, "Full", 40.0),
            ],
            edges: vec![
                assigned(1, 30, Some(50)),
                assigned(1, 31, None),
                assigned(1, 32, None),
            ],
            ..Default::default()
        };
        let c = f.capacity(MON, Some(1));
        // 4 days at 50% = 2 days of a 5-day week.
        assert_eq!(of(&c, "Half time on it").weeks[0].load_pct, 40.0);
        assert_eq!(of(&c, "Half time on it").weeks[0].tasks[0].days, 2.0);
        // 4 days against 20h (2.5 days) of capacity.
        assert_eq!(of(&c, "Part-timer").weeks[0].load_pct, 160.0);
        assert_eq!(of(&c, "Part-timer").weeks[0].capacity_days, 2.5);
        assert_eq!(of(&c, "Full").weeks[0].load_pct, 80.0);
    }

    #[test]
    fn hours_per_day_scales_capacity() {
        let f = Fixture {
            tasks: vec![task(1, "T", Some(5.0))],
            people: vec![person(30, "P", 40.0)],
            edges: vec![assigned(1, 30, None)],
            ..Default::default()
        };
        // At 4 hours a day a 40h week is 10 days of capacity.
        let c = compute(&CapacityInput {
            tasks: &f.tasks,
            edges: &f.edges,
            projects: &f.projects,
            people: &f.people,
            today: MON,
            hours_per_day: 4.0,
            from: None,
            to: None,
            weeks: Some(1),
            task_limit: 10,
        });
        assert_eq!(of(&c, "P").weeks[0].capacity_days, 10.0);
        assert_eq!(of(&c, "P").weeks[0].load_pct, 50.0);
    }

    #[test]
    fn only_open_assigned_work_of_active_people_counts() {
        let mut done = task(2, "Done", Some(5.0));
        done.status = TaskStatus::Done;
        done.completed_at = Some(date!(2027 - 02 - 26).midnight().assume_utc());
        let mut cancelled = task(3, "Cancelled", Some(5.0));
        cancelled.status = TaskStatus::Cancelled;
        let mut archived_person = person(31, "Gone", 40.0);
        archived_person.archived_at = Some(OffsetDateTime::UNIX_EPOCH);
        let f = Fixture {
            tasks: vec![
                task(1, "Real", Some(2.0)),
                done,
                cancelled,
                task(4, "Unassigned", Some(5.0)),
                task(5, "Gone's", Some(5.0)),
            ],
            people: vec![person(30, "Priya", 40.0), archived_person],
            edges: vec![
                assigned(1, 30, None),
                assigned(2, 30, None),
                assigned(3, 30, None),
                assigned(5, 31, None),
            ],
            ..Default::default()
        };
        let c = f.capacity(MON, Some(1));
        assert_eq!(c.people.len(), 1, "archived people aren't listed");
        let p = of(&c, "Priya");
        assert_eq!(p.weeks[0].load_days, 2.0);
        assert_eq!(p.active_tasks, 1);
    }

    #[test]
    fn dependencies_delay_the_load_into_later_weeks() {
        // B (3 days) can't start until A (5 days) is done: it lands in week two.
        let f = Fixture {
            tasks: vec![task(1, "A", Some(5.0)), task(2, "B", Some(3.0))],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![blocks(1, 2), assigned(1, 30, None), assigned(2, 30, None)],
            ..Default::default()
        };
        let p = of(&f.capacity(MON, Some(2)), "Priya").clone();
        assert_eq!(pcts(&p), [100.0, 60.0]);
        assert_eq!(p.weeks[1].tasks[0].title, "B");
    }

    #[test]
    fn the_task_count_flag_is_separate_and_counts_unscheduled_work_too() {
        let mut f = Fixture {
            people: vec![person(30, "Busy", 40.0), person(31, "Ok", 40.0)],
            ..Default::default()
        };
        for i in 0..11u128 {
            f.tasks.push(task(100 + i, &format!("b{i}"), None));
            f.edges.push(assigned(100 + i, 30, Some(1)));
        }
        for i in 0..10u128 {
            f.tasks.push(task(200 + i, &format!("o{i}"), Some(1.0)));
            f.edges.push(assigned(200 + i, 31, Some(1)));
        }
        let c = f.with(MON, None, None, Some(1), 10);
        let busy = of(&c, "Busy");
        assert_eq!(
            (
                busy.active_tasks,
                busy.over_task_limit,
                busy.unestimated_tasks
            ),
            (11, true, 11)
        );
        assert!(!of(&c, "Ok").over_task_limit, "exactly the limit is fine");
        assert_eq!(c.task_limit, 10);
        // A stricter limit flags both; the flag has nothing to do with load.
        let strict = f.with(MON, None, None, Some(1), 5);
        assert!(of(&strict, "Ok").over_task_limit);
        assert_eq!(
            of(&strict, "Ok").peak_pct,
            2.0,
            "10 one-day tasks at 1% each"
        );
        // Flagged people sort before the rest.
        assert_eq!(c.people[0].person.label, "Busy");
    }

    #[test]
    fn unestimated_work_is_counted_as_a_day_and_marked() {
        let f = Fixture {
            tasks: vec![task(1, "Vague", None)],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![assigned(1, 30, None)],
            ..Default::default()
        };
        let p = of(&f.capacity(MON, Some(1)), "Priya").clone();
        assert_eq!(p.weeks[0].load_days, 1.0);
        assert!(p.weeks[0].tasks[0].unestimated);
        assert_eq!(p.unestimated_tasks, 1);
    }

    #[test]
    fn the_window_snaps_to_weeks_and_can_be_given_as_a_range_or_a_count() {
        let f = Fixture::default();
        let c = f.capacity(date!(2027 - 03 - 03), None);
        assert_eq!(c.weeks.len(), 8);
        assert_eq!(c.weeks[0], MON);
        assert_eq!((c.from, c.to), (MON, date!(2027 - 04 - 25)));
        assert_eq!(c.current_week_start, MON);
        assert_eq!(
            (c.prev_from, c.next_from),
            (date!(2027 - 01 - 04), date!(2027 - 04 - 26))
        );
        // A range: from any date in the first week to any date in the last.
        let r = f.with(
            MON,
            Some(date!(2027 - 03 - 04)),
            Some(date!(2027 - 03 - 20)),
            None,
            10,
        );
        assert_eq!(
            r.weeks,
            vec![MON, date!(2027 - 03 - 08), date!(2027 - 03 - 15)]
        );
        assert_eq!(r.to, date!(2027 - 03 - 21));
        // A backwards range is one week; the count is capped.
        assert_eq!(
            f.with(
                MON,
                Some(date!(2027 - 03 - 10)),
                Some(date!(2027 - 03 - 01)),
                None,
                10
            )
            .weeks
            .len(),
            1
        );
        assert_eq!(f.capacity(MON, Some(500)).weeks.len(), MAX_WEEKS as usize);
        assert_eq!(f.capacity(MON, Some(0)).weeks.len(), 1);
        // A window in the future has no load from today's work.
        let far = f.with(MON, Some(date!(2028 - 01 - 03)), None, Some(2), 10);
        assert_eq!(far.weeks[0], date!(2028 - 01 - 03));
    }

    #[test]
    fn past_weeks_are_empty_because_the_schedule_starts_today() {
        let f = Fixture {
            tasks: vec![task(1, "T", Some(2.0))],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![assigned(1, 30, None)],
            ..Default::default()
        };
        let c = f.with(
            date!(2027 - 03 - 03),
            Some(date!(2027 - 02 - 15)),
            None,
            Some(3),
            10,
        );
        assert_eq!(pcts(of(&c, "Priya")), [0.0, 0.0, 40.0]);
    }

    #[test]
    fn a_loop_in_the_plan_warns_but_still_counts_open_tasks() {
        let f = Fixture {
            tasks: vec![task(1, "A", Some(1.0)), task(2, "B", Some(1.0))],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![
                blocks(1, 2),
                blocks(2, 1),
                assigned(1, 30, None),
                assigned(2, 30, None),
            ],
            ..Default::default()
        };
        let c = f.capacity(MON, Some(1));
        assert_eq!(c.warnings.len(), 1);
        assert!(c.warnings[0].contains("loop"));
        let p = of(&c, "Priya");
        assert_eq!((p.active_tasks, p.peak_pct), (2, 0.0));
    }

    #[test]
    fn projects_name_the_work_in_a_cell() {
        let mut t = task(1, "T", Some(1.0));
        t.project_id = Some(id(10));
        let f = Fixture {
            tasks: vec![t],
            people: vec![person(30, "Priya", 40.0)],
            edges: vec![assigned(1, 30, None)],
            projects: vec![Project {
                id: id(10),
                title: "API".into(),
                slug: "api".into(),
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: None,
                status: ProjectStatus::Active,
                priority: 3,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            }],
        };
        let c = f.capacity(MON, Some(1));
        assert_eq!(
            of(&c, "Priya").weeks[0].tasks[0].project_title.as_deref(),
            Some("API")
        );
    }

    proptest! {
        /// Weekly figures add up: over a window wide enough to hold everything, a person's load
        /// is exactly the sum of their tasks' durations times allocation, each cell's tasks sum
        /// to the cell, and nothing is negative.
        #[test]
        fn weekly_loads_add_up_to_the_work_assigned(
            specs in proptest::collection::vec((0u8..9, 10u8..101, 0usize..3), 1..8),
            edges in proptest::collection::vec((0usize..8, 0usize..8), 0..6),
        ) {
            let people = vec![person(30, "A", 40.0), person(31, "B", 20.0), person(32, "C", 40.0)];
            let tasks: Vec<Task> = specs.iter().enumerate()
                .map(|(i, (d, _, _))| task(i as u128 + 1, &format!("T{i}"), Some(f64::from(*d))))
                .collect();
            let mut es: Vec<Edge> = specs.iter().enumerate()
                .map(|(i, (_, pct, who))| assigned(i as u128 + 1, 30 + *who as u128, Some(u64::from(*pct))))
                .collect();
            let mut seen = std::collections::HashSet::new();
            for (a, b) in edges {
                if a < b && b < tasks.len() && seen.insert((a, b)) {
                    es.push(blocks(a as u128 + 1, b as u128 + 1));
                }
            }
            let f = Fixture { tasks, edges: es, people, ..Default::default() };
            let c = f.capacity(MON, Some(MAX_WEEKS));
            for (who, name) in [(0usize, "A"), (1, "B"), (2, "C")] {
                let p = of(&c, name);
                let expected: f64 = specs.iter()
                    .filter(|(_, _, w)| *w == who)
                    .map(|(d, pct, _)| f64::from(*d) * f64::from(*pct) / 100.0)
                    .sum();
                let total: f64 = p.weeks.iter().map(|w| w.load_days).sum();
                prop_assert!((total - expected).abs() < 0.02 * p.weeks.len() as f64 + 1e-6, "{name}: {total} vs {expected}");
                for w in &p.weeks {
                    prop_assert!(w.load_days >= 0.0 && w.load_pct >= 0.0);
                    let cell: f64 = w.tasks.iter().map(|t| t.days).sum();
                    prop_assert!((cell - w.load_days).abs() < 0.01, "cell {cell} vs {}", w.load_days);
                    prop_assert_eq!(w.over, w.load_pct > 100.0);
                }
                prop_assert_eq!(p.over_weeks as usize, p.weeks.iter().filter(|w| w.over).count());
                let max = p.weeks.iter().map(|w| w.load_pct).fold(0.0, f64::max);
                prop_assert!((p.peak_pct - max).abs() < 1e-9);
            }
        }
    }
}
