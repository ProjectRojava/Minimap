//! Your people (docs/product-design/06-your-people.md, spec 49): compact frames for you and each
//! of your direct reports. Pure.
//!
//! This is about support, never performance. A frame shows how loaded someone is this week, what
//! you wait on them for, what of theirs is blocked or overdue, and how long since your last 1:1.
//! No scores, no ranking, no health for people: the order is **by name**, never by load, and the
//! flags are facts.

use std::collections::{HashMap, HashSet};

use minimap_types::Uuid;
use minimap_types::{
    Capacity, Edge, EdgeType, Frame, FrameFlag, FramesHint, NodeSummary, NodeType, NoteItem,
    NoteKind, PeopleFrames, Person, ReportGroup, Task, TaskStatus, WaitingOn,
};
use time::Date;

use crate::waiting_on;

/// Everything `build` reads. All slices may be in any order: the result does not depend on it.
pub struct FramesInput<'a> {
    pub today: Date,
    pub people: &'a [Person],
    /// Active links (`reports_to`, `assigned_to`; others are ignored).
    pub edges: &'a [Edge],
    /// The tasks the capacity module sees (everything but meetings).
    pub tasks: &'a [Task],
    pub waiting: &'a [WaitingOn],
    /// Notes with what they mention (for the 1:1s).
    pub notes: &'a [NoteItem],
    /// The capacity window containing this week, as `get_capacity` computes it.
    pub capacity: &'a Capacity,
    pub stale_waiting_days: u32,
    pub one_on_one_every_days: u32,
    /// Also everyone further down the reporting tree, not only your direct reports.
    pub all_levels: bool,
}

fn active_edges<'a>(edges: &'a [Edge], edge_type: EdgeType) -> impl Iterator<Item = &'a Edge> + 'a {
    edges.iter().filter(move |e| {
        e.edge_type == edge_type
            && e.archived_at.is_none()
            && e.from_type == NodeType::Person
            && e.to_type == NodeType::Person
    })
}

/// The people a 1:1 note was with. A 1:1 mentions who it is with, but its notes also mention
/// others they talked about. With one person mentioned, it is with them; with several, it is with
/// those whose name is in the title ("1:1 with Priya"), or with all of them when none is.
fn one_on_one_participants(item: &NoteItem) -> Vec<Uuid> {
    let people: Vec<&NodeSummary> = item
        .mentions
        .iter()
        .filter(|m| m.node.node_type == NodeType::Person)
        .collect();
    if people.len() <= 1 {
        return people.iter().map(|m| m.node.id).collect();
    }
    let words = |text: &str| -> HashSet<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 2)
            .map(str::to_lowercase)
            .collect()
    };
    let title = words(&item.note.title);
    let named: Vec<Uuid> = people
        .iter()
        .filter(|m| words(&m.label).iter().any(|w| title.contains(w)))
        .map(|m| m.node.id)
        .collect();
    if named.is_empty() {
        people.iter().map(|m| m.node.id).collect()
    } else {
        named
    }
}

/// By name (ignoring case), then id: never by load.
fn by_name(a: &Person, b: &Person) -> std::cmp::Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.id.cmp(&b.id))
}

/// The flags of a frame, in their fixed order.
pub fn flags(frame: &Frame) -> Vec<FrameFlag> {
    let mut out = Vec::new();
    if frame.over {
        out.push(FrameFlag::Over);
    }
    if frame.many_tasks {
        out.push(FrameFlag::ManyTasks);
    }
    if let Some(days) = frame.one_on_one_due {
        out.push(FrameFlag::OneOnOneDue { days });
    }
    if frame.waiting_stale > 0 {
        out.push(FrameFlag::WaitingStale {
            count: frame.waiting_stale,
        });
    }
    if frame.blocked_tasks > 0 {
        out.push(FrameFlag::Blocked {
            count: frame.blocked_tasks,
        });
    }
    if frame.overdue_tasks > 0 {
        out.push(FrameFlag::Overdue {
            count: frame.overdue_tasks,
        });
    }
    out
}

