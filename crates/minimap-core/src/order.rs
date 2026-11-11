//! The order of work (spec 60, ADR-0024). Pure.
//!
//! Each task carries `order_step`, its step in its assignee's order: step 1 is done first, tasks
//! on the same step at the same time. The schedule reads the order as "comes after" pairs
//! ([`links`]): every task of a step comes after every task of the nearest earlier step. Only
//! open, assigned, unparked tasks that are not meetings count; a pair whose later task is already
//! in progress is left out (it has started, and its start is a fact).

use std::collections::{BTreeMap, HashMap, HashSet};

use minimap_types::{
    Edge, EdgeType, NodeRef, NodeSummary, NodeType, OrderCard, OrderStep, Project, Schedule,
    ScheduleScope, Task, TaskStatus, Uuid, WorkOrder, WorkWeek,
};
use time::Date;

/// Whether a task can be in an order: open, not archived, not a meeting, not on the backburner.
pub fn counts(t: &Task) -> bool {
    !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
        && t.archived_at.is_none()
        && !t.is_meeting()
        && !t.is_parked()
}

/// The active `assigned_to` links: task -> (person, allocation %), oldest link first.
fn assignments(edges: &[Edge]) -> HashMap<Uuid, Vec<(Uuid, u32)>> {
    let mut links: Vec<&Edge> = edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::AssignedTo && e.archived_at.is_none())
        .collect();
    links.sort_by_key(|e| e.id);
    let mut out: HashMap<Uuid, Vec<(Uuid, u32)>> = HashMap::new();
    for e in links {
        let pct = e
            .attrs
            .get("allocation_pct")
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(100);
        out.entry(e.from_id).or_default().push((e.to_id, pct));
    }
    out
}

/// The tasks that can be in `person`'s order: the ones that [`count`](counts) and are assigned
/// to them.
pub fn tasks_of<'a>(tasks: &'a [Task], edges: &[Edge], person: Uuid) -> Vec<&'a Task> {
    let assigned = assignments(edges);
    tasks
        .iter()
        .filter(|t| counts(t))
        .filter(|t| {
            assigned
                .get(&t.id)
                .is_some_and(|a| a.iter().any(|(p, _)| *p == person))
        })
        .collect()
}

/// Each person's order: the steps (in order) of the ids of their tasks that count.
pub fn steps_by_person(tasks: &[Task], edges: &[Edge]) -> BTreeMap<Uuid, Vec<Vec<Uuid>>> {
    let assigned = assignments(edges);
    let mut by_person: BTreeMap<Uuid, BTreeMap<u32, Vec<Uuid>>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| counts(t)) {
        let Some(step) = t.order_step else { continue };
        for (person, _) in assigned.get(&t.id).into_iter().flatten() {
            by_person
                .entry(*person)
                .or_default()
                .entry(step)
                .or_default()
                .push(t.id);
        }
    }
    by_person
        .into_iter()
        .map(|(person, steps)| {
            let steps = steps
                .into_values()
                .map(|mut ids| {
                    ids.sort();
                    ids
                })
                .collect();
            (person, steps)
        })
        .collect()
}

/// The "comes after" pairs of steps (earlier, later): every task of a step after every task of
/// the step before it. A later task already in progress is left out.
fn pairs_of(steps: &[Vec<Uuid>], started: &HashSet<Uuid>) -> Vec<(Uuid, Uuid)> {
    let mut out = Vec::new();
    for w in steps.windows(2) {
        for &a in &w[0] {
            for &b in w[1].iter().filter(|b| !started.contains(b)) {
                if a != b {
                    out.push((a, b));
                }
            }
        }
    }
    out
}

fn started(tasks: &[Task]) -> HashSet<Uuid> {
    tasks
        .iter()
        .filter(|t| t.status == TaskStatus::InProgress)
        .map(|t| t.id)
        .collect()
}

