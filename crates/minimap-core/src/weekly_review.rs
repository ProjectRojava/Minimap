//! The weekly review (spec 19). Pure. Weeks run Monday to Sunday.
//!
//! What happened during the week comes from the activity log (due dates and target dates moved
//! later, tasks that became blocked, projects marked done) and from the week's dates (tasks
//! completed, tasks that came due, decisions made, waiting-ons resolved). Capacity, stale
//! waiting-ons, health and risks are the state right now, taken from the overview.

use std::collections::{HashMap, HashSet};

use minimap_types::{
    Activity, Date, DecisionFilter, DecisionItem, DecisionStatus, NodeRef, NodeSummary, NodeType,
    Objective, ObjectiveStatus, PortfolioOverview, Project, ProjectStatus, ReviewBlocked,
    ReviewDone, ReviewFinished, ReviewSlip, SlipKind, TaskRow, TaskStatus, Uuid, WaitingOnFilter,
    WaitingOnItem, WeekTask, WeeklyReview, WorkWeek,
};
use time::Duration;

use crate::{decisions, schedule::end_index, this_week::monday_of, waiting_on};

pub struct ReviewInput {
    /// Every active task.
    pub tasks: Vec<TaskRow>,
    /// Open blockers of each task.
    pub blockers: HashMap<Uuid, Vec<NodeSummary>>,
    /// Activity from the review week (anything else is ignored), in any order.
    pub activity: Vec<Activity>,
    pub decisions: Vec<DecisionItem>,
    pub waiting: Vec<WaitingOnItem>,
    /// Active projects and objectives (names for the activity entries).
    pub projects: Vec<Project>,
    pub objectives: Vec<Objective>,
    pub overview: PortfolioOverview,
    pub today: Date,
    /// Any date in the week; today's week when `None`.
    pub week_of: Option<Date>,
    pub stale_days: u32,
    /// Slips are counted in working days of this work week.
    pub work_week: WorkWeek,
}

/// First and last day of the week containing `d` (Monday and Sunday).
pub fn week_of(d: Date) -> (Date, Date) {
    let start = monday_of(d);
    (start, start + Duration::days(6))
}

fn date_of_value(v: &serde_json::Value) -> Option<Date> {
    v.as_str()
        .and_then(|s| minimap_types::timefmt::parse_date(s).ok())
}

/// The `[old, new]` pair of one field of an activity entry.
fn change<'a>(
    a: &'a Activity,
    field: &str,
) -> Option<(&'a serde_json::Value, &'a serde_json::Value)> {
    let pair = a.diff.get(field)?.as_array()?;
    Some((pair.first()?, pair.get(1)?))
}

/// Per node, the first date it had this week and the last one it was moved to, from the entries
/// (oldest first) that changed `field` (a nullable date).
fn date_moves(
    entries: &[&Activity],
    node_type: NodeType,
    field: &str,
) -> HashMap<Uuid, (Date, Date)> {
    let mut moves: HashMap<Uuid, (Date, Date)> = HashMap::new();
    for a in entries.iter().filter(|a| a.node_type == node_type) {
        let Some((old, new)) = change(a, field) else {
            continue;
        };
        match (date_of_value(old), date_of_value(new)) {
            (Some(o), Some(n)) => {
                moves
                    .entry(a.node_id)
                    .and_modify(|m| m.1 = n)
                    .or_insert((o, n));
            }
            // Cleared: nothing slipped.
            (Some(_), None) => {
                moves.remove(&a.node_id);
            }
            // Set for the first time (or created): later moves are tracked from here.
            (None, Some(n)) => {
                moves.entry(a.node_id).or_insert((n, n));
            }
            (None, None) => {}
        }
    }
    moves
}

fn is_open(row: &TaskRow) -> bool {
    !matches!(row.task.status, TaskStatus::Done | TaskStatus::Cancelled)
}