struct Facts<'a> {
    input: &'a FramesInput<'a>,
    /// Open tasks by assignee.
    tasks: HashMap<Uuid, Vec<&'a Task>>,
    /// Newest 1:1 (on or before today) by person mentioned.
    last_one_on_one: HashMap<Uuid, Date>,
}

fn is_open(t: &Task) -> bool {
    t.archived_at.is_none() && !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
}

impl<'a> Facts<'a> {
    fn new(input: &'a FramesInput<'a>) -> Self {
        let by_id: HashMap<Uuid, &Task> = input.tasks.iter().map(|t| (t.id, t)).collect();
        let mut tasks: HashMap<Uuid, Vec<&Task>> = HashMap::new();
        for e in input.edges.iter().filter(|e| {
            e.edge_type == EdgeType::AssignedTo
                && e.archived_at.is_none()
                && e.from_type == NodeType::Task
        }) {
            if let Some(t) = by_id.get(&e.from_id).filter(|t| is_open(t)) {
                tasks.entry(e.to_id).or_default().push(t);
            }
        }
        let mut last_one_on_one: HashMap<Uuid, Date> = HashMap::new();
        for item in input.notes {
            let n = &item.note;
            // A 1:1 dated in the future has not happened yet.
            if n.archived_at.is_some() || n.kind != NoteKind::OneOnOne || n.note_date > input.today
            {
                continue;
            }
            for who in one_on_one_participants(item) {
                let slot = last_one_on_one.entry(who).or_insert(n.note_date);
                if n.note_date > *slot {
                    *slot = n.note_date;
                }
            }
        }
        Self {
            input,
            tasks,
            last_one_on_one,
        }
    }

    fn frame(&self, p: &Person) -> Frame {
        let input = self.input;
        let mine = self.tasks.get(&p.id).map_or(&[][..], Vec::as_slice);
        let open_tasks = mine.len() as u32;
        let blocked_tasks = mine
            .iter()
            .filter(|t| t.status == TaskStatus::Blocked)
            .count() as u32;
        let overdue_tasks = mine
            .iter()
            .filter(|t| t.due_date.is_some_and(|d| d < input.today))
            .count() as u32;

        let capacity = input
            .capacity
            .people
            .iter()
            .find(|c| c.person.node.id == p.id);
        let this_week = capacity.and_then(|c| {
            c.weeks
                .iter()
                .find(|w| w.week_start == input.capacity.current_week_start)
        });
        let load = this_week.filter(|w| w.capacity_days > 0.0);
        let load_pct = load.map(|w| w.load_pct.round().max(0.0) as u32);
        let over = load.is_some_and(|w| w.over);
        let many_tasks = capacity.is_some_and(|c| c.over_task_limit);

        // What you wait on *them* for: never about yourself.
        let (waiting_on_them, waiting_stale) = if p.is_self {
            (0, 0)
        } else {
            let theirs = input.waiting.iter().filter(|w| {
                w.person_id == p.id
                    && w.archived_at.is_none()
                    && waiting_on::is_open(w)
                    && !waiting_on::is_snoozed(w, input.today)
            });
            let (mut all, mut stale) = (0, 0);
            for w in theirs {
                all += 1;
                if waiting_on::is_stale(w, input.today, input.stale_waiting_days) {
                    stale += 1;
                }
            }
            (all, stale)
        };

        let last_one_on_one = if p.is_self {
            None
        } else {
            self.last_one_on_one.get(&p.id).copied()
        };
        // Days since the last 1:1 (or since they were added, if there has never been one), once
        // that is more than the rhythm.
        let one_on_one_due = if p.is_self {
            None
        } else {
            let since = last_one_on_one.unwrap_or_else(|| p.created_at.date());
            let days = (input.today - since).whole_days().max(0);
            (days > i64::from(input.one_on_one_every_days)).then_some(days as u32)
        };

        let mut frame = Frame {
            person: NodeSummary {
                node: minimap_types::NodeRef::new(NodeType::Person, p.id),
                label: p.name.clone(),
                archived: false,
            },
            role_title: p.role_title.clone(),
            is_self: p.is_self,
            load_pct,
            over,
            many_tasks,
            open_tasks,
            blocked_tasks,
            overdue_tasks,
            waiting_on_them,
            waiting_stale,
            last_one_on_one,
            one_on_one_due,
            flags: Vec::new(),
        };
        frame.flags = flags(&frame);
        frame
    }
}