/// Every person's order as "comes after" pairs (earlier, later), sorted and without repeats. The
/// schedule adds them to the `blocks` graph (each only if it closes no loop).
pub fn links(tasks: &[Task], edges: &[Edge]) -> Vec<(Uuid, Uuid)> {
    let started = started(tasks);
    let mut out: Vec<(Uuid, Uuid)> = steps_by_person(tasks, edges)
        .values()
        .flat_map(|steps| pairs_of(steps, &started))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether one task waits for another through `blocks` links and the `pairs` of an order,
/// directly or further back: `reaches(a, b)` is true when `b` can only start after `a`.
fn reach(tasks: &[Task], edges: &[Edge], pairs: &[(Uuid, Uuid)]) -> impl Fn(Uuid, Uuid) -> bool {
    let live: HashSet<Uuid> = tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Cancelled && t.archived_at.is_none())
        .map(|t| t.id)
        .collect();
    let mut next: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks && e.archived_at.is_none())
        .filter(|e| live.contains(&e.from_id) && live.contains(&e.to_id))
    {
        next.entry(e.from_id).or_default().push(e.to_id);
    }
    for (a, b) in pairs {
        next.entry(*a).or_default().push(*b);
    }
    move |from: Uuid, to: Uuid| {
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
}

/// Why a pair of the order can't hold: the earlier task already waits for the later one.
fn conflict_text(titles: &HashMap<Uuid, &str>, earlier: Uuid, later: Uuid) -> String {
    let name = |id: &Uuid| titles.get(id).copied().unwrap_or("?");
    format!(
        "“{}” has to be done before “{}” (blocks links say so), so it can't come after it.",
        name(&later),
        name(&earlier)
    )
}

/// Whether `steps` can be `person`'s order: every task is open, assigned to them, not a meeting
/// or on the backburner, listed once, and no step comes after a task it has to be done before.
pub fn check(
    tasks: &[Task],
    edges: &[Edge],
    person: Uuid,
    person_name: &str,
    steps: &[Vec<Uuid>],
) -> Result<(), String> {
    let by_id: HashMap<Uuid, &Task> = tasks.iter().map(|t| (t.id, t)).collect();
    let assigned = assignments(edges);
    let mut seen: HashSet<Uuid> = HashSet::new();
    for id in steps.iter().flatten() {
        let Some(t) = by_id.get(id) else {
            return Err("One of the tasks no longer exists. Reload and try again.".into());
        };
        if !seen.insert(*id) {
            return Err(format!("“{}” is in the order twice.", t.title));
        }
        let why = if t.archived_at.is_some() {
            Some("is archived")
        } else if matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled) {
            Some("is finished")
        } else if t.is_meeting() {
            Some("is a meeting, which happens at its own time")
        } else if t.is_parked() {
            Some("is on the backburner")
        } else if !assigned
            .get(id)
            .is_some_and(|a| a.iter().any(|(p, _)| *p == person))
        {
            Some("isn't assigned to them")
        } else {
            None
        };
        if let Some(why) = why {
            return Err(format!(
                "“{}” can't be in {person_name}'s order: it {why}.",
                t.title
            ));
        }
    }
    let titles: HashMap<Uuid, &str> = tasks.iter().map(|t| (t.id, t.title.as_str())).collect();
    let steps: Vec<Vec<Uuid>> = steps.iter().filter(|s| !s.is_empty()).cloned().collect();
    let pairs = pairs_of(&steps, &started(tasks));
    // A loop through the blocks links and the order itself, not only one link against one pair.
    let reaches = reach(tasks, edges, &pairs);
    for &(a, b) in &pairs {
        if reaches(b, a) {
            return Err(conflict_text(&titles, a, b));
        }
    }
    Ok(())
}