pub fn build(input: ReviewInput) -> WeeklyReview {
    let today = input.today;
    let (week_start, week_end) = week_of(input.week_of.unwrap_or(today));

    let mut entries: Vec<&Activity> = input
        .activity
        .iter()
        .filter(|a| {
            let day = a.at.date();
            day >= week_start && day <= week_end
        })
        .collect();
    entries.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));

    let tasks: HashMap<Uuid, &TaskRow> = input.tasks.iter().map(|r| (r.task.id, r)).collect();
    let summary_of = |node_type: NodeType, id: Uuid, label: &str| NodeSummary {
        node: NodeRef::new(node_type, id),
        label: label.to_owned(),
        archived: false,
    };
    let task_summary = |r: &TaskRow| summary_of(NodeType::Task, r.task.id, &r.task.title);

    // ---- slipped
    let week = input.work_week;
    let mut slipped: Vec<ReviewSlip> = Vec::new();
    let mut slipped_tasks: HashSet<Uuid> = HashSet::new();
    for (id, (from, to)) in date_moves(&entries, NodeType::Task, "due_date") {
        let Some(row) = tasks.get(&id).filter(|r| is_open(r)) else {
            continue;
        };
        let days = (end_index(week, to) - end_index(week, from)).max(0) as u32;
        if to > from && days > 0 {
            slipped_tasks.insert(id);
            slipped.push(ReviewSlip {
                node: task_summary(row),
                kind: SlipKind::DueMoved,
                project: row.project.as_ref().map(|p| p.label.clone()),
                assignee: row.assignee.clone(),
                from: Some(from),
                to: Some(to),
                days,
            });
        }
    }
    for row in input.tasks.iter().filter(|r| is_open(r)) {
        let Some(due) = row.task.due_date else {
            continue;
        };
        // Came due this week (before today) and is still open.
        if due >= week_start
            && due <= week_end
            && due < today
            && !slipped_tasks.contains(&row.task.id)
        {
            slipped.push(ReviewSlip {
                node: task_summary(row),
                kind: SlipKind::Overdue,
                project: row.project.as_ref().map(|p| p.label.clone()),
                assignee: row.assignee.clone(),
                from: None,
                to: Some(due),
                days: (today - due).whole_days().max(1) as u32,
            });
        }
    }
    let mut target_slip = |node_type: NodeType, field: &str, names: HashMap<Uuid, String>| {
        for (id, (from, to)) in date_moves(&entries, node_type, field) {
            let Some(name) = names.get(&id) else {
                continue;
            };
            let days = (end_index(week, to) - end_index(week, from)).max(0) as u32;
            if to > from && days > 0 {
                slipped.push(ReviewSlip {
                    node: summary_of(node_type, id, name),
                    kind: SlipKind::TargetMoved,
                    project: None,
                    assignee: None,
                    from: Some(from),
                    to: Some(to),
                    days,
                });
            }
        }
    };
    target_slip(
        NodeType::Project,
        "target_date",
        input
            .projects
            .iter()
            .filter(|p| !matches!(p.status, ProjectStatus::Done | ProjectStatus::Cancelled))
            .map(|p| (p.id, p.title.clone()))
            .collect(),
    );
    target_slip(
        NodeType::Objective,
        "target_date",
        input
            .objectives
            .iter()
            .filter(|o| o.status != ObjectiveStatus::Done)
            .map(|o| (o.id, o.title.clone()))
            .collect(),
    );
    // Biggest first; projects and objectives (the board's concern) before tasks of equal size.
    slipped.sort_by(|a, b| {
        b.days
            .cmp(&a.days)
            .then_with(|| (b.kind == SlipKind::TargetMoved).cmp(&(a.kind == SlipKind::TargetMoved)))
            .then_with(|| a.node.label.cmp(&b.node.label))
            .then_with(|| a.node.node.id.cmp(&b.node.node.id))
    });

    // ---- blocked
    let newly_blocked: HashSet<Uuid> = entries
        .iter()
        .filter(|a| a.node_type == NodeType::Task)
        .filter(|a| change(a, "status").is_some_and(|(_, new)| new.as_str() == Some("blocked")))
        .map(|a| a.node_id)
        .collect();
    let mut blocked: Vec<ReviewBlocked> = input
        .tasks
        .iter()
        .filter(|r| r.task.status == TaskStatus::Blocked)
        .map(|r| ReviewBlocked {
            task: WeekTask {
                row: r.clone(),
                overdue_days: r
                    .task
                    .due_date
                    .filter(|d| *d < today)
                    .map(|d| (today - d).whole_days().max(1) as u32),
                blocked_by: input.blockers.get(&r.task.id).cloned().unwrap_or_default(),
            },
            newly_blocked: newly_blocked.contains(&r.task.id),
        })
        .collect();
    blocked.sort_by(|a, b| {
        b.newly_blocked
            .cmp(&a.newly_blocked)
            .then(a.task.row.task.priority.cmp(&b.task.row.task.priority))
            .then_with(|| a.task.row.task.title.cmp(&b.task.row.task.title))
            .then_with(|| a.task.row.task.id.cmp(&b.task.row.task.id))
    });

    // ---- done
    let mut done: Vec<ReviewDone> = input
        .tasks
        .iter()
        .filter(|r| r.task.status == TaskStatus::Done)
        .filter_map(|r| {
            let on = r.task.completed_at?.date();
            (on >= week_start && on <= week_end).then(|| ReviewDone {
                node: task_summary(r),
                project: r.project.as_ref().map(|p| p.label.clone()),
                assignee: r.assignee.as_ref().map(|a| a.label.clone()),
                priority: r.task.priority,
                completed_on: on,
            })
        })
        .collect();
    done.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(a.completed_on.cmp(&b.completed_on))
            .then_with(|| a.node.label.cmp(&b.node.label))
            .then_with(|| a.node.node.id.cmp(&b.node.node.id))
    });

    // Projects and objectives marked done this week (and still done).
    let finished_names: HashMap<Uuid, (NodeType, &str)> = input
        .projects
        .iter()
        .filter(|p| p.status == ProjectStatus::Done)
        .map(|p| (p.id, (NodeType::Project, p.title.as_str())))
        .chain(
            input
                .objectives
                .iter()
                .filter(|o| o.status == ObjectiveStatus::Done)
                .map(|o| (o.id, (NodeType::Objective, o.title.as_str()))),
        )
        .collect();
    let mut finished: Vec<ReviewFinished> = Vec::new();
    for a in entries
        .iter()
        .filter(|a| matches!(a.node_type, NodeType::Project | NodeType::Objective))
    {
        let marked_done = change(a, "status").is_some_and(|(_, new)| new.as_str() == Some("done"));
        if let (true, Some((node_type, name))) = (marked_done, finished_names.get(&a.node_id)) {
            if !finished.iter().any(|f| f.node.node.id == a.node_id) {
                finished.push(ReviewFinished {
                    node: summary_of(*node_type, a.node_id, name),
                    on: a.at.date(),
                });
            }
        }
    }
    finished.sort_by(|a, b| {
        a.on.cmp(&b.on)
            .then_with(|| a.node.label.cmp(&b.node.label))
    });

    // ---- decisions made this week
    let mut made = decisions::arrange(
        input.decisions,
        &DecisionFilter {
            date_from: Some(week_start),
            date_to: Some(week_end),
            ..Default::default()
        },
    );
    made.retain(|d| d.status != DecisionStatus::Proposed);

    // ---- waiting-ons resolved this week
    let waiting_resolved: Vec<_> = waiting_on::arrange(
        input.waiting,
        &WaitingOnFilter {
            person_id: None,
            include_resolved: true,
            include_snoozed: true,
        },
        today,
        input.stale_days,
    )
    .into_iter()
    .filter(|w| {
        w.waiting
            .resolved_on
            .is_some_and(|d| d >= week_start && d <= week_end)
    })
    .collect();

    let overview = input.overview;
    WeeklyReview {
        week_start,
        week_end,
        prev_week_start: week_start - Duration::days(7),
        next_week_start: week_start + Duration::days(7),
        today,
        is_current_week: today >= week_start && today <= week_end,
        slipped,
        blocked,
        overloaded: overview.overloaded,
        waiting: overview.stale_waiting,
        waiting_resolved,
        decisions: made,
        done,
        finished,
        counts: overview.counts,
        objectives: overview.objectives,
        reviews: crate::objectives::reviews_due(&input.objectives, today, week_end),
        risks: overview.risks,
        more_risks: overview.more_risks,
        warnings: overview.warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{ActivityAction, Decision, OverviewCounts, Task, WaitingOn};
    use serde_json::json;
    use time::{macros::date, OffsetDateTime};

    // Wednesday 2027-03-03: the week is Mon 03-01 .. Sun 03-07.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(d: Date, hour: u8) -> OffsetDateTime {
        d.with_hms(hour, 0, 0).unwrap().assume_utc()
    }

    fn task(n: u128, title: &str, status: TaskStatus, due: Option<Date>) -> TaskRow {
        TaskRow {
            task: Task {
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
                id: id(n),
                title: title.into(),
                description: String::new(),
                project_id: None,
                status,
                estimate_days: None,
                start_date: None,
                due_date: due,
                completed_at: None,
                priority: 3,
                recurrence: None,
                created_at: at(date!(2027 - 01 - 01), 9),
                updated_at: at(date!(2027 - 01 - 01), 9),
                archived_at: None,
            },
            project: None,
            assignee: None,
            link_count: 0,
            attachment_count: 0,
            parent: None,
            subtask_count: 0,
            subtasks_done: 0,
        }
    }

    fn entry(
        n: u128,
        node_type: NodeType,
        node: u128,
        when: OffsetDateTime,
        diff: serde_json::Value,
    ) -> Activity {
        Activity {
            id: id(1000 + n),
            at: when,
            node_type,
            node_id: id(node),
            action: ActivityAction::Updated,
            diff,
        }
    }

    fn overview() -> PortfolioOverview {
        PortfolioOverview {
            today: TODAY,
            counts: OverviewCounts {
                red: 0,
                amber: 0,
                green: 0,
                idle: 0,
            },
            objectives: vec![],
            unlinked_projects: vec![],
            risks: vec![],
            more_risks: 0,
            overloaded: vec![],
            stale_waiting: vec![],
            warnings: vec![],
            thresholds: Default::default(),
        }
    }

    fn input() -> ReviewInput {
        ReviewInput {
            tasks: vec![],
            blockers: HashMap::new(),
            activity: vec![],
            decisions: vec![],
            waiting: vec![],
            projects: vec![],
            objectives: vec![],
            overview: overview(),
            today: TODAY,
            week_of: None,
            stale_days: 7,
            work_week: WorkWeek::MON_FRI,
        }
    }

    #[test]
    fn the_week_runs_monday_to_sunday_and_any_date_picks_it() {
        let r = build(input());
        assert_eq!(
            (r.week_start, r.week_end),
            (date!(2027 - 03 - 01), date!(2027 - 03 - 07))
        );
        assert!(r.is_current_week);
        assert_eq!(r.prev_week_start, date!(2027 - 02 - 22));
        assert_eq!(r.next_week_start, date!(2027 - 03 - 08));
        let r = build(ReviewInput {
            week_of: Some(date!(2027 - 02 - 28)),
            ..input()
        });
        assert_eq!(r.week_start, date!(2027 - 02 - 22));
        assert!(!r.is_current_week);
    }

    #[test]
    fn a_due_date_moved_later_is_a_slip_counted_in_working_days() {
        let mut i = input();
        i.tasks = vec![task(
            1,
            "Ship",
            TaskStatus::Todo,
            Some(date!(2027 - 03 - 08)),
        )];
        i.activity = vec![entry(
            1,
            NodeType::Task,
            1,
            at(date!(2027 - 03 - 02), 10),
            json!({"due_date": ["2027-03-05", "2027-03-08"]}),
        )];
        let r = build(i);
        assert_eq!(r.slipped.len(), 1);
        let s = &r.slipped[0];
        assert_eq!(s.kind, SlipKind::DueMoved);
        assert_eq!(
            (s.from, s.to),
            (Some(date!(2027 - 03 - 05)), Some(date!(2027 - 03 - 08)))
        );
        assert_eq!(s.days, 1, "Friday to Monday is one working day");
    }

    #[test]
    fn slips_count_only_the_days_the_work_week_works() {
        // In a Sunday-to-Thursday week Friday is a day off, so moving a due date from Friday to
        // Monday spans two working days (Sunday and Monday), not one.
        let mut i = input();
        i.work_week = WorkWeek::from_days(&[6, 0, 1, 2, 3]).unwrap();
        i.tasks = vec![task(
            1,
            "Ship",
            TaskStatus::Todo,
            Some(date!(2027 - 03 - 08)),
        )];
        i.activity = vec![entry(
            1,
            NodeType::Task,
            1,
            at(date!(2027 - 03 - 02), 10),
            json!({"due_date": ["2027-03-05", "2027-03-08"]}),
        )];
        let r = build(i);
        assert_eq!(r.slipped.len(), 1);
        assert_eq!(r.slipped[0].days, 2);
    }

    #[test]
    fn several_moves_count_from_the_first_date_to_the_last() {
        let mut i = input();
        i.tasks = vec![task(
            1,
            "Ship",
            TaskStatus::Todo,
            Some(date!(2027 - 03 - 10)),
        )];
        i.activity = vec![
            entry(
                2,
                NodeType::Task,
                1,
                at(date!(2027 - 03 - 03), 10),
                json!({"due_date": ["2027-03-08", "2027-03-10"]}),
            ),
            entry(
                1,
                NodeType::Task,
                1,
                at(date!(2027 - 03 - 02), 10),
                json!({"due_date": ["2027-03-04", "2027-03-08"]}),
            ),
        ];
        let r = build(i);
        assert_eq!(r.slipped.len(), 1);
        assert_eq!(r.slipped[0].from, Some(date!(2027 - 03 - 04)));
        assert_eq!(r.slipped[0].to, Some(date!(2027 - 03 - 10)));
        assert_eq!(r.slipped[0].days, 4);
    }

    #[test]
    fn earlier_dates_first_dates_and_other_weeks_are_not_slips() {
        let mut i = input();
        i.tasks = vec![
            task(
                1,
                "Pulled in",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 04)),
            ),
            task(2, "New date", TaskStatus::Todo, Some(date!(2027 - 03 - 20))),
            task(
                3,
                "Last week",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 20)),
            ),
            task(
                4,
                "Finished anyway",
                TaskStatus::Done,
                Some(date!(2027 - 03 - 20)),
            ),
        ];
        i.activity = vec![
            entry(
                1,
                NodeType::Task,
                1,
                at(date!(2027 - 03 - 02), 10),
                json!({"due_date": ["2027-03-08", "2027-03-04"]}),
            ),
            entry(
                2,
                NodeType::Task,
                2,
                at(date!(2027 - 03 - 02), 10),
                json!({"due_date": [null, "2027-03-20"]}),
            ),
            entry(
                3,
                NodeType::Task,
                3,
                at(date!(2027 - 02 - 24), 10),
                json!({"due_date": ["2027-03-01", "2027-03-20"]}),
            ),
            entry(
                4,
                NodeType::Task,
                4,
                at(date!(2027 - 03 - 02), 10),
                json!({"due_date": ["2027-03-01", "2027-03-20"]}),
            ),
        ];
        assert!(build(i).slipped.is_empty());
    }

    #[test]
    fn work_that_came_due_this_week_and_is_still_open_is_overdue_once() {
        let mut i = input();
        i.tasks = vec![
            task(
                1,
                "Late",
                TaskStatus::InProgress,
                Some(date!(2027 - 03 - 01)),
            ),
            task(2, "Due today", TaskStatus::Todo, Some(TODAY)),
            task(3, "Done", TaskStatus::Done, Some(date!(2027 - 03 - 01))),
            task(
                4,
                "Earlier week",
                TaskStatus::Todo,
                Some(date!(2027 - 02 - 20)),
            ),
            task(
                5,
                "Moved and late",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 02)),
            ),
        ];
        i.activity = vec![entry(
            1,
            NodeType::Task,
            5,
            at(date!(2027 - 03 - 01), 9),
            json!({"due_date": ["2027-02-26", "2027-03-02"]}),
        )];
        let r = build(i);
        let kinds: Vec<(&str, SlipKind, u32)> = r
            .slipped
            .iter()
            .map(|s| (s.node.label.as_str(), s.kind, s.days))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("Late", SlipKind::Overdue, 2),
                ("Moved and late", SlipKind::DueMoved, 2),
            ],
            "biggest first; a moved task is reported once, as moved"
        );
    }

    #[test]
    fn project_and_objective_targets_moved_later_are_slips() {
        use minimap_types::{Objective, Project};
        let project = Project {
            id: id(50),
            title: "EU region".into(),
            slug: "eu".into(),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: Some(date!(2027 - 04 - 15)),
            status: ProjectStatus::Active,
            priority: 3,
            created_at: at(date!(2027 - 01 - 01), 9),
            updated_at: at(date!(2027 - 01 - 01), 9),
            archived_at: None,
        };
        let objective = Objective {
            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            id: id(60),
            title: "Launch".into(),
            description: String::new(),
            target_date: Some(date!(2027 - 06 - 30)),
            status: ObjectiveStatus::OnTrack,
            priority: 2,
            created_at: at(date!(2027 - 01 - 01), 9),
            updated_at: at(date!(2027 - 01 - 01), 9),
            archived_at: None,
        };
        let mut i = input();
        i.projects = vec![project];
        i.objectives = vec![objective];
        i.activity = vec![
            entry(
                1,
                NodeType::Project,
                50,
                at(date!(2027 - 03 - 02), 10),
                json!({"target_date": ["2027-04-01", "2027-04-15"]}),
            ),
            entry(
                2,
                NodeType::Objective,
                60,
                at(date!(2027 - 03 - 02), 11),
                json!({"target_date": ["2027-06-30", "2027-06-01"]}),
            ),
        ];
        let r = build(i);
        assert_eq!(r.slipped.len(), 1, "an earlier target is not a slip");
        assert_eq!(r.slipped[0].kind, SlipKind::TargetMoved);
        assert_eq!(r.slipped[0].node.label, "EU region");
        assert_eq!(r.slipped[0].days, 10);
    }

    #[test]
    fn blocked_tasks_list_the_new_ones_first() {
        let mut i = input();
        let mut urgent = task(2, "Urgent old", TaskStatus::Blocked, None);
        urgent.task.priority = 1;
        i.tasks = vec![
            urgent,
            task(1, "Newly stuck", TaskStatus::Blocked, None),
            task(3, "Fine", TaskStatus::Todo, None),
        ];
        i.blockers.insert(
            id(1),
            vec![NodeSummary {
                node: NodeRef::new(NodeType::Task, id(9)),
                label: "Upstream".into(),
                archived: false,
            }],
        );
        i.activity = vec![entry(
            1,
            NodeType::Task,
            1,
            at(date!(2027 - 03 - 02), 10),
            json!({"status": ["todo", "blocked"]}),
        )];
        let r = build(i);
        let names: Vec<(&str, bool)> = r
            .blocked
            .iter()
            .map(|b| (b.task.row.task.title.as_str(), b.newly_blocked))
            .collect();
        assert_eq!(names, vec![("Newly stuck", true), ("Urgent old", false)]);
        assert_eq!(r.blocked[0].task.blocked_by[0].label, "Upstream");
    }

    #[test]
    fn done_means_completed_during_the_week() {
        let mut i = input();
        let mut a = task(1, "This week", TaskStatus::Done, None);
        a.task.completed_at = Some(at(date!(2027 - 03 - 02), 15));
        let mut b = task(2, "Last week", TaskStatus::Done, None);
        b.task.completed_at = Some(at(date!(2027 - 02 - 26), 15));
        let mut c = task(3, "Sunday night", TaskStatus::Done, None);
        c.task.completed_at = Some(at(date!(2027 - 03 - 07), 23));
        let d = task(4, "Not finished", TaskStatus::InProgress, None);
        i.tasks = vec![a, b, c, d];
        let r = build(i);
        let names: Vec<&str> = r.done.iter().map(|d| d.node.label.as_str()).collect();
        assert_eq!(names, vec!["This week", "Sunday night"]);
    }

    #[test]
    fn the_review_lists_ongoing_objectives_that_are_overdue_or_due_this_week() {
        let obj = |n: u128, title: &str, last: Date| Objective {
            id: id(n),
            title: title.into(),
            description: String::new(),
            target_date: None,
            status: ObjectiveStatus::OnTrack,
            priority: 3,
            ongoing: true,
            review_every_days: Some(30),
            last_reviewed_on: Some(last),
            created_at: at(date!(2026 - 01 - 01), 9),
            updated_at: at(date!(2026 - 01 - 01), 9),
            archived_at: None,
        };
        let mut i = input();
        i.objectives = vec![
            obj(1, "Maintenance", date!(2027 - 01 - 15)), // due 02-14: overdue
            obj(2, "Security hygiene", date!(2027 - 02 - 05)), // due 03-07: Sunday
            obj(3, "Later", date!(2027 - 03 - 01)),
        ];
        let r = build(i);
        let names: Vec<&str> = r
            .reviews
            .iter()
            .map(|x| x.objective.label.as_str())
            .collect();
        assert_eq!(names, ["Maintenance", "Security hygiene"]);
        assert!(r.reviews[0].overdue_days.is_some() && r.reviews[1].overdue_days.is_none());
    }

    #[test]
    fn projects_marked_done_this_week_are_finished() {
        use minimap_types::Project;
        let project = |n: u128, title: &str, status| Project {
            id: id(n),
            title: title.into(),
            slug: title.to_lowercase(),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status,
            priority: 3,
            created_at: at(date!(2027 - 01 - 01), 9),
            updated_at: at(date!(2027 - 01 - 01), 9),
            archived_at: None,
        };
        let mut i = input();
        i.projects = vec![
            project(1, "Alpha", ProjectStatus::Done),
            project(2, "Beta", ProjectStatus::Done),
            project(3, "Gamma", ProjectStatus::Active),
        ];
        i.activity = vec![
            entry(
                1,
                NodeType::Project,
                1,
                at(date!(2027 - 03 - 02), 10),
                json!({"status": ["active", "done"]}),
            ),
            // Done long ago.
            entry(
                2,
                NodeType::Project,
                2,
                at(date!(2027 - 02 - 02), 10),
                json!({"status": ["active", "done"]}),
            ),
            // Marked done, then reopened.
            entry(
                3,
                NodeType::Project,
                3,
                at(date!(2027 - 03 - 02), 10),
                json!({"status": ["active", "done"]}),
            ),
        ];
        let r = build(i);
        assert_eq!(r.finished.len(), 1);
        assert_eq!(r.finished[0].node.label, "Alpha");
        assert_eq!(r.finished[0].on, date!(2027 - 03 - 02));
    }

    #[test]
    fn decisions_made_this_week_exclude_proposals_and_other_weeks() {
        let item = |n: u128, title: &str, status, on: Option<Date>, created: Date| DecisionItem {
            decision: Decision {
                id: id(n),
                title: title.into(),
                context: String::new(),
                decision: format!("{title} text"),
                rationale: String::new(),
                decided_on: on,
                status,
                created_at: at(created, 9),
                updated_at: at(created, 9),
                archived_at: None,
            },
            affects: vec![],
            superseded_by: None,
        };
        let mut i = input();
        i.decisions = vec![
            item(
                1,
                "Made",
                DecisionStatus::Decided,
                Some(date!(2027 - 03 - 02)),
                date!(2027 - 03 - 02),
            ),
            item(
                2,
                "Only proposed",
                DecisionStatus::Proposed,
                None,
                date!(2027 - 03 - 02),
            ),
            item(
                3,
                "Old",
                DecisionStatus::Decided,
                Some(date!(2027 - 02 - 10)),
                date!(2027 - 02 - 10),
            ),
            item(
                4,
                "Replaced since",
                DecisionStatus::Superseded,
                Some(date!(2027 - 03 - 01)),
                date!(2027 - 02 - 01),
            ),
        ];
        let r = build(i);
        let titles: Vec<&str> = r.decisions.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(titles, vec!["Made", "Replaced since"]);
    }

    #[test]
    fn waiting_ons_resolved_this_week_are_listed_and_stale_ones_come_from_the_overview() {
        let wait = |n: u128, resolved: Option<Date>| WaitingOnItem {
            waiting: WaitingOn {
                id: id(n),
                description: format!("Thing {n}"),
                person_id: id(99),
                asked_on: date!(2027 - 02 - 01),
                expected_by: None,
                follow_up_on: None,
                resolved_on: resolved,
                created_at: at(date!(2027 - 02 - 01), 9),
                updated_at: at(date!(2027 - 02 - 01), 9),
                archived_at: None,
            },
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, id(99)),
                label: "Raj".into(),
                archived: false,
            },
            about: None,
        };
        let mut i = input();
        i.waiting = vec![
            wait(1, Some(date!(2027 - 03 - 02))),
            wait(2, Some(date!(2027 - 02 - 20))),
            wait(3, None),
        ];
        let r = build(i);
        assert_eq!(r.waiting_resolved.len(), 1);
        assert_eq!(r.waiting_resolved[0].waiting.description, "Thing 1");
        assert!(r.waiting.is_empty(), "stale ones are the overview's");
    }
}
