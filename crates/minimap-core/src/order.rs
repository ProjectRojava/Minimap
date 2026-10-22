//! The order of work (spec 60, ADR-0024). Pure.
//!
//! Each task carries `order_step`, its step in its assignee's order: step 1 is done first, tasks
//! on the same step at the same time. The schedule reads the order as "comes after" pairs
//! ([`links`]): every task of a step comes after every task of the nearest earlier step. Only
//! open, assigned, unparked tasks that are not meetings count; a pair whose later task is already
//! in progress is left out (it has started, and its start is a fact).

use std::collections::{BTreeMap, HashMap, HashSet};

use minimap_types::{
    Edge, EdgeType, NodeRef, NodeSummary, NodeType, OrderCard, OrderStep, Project, Schedule, Task,
    TaskStatus, Uuid, WorkOrder,
};

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
    }
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
}