/// The steps as given, without empty steps and with each task kept only where it first appears.
pub fn normalise(steps: Vec<Vec<Uuid>>) -> Vec<Vec<Uuid>> {
    let mut seen: HashSet<Uuid> = HashSet::new();
    steps
        .into_iter()
        .map(|s| {
            s.into_iter()
                .filter(|id| seen.insert(*id))
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// What the Order screen shows for `person`: their steps with the dates the schedule gives
/// them, their other open tasks, and the parts of the order a `blocks` link overrides.
pub fn view(
    person: NodeSummary,
    tasks: &[Task],
    edges: &[Edge],
    projects: &[Project],
    schedule: &Schedule,
) -> WorkOrder {
    let assigned = assignments(edges);
    let scheduled: HashMap<Uuid, _> = schedule.tasks.iter().map(|s| (s.id, s)).collect();
    let project_title: HashMap<Uuid, &str> =
        projects.iter().map(|p| (p.id, p.title.as_str())).collect();
    let pid = person.node.id;
    let mine = tasks_of(tasks, edges, pid);
    let card = |t: &Task| {
        let s = scheduled.get(&t.id);
        let finish = s.map(|s| s.finish);
        OrderCard {
            id: t.id,
            title: t.title.clone(),
            project: t.project_id.map(|p| NodeSummary {
                node: NodeRef::new(NodeType::Project, p),
                label: project_title.get(&p).copied().unwrap_or("?").to_owned(),
                archived: false,
            }),
            status: t.status,
            priority: t.priority,
            task_type: t.task_type.clone(),
            estimate_days: t.estimate_days,
            start: s.map(|s| s.start),
            finish,
            due: t.due_date,
            past_due: matches!((finish, t.due_date), (Some(f), Some(d)) if f > d),
            started: t.status == TaskStatus::InProgress,
            allocation_pct: assigned
                .get(&t.id)
                .and_then(|a| a.iter().find(|(p, _)| *p == pid))
                .map_or(100, |(_, pct)| *pct),
        }
    };
    let by_title = |a: &OrderCard, b: &OrderCard| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.id.cmp(&b.id))
    };
    let mut grouped: BTreeMap<u32, Vec<OrderCard>> = BTreeMap::new();
    let mut unordered: Vec<OrderCard> = Vec::new();
    for t in &mine {
        match t.order_step {
            Some(step) => grouped.entry(step).or_default().push(card(t)),
            None => unordered.push(card(t)),
        }
    }
    let steps: Vec<OrderStep> = grouped
        .into_values()
        .enumerate()
        .map(|(i, mut cards)| {
            cards.sort_by(by_title);
            OrderStep {
                step: i as u32 + 1,
                start: cards.iter().filter_map(|c| c.start).min(),
                finish: cards.iter().filter_map(|c| c.finish).max(),
                load_pct: cards.iter().map(|c| c.allocation_pct).sum(),
                cards,
            }
        })
        .collect();
    unordered.sort_by(|a, b| {
        (a.due.is_none(), a.due, a.priority)
            .cmp(&(b.due.is_none(), b.due, b.priority))
            .then_with(|| by_title(a, b))
    });
    let ids: Vec<Vec<Uuid>> = steps
        .iter()
        .map(|s| s.cards.iter().map(|c| c.id).collect())
        .collect();
    let titles: HashMap<Uuid, &str> = tasks.iter().map(|t| (t.id, t.title.as_str())).collect();
    let pairs = pairs_of(&ids, &started(tasks));
    let reaches = reach(tasks, edges, &pairs);
    let conflicts = pairs
        .into_iter()
        .filter(|(a, b)| reaches(*b, *a))
        .map(|(a, b)| conflict_text(&titles, a, b))
        .collect();
    WorkOrder {
        person,
        steps,
        unordered,
        conflicts,
        ruler: Vec::new(),
        today: None,
    }
}

/// How many working days the Order screen's ruler shows (spec 61).
pub const RULER_DAYS: usize = 15;
/// The most working days the ruler grows to, to reach the last step's end (about half a year).
pub const RULER_MAX: usize = 130;
/// How far back the ruler may start, in working days, to show a task already under way.
pub const RULER_BACK: usize = 5;

/// The working days the Order screen draws `order`'s bars on: at least [`RULER_DAYS`] of them,
/// starting at the earlier of today and the first start in the order, but never more than
/// [`RULER_BACK`] working days before today, and growing (up to [`RULER_MAX`]) until the last
/// step's end is on it, so every bar can be reached by scrolling. A day off at either end moves
/// to the next working day.
pub fn ruler(order: &WorkOrder, today: Date, week: WorkWeek) -> Vec<Date> {
    if week.days_per_week() == 0 {
        return Vec::new();
    }
    let working = |d: Date| week.contains_weekday(d.weekday());
    let mut earliest = today;
    for _ in 0..RULER_BACK {
        let mut d = earliest;
        while let Some(p) = d.previous_day() {
            d = p;
            if working(d) {
                break;
            }
        }
        earliest = d;
    }
    let first = order
        .steps
        .iter()
        .flat_map(|s| s.cards.iter().filter_map(|c| c.start))
        .min()
        .unwrap_or(today);
    let last = order
        .steps
        .iter()
        .flat_map(|s| s.cards.iter().filter_map(|c| c.finish))
        .max();
    let mut day = first.clamp(earliest, today);
    let mut out = Vec::with_capacity(RULER_DAYS);
    // A week with no working day would never fill the ruler.
    for _ in 0..RULER_MAX * 7 {
        let reaches_last = last.is_none_or(|l| out.last().is_some_and(|d| *d >= l));
        if out.len() >= RULER_MAX || (out.len() >= RULER_DAYS && reaches_last) {
            break;
        }
        if working(day) {
            out.push(day);
        }
        match day.next_day() {
            Some(n) => day = n,
            None => break,
        }
    }
    out
}

/// What the order writes on one task: a `Some` field is a date to set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderDates {
    pub id: Uuid,
    pub start_date: Option<Date>,
    pub due_date: Option<Date>,
}