/// The frames for you and your direct reports; `None` when there is no "me" yet.
pub fn build(input: &FramesInput) -> Option<PeopleFrames> {
    let active: Vec<&Person> = input
        .people
        .iter()
        .filter(|p| p.archived_at.is_none())
        .collect();
    let me = active.iter().find(|p| p.is_self)?;
    let facts = Facts::new(input);

    let managed_by = |manager: Uuid| -> Vec<&Person> {
        let ids: HashSet<Uuid> = active_edges(input.edges, EdgeType::ReportsTo)
            .filter(|e| e.to_id == manager)
            .map(|e| e.from_id)
            .collect();
        let mut people: Vec<&Person> = active
            .iter()
            .copied()
            .filter(|p| ids.contains(&p.id) && p.id != manager)
            .collect();
        people.sort_by(|a, b| by_name(a, b));
        people
    };

    let direct = managed_by(me.id);
    let reports: Vec<Frame> = direct.iter().map(|p| facts.frame(p)).collect();
    // Everyone further down, depth first: each manager's group is followed by the groups of
    // the people in it. A person is shown once, under the first manager that reaches them.
    let mut below: Vec<ReportGroup> = Vec::new();
    if input.all_levels {
        let mut shown: HashSet<Uuid> = direct.iter().map(|p| p.id).chain([me.id]).collect();
        let mut stack: Vec<(Uuid, u32)> = direct.iter().rev().map(|p| (p.id, 1)).collect();
        while let Some((manager, depth)) = stack.pop() {
            let people: Vec<&Person> = managed_by(manager)
                .into_iter()
                .filter(|p| !shown.contains(&p.id))
                .collect();
            if people.is_empty() {
                continue;
            }
            shown.extend(people.iter().map(|p| p.id));
            below.push(ReportGroup {
                manager,
                depth,
                frames: people.iter().map(|p| facts.frame(p)).collect(),
            });
            stack.extend(people.iter().rev().map(|p| (p.id, depth + 1)));
        }
    }
    Some(PeopleFrames {
        me: facts.frame(me),
        hint: reports.is_empty().then_some(FramesHint::NoReports),
        reports,
        below,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::{compute, CapacityInput};
    use minimap_types::{NodeRef, Note, WorkWeek};
    use proptest::prelude::*;
    use time::{macros::date, Duration, OffsetDateTime, Time};

    // Wednesday 2027-03-03.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(d: Date) -> OffsetDateTime {
        OffsetDateTime::new_utc(d, Time::MIDNIGHT)
    }

    fn person(n: u128, name: &str, me: bool, hours: f64, added: Date) -> Person {
        Person {
            id: id(n),
            name: name.into(),
            role_title: format!("{name}'s role"),
            email: None,
            weekly_capacity_hours: hours,
            is_self: me,
            notes: String::new(),
            created_at: at(added),
            updated_at: at(added),
            archived_at: None,
        }
    }

    fn edge(n: u128, edge_type: EdgeType, from: (NodeType, u128), to: (NodeType, u128)) -> Edge {
        Edge {
            id: id(n),
            edge_type,
            from_type: from.0,
            from_id: id(from.1),
            to_type: to.0,
            to_id: id(to.1),
            attrs: serde_json::json!({}),
            created_at: at(TODAY),
            archived_at: None,
        }
    }

    fn reports_to(n: u128, who: u128, boss: u128) -> Edge {
        edge(
            n,
            EdgeType::ReportsTo,
            (NodeType::Person, who),
            (NodeType::Person, boss),
        )
    }

    fn assigned(n: u128, task: u128, who: u128) -> Edge {
        edge(
            n,
            EdgeType::AssignedTo,
            (NodeType::Task, task),
            (NodeType::Person, who),
        )
    }

    fn task(n: u128, status: TaskStatus, estimate: Option<f64>, due: Option<Date>) -> Task {
        Task {
            id: id(n),
            title: format!("T{n}"),
            description: String::new(),
            project_id: None,
            status,
            estimate_days: estimate,
            start_date: None,
            due_date: due,
            completed_at: None,
            priority: 3,
            recurrence: None,
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            confirmed_on: None,
            created_at: at(TODAY),
            updated_at: at(TODAY),
            archived_at: None,
        }
    }

    fn waiting(n: u128, who: u128, asked: Date) -> WaitingOn {
        WaitingOn {
            id: id(n),
            description: format!("W{n}"),
            person_id: id(who),
            asked_on: asked,
            expected_by: None,
            follow_up_on: None,
            resolved_on: None,
            created_at: at(asked),
            updated_at: at(asked),
            archived_at: None,
        }
    }

    fn one_on_one(n: u128, on: Date, with: &[u128]) -> NoteItem {
        NoteItem {
            note: Note {
                id: id(n),
                title: "1:1".into(),
                body: String::new(),
                note_date: on,
                kind: NoteKind::OneOnOne,
                recurrence: None,
                created_at: at(on),
                updated_at: at(on),
                archived_at: None,
            },
            mentions: with
                .iter()
                .map(|w| NodeSummary {
                    node: NodeRef::new(NodeType::Person, id(*w)),
                    label: "x".into(),
                    archived: false,
                })
                .collect(),
        }
    }

    /// A small organisation: me (1) with Raj (2), Priya (3) and Anna (4) reporting to me, Maya
    /// (5) reporting to Priya, and Zed (6) reporting to nobody.
    struct World {
        people: Vec<Person>,
        edges: Vec<Edge>,
        tasks: Vec<Task>,
        waiting: Vec<WaitingOn>,
        notes: Vec<NoteItem>,
    }

    fn world() -> World {
        let long_ago = date!(2026 - 01 - 01);
        World {
            people: vec![
                person(1, "Me", true, 40.0, long_ago),
                person(2, "Raj", false, 40.0, long_ago),
                person(3, "Priya", false, 40.0, long_ago),
                person(4, "anna", false, 40.0, long_ago),
                person(5, "Maya", false, 40.0, long_ago),
                person(6, "Zed", false, 40.0, long_ago),
            ],
            edges: vec![
                reports_to(10, 2, 1),
                reports_to(11, 3, 1),
                reports_to(12, 4, 1),
                reports_to(13, 5, 3),
                // Raj has more scheduled this week than he has days.
                assigned(20, 100, 2),
                assigned(21, 101, 2),
                assigned(22, 102, 3),
                assigned(23, 103, 3),
                assigned(24, 104, 2),
            ],
            tasks: vec![
                task(100, TaskStatus::Todo, Some(8.0), None),
                task(
                    101,
                    TaskStatus::Blocked,
                    Some(1.0),
                    Some(date!(2027 - 03 - 01)),
                ),
                task(102, TaskStatus::InProgress, Some(1.0), None),
                task(
                    103,
                    TaskStatus::Done,
                    Some(1.0),
                    Some(date!(2027 - 02 - 01)),
                ),
                task(104, TaskStatus::InProgress, Some(3.0), None),
            ],
            waiting: vec![
                waiting(200, 3, TODAY - Duration::days(20)),
                waiting(201, 3, TODAY - Duration::days(1)),
                waiting(202, 2, TODAY - Duration::days(2)),
            ],
            notes: vec![
                one_on_one(300, TODAY - Duration::days(18), &[2]),
                one_on_one(301, TODAY - Duration::days(3), &[3]),
                one_on_one(302, TODAY - Duration::days(30), &[3]),
            ],
        }
    }

    fn input<'a>(w: &'a World, capacity: &'a Capacity) -> FramesInput<'a> {
        FramesInput {
            today: TODAY,
            people: &w.people,
            edges: &w.edges,
            tasks: &w.tasks,
            waiting: &w.waiting,
            notes: &w.notes,
            capacity,
            stale_waiting_days: 7,
            one_on_one_every_days: 14,
            all_levels: false,
        }
    }

    fn capacity_of(w: &World) -> Capacity {
        compute(&CapacityInput {
            tasks: &w.tasks,
            edges: &w.edges,
            projects: &[],
            people: &w.people,
            today: TODAY,
            hours_per_day: 8.0,
            work_week: WorkWeek::MON_FRI,
            from: None,
            to: None,
            weeks: Some(4),
            task_limit: 10,
        })
    }

    fn frames(w: &World, tweak: impl FnOnce(&mut FramesInput)) -> PeopleFrames {
        let capacity = capacity_of(w);
        let mut i = input(w, &capacity);
        tweak(&mut i);
        build(&i).expect("a me")
    }

    #[test]
    fn it_is_you_then_direct_reports_by_name_not_by_load() {
        let f = frames(&world(), |_| {});
        assert!(f.me.is_self);
        let names: Vec<&str> = f.reports.iter().map(|r| r.person.label.as_str()).collect();
        // Raj is the most loaded, but the order is alphabetical (ignoring case).
        assert_eq!(names, ["anna", "Priya", "Raj"]);
        assert!(f.below.is_empty(), "only when asked for");
        assert_eq!(f.hint, None);
    }

    #[test]
    fn archived_people_and_removed_links_are_ignored() {
        let mut w = world();
        w.people[2].archived_at = Some(at(TODAY)); // Priya
        w.edges[2].archived_at = Some(at(TODAY)); // anna's link
        let f = frames(&w, |_| {});
        let names: Vec<&str> = f.reports.iter().map(|r| r.person.label.as_str()).collect();
        assert_eq!(names, ["Raj"]);
    }

    #[test]
    fn reports_of_reports_are_shown_only_with_the_setting() {
        let f = frames(&world(), |i| i.all_levels = true);
        assert_eq!(f.below.len(), 1);
        assert_eq!(f.below[0].manager, id(3));
        assert_eq!(f.below[0].depth, 1);
        assert_eq!(f.below[0].frames[0].person.label, "Maya");
        // Zed reports to nobody, so is nowhere.
        let all: Vec<&str> = f
            .reports
            .iter()
            .chain(f.below.iter().flat_map(|s| s.frames.iter()))
            .map(|r| r.person.label.as_str())
            .collect();
        assert!(!all.contains(&"Zed"));
    }

    #[test]
    fn everyone_below_a_report_is_shown_however_deep_each_group_after_its_manager() {
        let mut w = world();
        // Maya (5) has Zoe (7) and Abe (8); Zoe has Kit (9). Three levels below Priya.
        let long_ago = date!(2026 - 01 - 01);
        for (n, name) in [(7, "Zoe"), (8, "Abe"), (9, "Kit")] {
            w.people.push(person(n, name, false, 40.0, long_ago));
        }
        w.edges.extend([
            reports_to(14, 7, 5),
            reports_to(15, 8, 5),
            reports_to(16, 9, 7),
        ]);
        let f = frames(&w, |i| i.all_levels = true);
        let groups: Vec<(u128, u32, Vec<&str>)> = f
            .below
            .iter()
            .map(|g| {
                (
                    g.manager.as_u128(),
                    g.depth,
                    g.frames.iter().map(|x| x.person.label.as_str()).collect(),
                )
            })
            .collect();
        // Priya's people (Maya), then Maya's (Abe, Zoe by name), then Zoe's (Kit): a manager's
        // people sit right after their group.
        assert_eq!(
            groups,
            [
                (3, 1, vec!["Maya"]),
                (5, 2, vec!["Abe", "Zoe"]),
                (7, 3, vec!["Kit"]),
            ]
        );
        // Without the setting only your direct reports are shown.
        assert!(frames(&w, |_| {}).below.is_empty());
    }

    #[test]
    fn a_person_who_reaches_you_two_ways_is_shown_once() {
        let mut w = world();
        // Maya (5) also reports to Raj (2): she is shown under the first manager only.
        w.edges.push(reports_to(17, 5, 2));
        let f = frames(&w, |i| i.all_levels = true);
        let shown: usize = f
            .below
            .iter()
            .flat_map(|g| g.frames.iter())
            .filter(|x| x.person.label == "Maya")
            .count();
        assert_eq!(shown, 1);
    }

    #[test]
    fn nobody_reporting_to_you_gives_just_you_and_a_hint() {
        let mut w = world();
        w.edges.retain(|e| e.edge_type != EdgeType::ReportsTo);
        let f = frames(&w, |_| {});
        assert!(f.reports.is_empty());
        assert_eq!(f.hint, Some(FramesHint::NoReports));
    }

    #[test]
    fn no_me_no_frames() {
        let mut w = world();
        w.people[0].is_self = false;
        let capacity = capacity_of(&w);
        assert!(build(&input(&w, &capacity)).is_none());
    }

    #[test]
    fn the_load_is_the_number_the_capacity_heatmap_shows_for_this_week() {
        let w = world();
        let capacity = capacity_of(&w);
        let f = build(&input(&w, &capacity)).expect("a me");
        for frame in f.reports.iter().chain([&f.me]) {
            let cell = capacity
                .people
                .iter()
                .find(|c| c.person.node == frame.person.node)
                .and_then(|c| {
                    c.weeks
                        .iter()
                        .find(|x| x.week_start == capacity.current_week_start)
                })
                .expect("a cell");
            assert_eq!(frame.load_pct, Some(cell.load_pct.round() as u32));
            assert_eq!(frame.over, cell.over);
        }
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert!(raj.over, "{:?}", raj.load_pct);
        assert!(raj.load_pct.unwrap() > 100);
        assert!(raj.flags.contains(&FrameFlag::Over));
    }

    #[test]
    fn someone_with_no_hours_has_no_load_to_show() {
        let mut w = world();
        w.people[1].weekly_capacity_hours = 0.0;
        let f = frames(&w, |_| {});
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert_eq!(raj.load_pct, None);
        assert!(!raj.over);
    }

    #[test]
    fn task_counts_only_count_open_assigned_work() {
        let f = frames(&world(), |_| {});
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert_eq!(
            (raj.open_tasks, raj.blocked_tasks, raj.overdue_tasks),
            (3, 1, 1)
        );
        let priya = f
            .reports
            .iter()
            .find(|r| r.person.label == "Priya")
            .unwrap();
        // The done task does not count.
        assert_eq!(
            (priya.open_tasks, priya.blocked_tasks, priya.overdue_tasks),
            (1, 0, 0)
        );
        // And it is the capacity module's own count of open tasks.
        let capacity = capacity_of(&world());
        for frame in &f.reports {
            let c = capacity
                .people
                .iter()
                .find(|c| c.person.node == frame.person.node)
                .unwrap();
            assert_eq!(frame.open_tasks, c.active_tasks, "{}", frame.person.label);
        }
    }

    #[test]
    fn waiting_counts_are_what_you_wait_on_them_for() {
        let mut w = world();
        let f = frames(&w, |_| {});
        let priya = f
            .reports
            .iter()
            .find(|r| r.person.label == "Priya")
            .unwrap();
        assert_eq!((priya.waiting_on_them, priya.waiting_stale), (2, 1));
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert_eq!((raj.waiting_on_them, raj.waiting_stale), (1, 0));
        // Resolved and snoozed ones are not counted.
        w.waiting[0].resolved_on = Some(TODAY);
        w.waiting[1].follow_up_on = Some(TODAY + Duration::days(3));
        let f = frames(&w, |_| {});
        let priya = f
            .reports
            .iter()
            .find(|r| r.person.label == "Priya")
            .unwrap();
        assert_eq!((priya.waiting_on_them, priya.waiting_stale), (0, 0));
        // It is never about you.
        w.waiting.push(waiting(203, 1, TODAY - Duration::days(30)));
        assert_eq!(frames(&w, |_| {}).me.waiting_on_them, 0);
    }

    #[test]
    fn a_1_1_is_due_only_past_the_rhythm() {
        let w = world();
        let f = frames(&w, |_| {});
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert_eq!(raj.last_one_on_one, Some(TODAY - Duration::days(18)));
        assert_eq!(raj.one_on_one_due, Some(18));
        assert!(raj.flags.contains(&FrameFlag::OneOnOneDue { days: 18 }));
        // The newest 1:1 counts, not the oldest.
        let priya = f
            .reports
            .iter()
            .find(|r| r.person.label == "Priya")
            .unwrap();
        assert_eq!(priya.last_one_on_one, Some(TODAY - Duration::days(3)));
        assert_eq!(priya.one_on_one_due, None);
        // The boundary: exactly the rhythm is not yet due, one day more is.
        let at_boundary = frames(&w, |i| i.one_on_one_every_days = 18);
        let raj = at_boundary
            .reports
            .iter()
            .find(|r| r.person.label == "Raj")
            .unwrap();
        assert_eq!(raj.one_on_one_due, None);
        let past = frames(&w, |i| i.one_on_one_every_days = 17);
        let raj = past
            .reports
            .iter()
            .find(|r| r.person.label == "Raj")
            .unwrap();
        assert_eq!(raj.one_on_one_due, Some(18));
    }

    #[test]
    fn someone_never_met_is_due_once_they_have_been_around_longer_than_the_rhythm() {
        let w = world();
        // anna has no 1:1 and was added long ago.
        let f = frames(&w, |_| {});
        let anna = f.reports.iter().find(|r| r.person.label == "anna").unwrap();
        assert_eq!(anna.last_one_on_one, None);
        assert!(anna.one_on_one_due.is_some_and(|d| d > 300));
        // Added last week: not due yet.
        let mut w = world();
        w.people[3].created_at = at(TODAY - Duration::days(7));
        let f = frames(&w, |_| {});
        let anna = f.reports.iter().find(|r| r.person.label == "anna").unwrap();
        assert_eq!(anna.one_on_one_due, None);
    }

    #[test]
    fn a_1_1_dated_in_the_future_has_not_happened_and_other_kinds_do_not_count() {
        let mut w = world();
        w.notes
            .push(one_on_one(310, TODAY + Duration::days(2), &[2]));
        let mut meeting = one_on_one(311, TODAY - Duration::days(1), &[2]);
        meeting.note.kind = NoteKind::Meeting;
        w.notes.push(meeting);
        let f = frames(&w, |_| {});
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert_eq!(raj.last_one_on_one, Some(TODAY - Duration::days(18)));
    }

    #[test]
    fn a_1_1_is_with_who_it_names_not_everyone_it_mentions() {
        let mut w = world();
        // "1:1 with Priya" that also talks about Raj and Maya.
        let mut note = one_on_one(320, TODAY - Duration::days(1), &[3, 2, 5]);
        note.note.title = "1:1 with Priya".into();
        for m in &mut note.mentions {
            m.label = match m.node.id.as_u128() {
                3 => "Priya Nair".into(),
                2 => "Raj".into(),
                _ => "Maya".into(),
            };
        }
        w.notes = vec![note];
        let f = frames(&w, |i| i.all_levels = true);
        let of = |name: &str| {
            f.reports
                .iter()
                .chain(f.below.iter().flat_map(|s| s.frames.iter()))
                .find(|r| r.person.label == name)
                .unwrap()
                .last_one_on_one
        };
        assert_eq!(of("Priya"), Some(TODAY - Duration::days(1)));
        assert_eq!(of("Raj"), None);
        assert_eq!(of("Maya"), None);
        // With no name in the title, everyone mentioned counts.
        w.notes[0].note.title = "Weekly sync".into();
        let f = frames(&w, |_| {});
        let raj = f.reports.iter().find(|r| r.person.label == "Raj").unwrap();
        assert_eq!(raj.last_one_on_one, Some(TODAY - Duration::days(1)));
    }

    #[test]
    fn you_have_no_1_1_and_no_waiting() {
        let f = frames(&world(), |_| {});
        assert_eq!(f.me.last_one_on_one, None);
        assert_eq!(f.me.one_on_one_due, None);
        assert!(f.me.flags.iter().all(|x| !matches!(
            x,
            FrameFlag::OneOnOneDue { .. } | FrameFlag::WaitingStale { .. }
        )));
    }

    #[test]
    fn flags_come_in_a_fixed_order_and_are_facts() {
        let frame = Frame {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, id(1)),
                label: "x".into(),
                archived: false,
            },
            role_title: String::new(),
            is_self: false,
            load_pct: Some(130),
            over: true,
            many_tasks: true,
            open_tasks: 12,
            blocked_tasks: 2,
            overdue_tasks: 3,
            waiting_on_them: 4,
            waiting_stale: 1,
            last_one_on_one: None,
            one_on_one_due: Some(20),
            flags: Vec::new(),
        };
        assert_eq!(
            flags(&frame),
            [
                FrameFlag::Over,
                FrameFlag::ManyTasks,
                FrameFlag::OneOnOneDue { days: 20 },
                FrameFlag::WaitingStale { count: 1 },
                FrameFlag::Blocked { count: 2 },
                FrameFlag::Overdue { count: 3 },
            ]
        );
        let calm = Frame {
            over: false,
            many_tasks: false,
            blocked_tasks: 0,
            overdue_tasks: 0,
            waiting_stale: 0,
            one_on_one_due: None,
            ..frame
        };
        assert!(flags(&calm).is_empty());
    }

    /// `items` in the order that sorting by `keys` (one per item, repeated if short) gives.
    fn shuffled<T: Clone>(items: &[T], keys: &[u32]) -> Vec<T> {
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|i| (keys[i % keys.len()], *i));
        order.into_iter().map(|i| items[i].clone()).collect()
    }

    proptest! {
        /// The frames are a function of the data, not of the order it came in.
        #[test]
        fn the_order_of_the_input_does_not_matter(
            keys in proptest::collection::vec(any::<u32>(), 12)
        ) {
            let w = world();
            let want = frames(&w, |i| i.all_levels = true);
            let shuffled = World {
                people: shuffled(&w.people, &keys),
                edges: shuffled(&w.edges, &keys[3..]),
                tasks: shuffled(&w.tasks, &keys[5..]),
                waiting: shuffled(&w.waiting, &keys[7..]),
                notes: shuffled(&w.notes, &keys[2..]),
            };
            let got = frames(&shuffled, |i| i.all_levels = true);
            prop_assert_eq!(got.me, want.me);
            prop_assert_eq!(got.reports, want.reports);
            prop_assert_eq!(got.below, want.below);
        }
    }
}