/// The dates the order writes (spec 60, ADR-0024 as amended).
///
/// **Start**: for each task that comes after another in someone's order, the day the schedule
/// starts it, when that is later than the start date it has (or it has none). A task above
/// finishes, and its follower starts on the next working day; when the work above slips, every
/// date below moves down with it. A date is never moved earlier, so a start date the person typed
/// (or dragged to) that is later stays. Tasks in step 1, started tasks and tasks outside any
/// order get no start.
///
/// **Due**: an open task in an order that has an estimate and no due date gets the last day the
/// schedule works on it (start and estimate). A due date that is set is a promise and is never
/// touched.
pub fn dates(
    tasks: &[Task],
    edges: &[Edge],
    projects: &[Project],
    today: Date,
    week: WorkWeek,
) -> Result<Vec<OrderDates>, crate::schedule::ScheduleError> {
    let after: HashSet<Uuid> = links(tasks, edges).into_iter().map(|(_, b)| b).collect();
    let ordered: HashSet<Uuid> = steps_by_person(tasks, edges)
        .into_values()
        .flatten()
        .flatten()
        .collect();
    if ordered.is_empty() {
        return Ok(Vec::new());
    }
    let schedule = crate::schedule::compute(
        tasks,
        edges,
        projects,
        today,
        week,
        ScheduleScope::Portfolio,
    )?;
    let by_id: HashMap<Uuid, &Task> = tasks.iter().map(|t| (t.id, t)).collect();
    let mut out: Vec<OrderDates> = schedule
        .tasks
        .iter()
        .filter(|s| ordered.contains(&s.id) && !s.done)
        .filter_map(|s| {
            let t = by_id.get(&s.id)?;
            let start_date = (after.contains(&s.id) && t.start_date.is_none_or(|d| d < s.start))
                .then_some(s.start);
            let due_date = (t.due_date.is_none() && t.estimate_days.is_some()).then_some(s.finish);
            (start_date.is_some() || due_date.is_some()).then_some(OrderDates {
                id: s.id,
                start_date,
                due_date,
            })
        })
        .collect();
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    pub(crate) fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    pub(crate) fn task(n: u128, title: &str, step: Option<u32>) -> Task {
        Task {
            confirmed_on: None,
            order_step: step,
            links: Vec::new(),
            task_type: Some("build".into()),
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: None,
            status: TaskStatus::Todo,
            estimate_days: Some(2.0),
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

    pub(crate) fn edge(kind: EdgeType, from: u128, to: u128) -> Edge {
        Edge {
            id: Uuid::from_u128(50_000 + from * 1000 + to),
            edge_type: kind,
            from_type: NodeType::Task,
            from_id: id(from),
            to_type: if kind == EdgeType::AssignedTo {
                NodeType::Person
            } else {
                NodeType::Task
            },
            to_id: id(to),
            attrs: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    const P: u128 = 900;
    const Q: u128 = 901;

    fn assign(task: u128, person: u128) -> Edge {
        edge(EdgeType::AssignedTo, task, person)
    }

    #[test]
    fn each_step_comes_after_the_whole_step_before_it() {
        let tasks = vec![
            task(1, "A", Some(1)),
            task(2, "B", Some(1)),
            task(3, "C", Some(2)),
            task(4, "D", Some(5)),
            task(5, "E", None),
        ];
        let edges: Vec<Edge> = (1..=5).map(|n| assign(n, P)).collect();
        // Steps 1, 2 and 5 are consecutive in the order: the gap in the numbers doesn't matter.
        assert_eq!(
            links(&tasks, &edges),
            [(id(1), id(3)), (id(2), id(3)), (id(3), id(4))]
        );
    }

    #[test]
    fn what_the_order_ignores() {
        let mut tasks = vec![
            task(1, "A", Some(1)),
            task(2, "B", Some(2)),
            task(3, "C", Some(3)),
            task(4, "D", Some(4)),
        ];
        let edges: Vec<Edge> = (1..=4).map(|n| assign(n, P)).collect();
        // A started task keeps its start: nothing is put before it.
        tasks[1].status = TaskStatus::InProgress;
        assert_eq!(links(&tasks, &edges), [(id(2), id(3)), (id(3), id(4))]);
        // Finished, parked and meeting tasks drop out, and the steps close up around them.
        tasks[1].status = TaskStatus::Done;
        tasks[2].backburner_since = Some(time::macros::date!(2027 - 03 - 01));
        assert_eq!(links(&tasks, &edges), [(id(1), id(4))]);
        tasks[3].task_type = Some(minimap_types::MEETING_TYPE.into());
        assert!(links(&tasks, &edges).is_empty());
        // An unassigned task is in nobody's order.
        let alone = vec![task(1, "A", Some(1)), task(2, "B", Some(2))];
        assert!(links(&alone, &[]).is_empty());
    }

    #[test]
    fn each_person_has_their_own_order() {
        let tasks = vec![
            task(1, "A", Some(1)),
            task(2, "B", Some(2)),
            task(3, "C", Some(1)),
            task(4, "D", Some(2)),
        ];
        let edges = vec![assign(1, P), assign(2, P), assign(3, Q), assign(4, Q)];
        assert_eq!(links(&tasks, &edges), [(id(1), id(2)), (id(3), id(4))]);
    }

    #[test]
    fn an_order_against_blocks_or_with_a_task_that_cant_be_in_it_is_refused() {
        let mut tasks = vec![
            task(1, "Design", None),
            task(2, "Build", None),
            task(3, "Other", None),
        ];
        let edges = vec![assign(1, P), assign(2, P), edge(EdgeType::Blocks, 1, 2)];
        let ok = check(&tasks, &edges, id(P), "Vinay", &[vec![id(1)], vec![id(2)]]);
        assert!(ok.is_ok());
        let err = check(&tasks, &edges, id(P), "Vinay", &[vec![id(2)], vec![id(1)]]).unwrap_err();
        assert!(
            err.contains("“Design” has to be done before “Build”"),
            "{err}"
        );
        // Also through a step in between: Build, then Other, then Design.
        let mut three = tasks.clone();
        three.push(task(4, "Middle", None));
        let mut more = edges.clone();
        more.push(assign(4, P));
        let err = check(
            &three,
            &more,
            id(P),
            "Vinay",
            &[vec![id(2)], vec![id(4)], vec![id(1)]],
        )
        .unwrap_err();
        assert!(err.contains("can't come after"), "{err}");
        // Side by side is allowed: the blocks link still orders them.
        assert!(check(&tasks, &edges, id(P), "Vinay", &[vec![id(1), id(2)]]).is_ok());
        let err = check(&tasks, &edges, id(P), "Vinay", &[vec![id(3)]]).unwrap_err();
        assert!(err.contains("isn't assigned to them"), "{err}");
        let err = check(&tasks, &edges, id(P), "Vinay", &[vec![id(1)], vec![id(1)]]).unwrap_err();
        assert!(err.contains("twice"), "{err}");
        tasks[0].status = TaskStatus::Done;
        let err = check(&tasks, &edges, id(P), "Vinay", &[vec![id(1)]]).unwrap_err();
        assert!(err.contains("is finished"), "{err}");
    }

    #[test]
    fn empty_steps_and_repeats_are_dropped() {
        let steps = normalise(vec![vec![], vec![id(1), id(2)], vec![id(2)], vec![id(3)]]);
        assert_eq!(steps, [vec![id(1), id(2)], vec![id(3)]]);
    }

    type Written = Vec<(Uuid, Date)>;

    fn written(got: &[OrderDates]) -> (Written, Written) {
        let pick = |f: fn(&OrderDates) -> Option<Date>| {
            got.iter().filter_map(|d| f(d).map(|x| (d.id, x))).collect()
        };
        (pick(|d| d.start_date), pick(|d| d.due_date))
    }

    fn apply(tasks: &mut [Task], got: &[OrderDates]) {
        for d in got {
            let t = tasks.iter_mut().find(|t| t.id == d.id).unwrap();
            t.start_date = d.start_date.or(t.start_date);
            t.due_date = d.due_date.or(t.due_date);
        }
    }

    #[test]
    fn a_task_below_starts_the_working_day_after_the_one_above_and_a_slip_moves_it() {
        use time::macros::date;
        // A Wednesday; A takes 2 days (Wed, Thu), so B starts on Friday, and C (3 days) after B.
        let today = date!(2027 - 03 - 03);
        let mut tasks = vec![
            task(1, "A", Some(1)),
            task(2, "B", Some(2)),
            task(3, "C", Some(3)),
        ];
        tasks[2].estimate_days = Some(3.0);
        let edges: Vec<Edge> = (1..=3).map(|n| assign(n, P)).collect();
        let week = WorkWeek::MON_FRI;
        let got = dates(&tasks, &edges, &[], today, week).unwrap();
        let (starts, dues) = written(&got);
        // B works Fri and Mon, so C starts on Tuesday. Step 1 gets no start date.
        assert_eq!(
            starts,
            [
                (id(2), date!(2027 - 03 - 05)),
                (id(3), date!(2027 - 03 - 09))
            ]
        );
        // No due dates were set, so each gets its last working day from start and estimate.
        assert_eq!(
            dues,
            [
                (id(1), date!(2027 - 03 - 04)),
                (id(2), date!(2027 - 03 - 08)),
                (id(3), date!(2027 - 03 - 11))
            ]
        );
        // Written, nothing more to write.
        apply(&mut tasks, &got);
        assert!(dates(&tasks, &edges, &[], today, week).unwrap().is_empty());
        // A slips a day (3 days): B and C start one working day later. Their due dates are
        // promises and stay.
        tasks[0].estimate_days = Some(3.0);
        let got = dates(&tasks, &edges, &[], today, week).unwrap();
        let (starts, dues) = written(&got);
        assert_eq!(
            starts,
            [
                (id(2), date!(2027 - 03 - 08)),
                (id(3), date!(2027 - 03 - 10))
            ]
        );
        assert!(dues.is_empty(), "{dues:?}");
        // A later date the person chose stays; an earlier-finishing A never pulls B up.
        apply(&mut tasks, &got);
        tasks[1].start_date = Some(date!(2027 - 03 - 15));
        tasks[0].estimate_days = Some(1.0);
        let got = dates(&tasks, &edges, &[], today, week).unwrap();
        assert!(got.iter().all(|d| d.id != id(2)), "{got:?}");
        // No estimate: no due date to work out. A started task is not given a start.
        tasks[1].start_date = None;
        tasks[1].status = TaskStatus::InProgress;
        tasks[1].due_date = None;
        tasks[1].estimate_days = None;
        let got = dates(&tasks, &edges, &[], today, week).unwrap();
        assert!(got.iter().all(|d| d.id != id(2)), "{got:?}");
    }

    fn ordered(starts: &[Date]) -> WorkOrder {
        let card = |n: usize, start: Date| OrderCard {
            id: id(n as u128),
            title: format!("T{n}"),
            project: None,
            status: TaskStatus::Todo,
            priority: 3,
            task_type: None,
            estimate_days: Some(1.0),
            start: Some(start),
            finish: Some(start),
            due: None,
            past_due: false,
            started: false,
            allocation_pct: 100,
        };
        WorkOrder {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, id(P)),
                label: "P".into(),
                archived: false,
            },
            steps: starts
                .iter()
                .enumerate()
                .map(|(i, d)| OrderStep {
                    step: i as u32 + 1,
                    start: Some(*d),
                    finish: Some(*d),
                    load_pct: 100,
                    cards: vec![card(i, *d)],
                })
                .collect(),
            unordered: Vec::new(),
            conflicts: Vec::new(),
            ruler: Vec::new(),
            today: None,
        }
    }

    #[test]
    fn the_ruler_shows_fifteen_working_days_from_today_or_a_little_before() {
        use time::macros::date;
        let wed = date!(2027 - 03 - 03);
        let week = WorkWeek::MON_FRI;
        // Nothing started earlier: from today, weekends left out.
        let r = ruler(&ordered(&[date!(2027 - 03 - 10)]), wed, week);
        assert_eq!(r.len(), RULER_DAYS);
        assert_eq!(r[0], wed);
        assert!(r.iter().all(|d| week.contains_weekday(d.weekday())));
        assert_eq!(r[RULER_DAYS - 1], date!(2027 - 03 - 23));
        // A task under way since last Friday: the ruler starts there.
        let r = ruler(&ordered(&[date!(2027 - 02 - 26)]), wed, week);
        assert_eq!(r[0], date!(2027 - 02 - 26));
        // Started a month ago: no more than five working days back.
        let r = ruler(&ordered(&[date!(2027 - 02 - 01)]), wed, week);
        assert_eq!(r[0], date!(2027 - 02 - 24));
        // Today a Saturday: the next working day.
        let r = ruler(&ordered(&[]), date!(2027 - 03 - 06), week);
        assert_eq!(r[0], date!(2027 - 03 - 08));
    }

    #[test]
    fn the_ruler_grows_to_reach_the_last_end_but_not_without_limit() {
        use time::macros::date;
        let wed = date!(2027 - 03 - 03);
        let week = WorkWeek::MON_FRI;
        // A step ending in the week of 2027-05-03 (a Monday): the ruler ends on that day.
        let r = ruler(
            &ordered(&[date!(2027 - 03 - 10), date!(2027 - 05 - 03)]),
            wed,
            week,
        );
        assert!(r.len() > RULER_DAYS);
        assert_eq!(r.last(), Some(&date!(2027 - 05 - 03)));
        // Years away: stopped at the limit.
        let r = ruler(&ordered(&[date!(2030 - 01 - 07)]), wed, week);
        assert_eq!(r.len(), RULER_MAX);
    }
}
