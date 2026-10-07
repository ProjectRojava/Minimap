//! "This week" (spec 16): what needs attention now. Pure. Weeks run Monday to Sunday.
//!
//! Sections answer different questions, so a task can be in more than one (an overdue task
//! that is also blocked shows in both): overdue (before today), due this week (today through
//! Sunday), blocked, my tasks in progress, waiting-ons that are stale or due, 1:1s this week.

use std::collections::HashMap;

use minimap_types::{
    Date, NodeSummary, NoteFilter, NoteItem, NoteKind, TaskRow, TaskStatus, ThisWeek, Uuid,
    WaitingOnFilter, WaitingOnItem, WaitingOnRow, WeekDay, WeekTask,
};
use time::Duration;

use crate::{notes, waiting_on};

pub struct WeekInput {
    pub tasks: Vec<TaskRow>,
    /// Open blockers of each task.
    pub blockers: HashMap<Uuid, Vec<NodeSummary>>,
    pub waiting: Vec<WaitingOnItem>,
    pub notes: Vec<NoteItem>,
    /// Active objectives (the ongoing ones with a review rhythm can be due for review).
    pub objectives: Vec<minimap_types::Objective>,
    /// The "me" person, if there is one.
    pub self_id: Option<Uuid>,
    pub today: Date,
    /// Any date in the week to show; today's week when `None`.
    pub week_of: Option<Date>,
    pub stale_days: u32,
}

/// The Monday of the week containing `d`.
pub fn monday_of(d: Date) -> Date {
    d - Duration::days(i64::from(d.weekday().number_days_from_monday()))
}

fn is_open(row: &TaskRow) -> bool {
    !matches!(row.task.status, TaskStatus::Done | TaskStatus::Cancelled)
}

pub fn build(input: WeekInput) -> ThisWeek {
    let today = input.today;
    let week_start = monday_of(input.week_of.unwrap_or(today));
    let week_end = week_start + Duration::days(6);
    let due_from = week_start.max(today);

    let wrap = |row: &TaskRow| -> WeekTask {
        let overdue_days = row
            .task
            .due_date
            .filter(|d| *d < today)
            .map(|d| (today - d).whole_days().max(1) as u32);
        WeekTask {
            row: row.clone(),
            overdue_days,
            blocked_by: input
                .blockers
                .get(&row.task.id)
                .cloned()
                .unwrap_or_default(),
        }
    };
    let by_due_then_priority = |a: &WeekTask, b: &WeekTask| {
        a.row
            .task
            .due_date
            .cmp(&b.row.task.due_date)
            .then(a.row.task.priority.cmp(&b.row.task.priority))
            .then_with(|| a.row.task.title.cmp(&b.row.task.title))
            .then_with(|| a.row.task.id.cmp(&b.row.task.id))
    };
    // Priority first, undated last, for sections that aren't about a date.
    let by_priority_then_due = |a: &WeekTask, b: &WeekTask| {
        let key = |t: &WeekTask| {
            (
                t.row.task.priority,
                t.row.task.due_date.is_none(),
                t.row.task.due_date,
            )
        };
        key(a)
            .cmp(&key(b))
            .then_with(|| a.row.task.title.cmp(&b.row.task.title))
            .then_with(|| a.row.task.id.cmp(&b.row.task.id))
    };

    let open: Vec<&TaskRow> = input.tasks.iter().filter(|r| is_open(r)).collect();
    let mut overdue: Vec<WeekTask> = open
        .iter()
        .filter(|r| r.task.due_date.is_some_and(|d| d < today))
        .map(|r| wrap(r))
        .collect();
    overdue.sort_by(by_due_then_priority);
    let mut due_this_week: Vec<WeekTask> = open
        .iter()
        .filter(|r| {
            r.task
                .due_date
                .is_some_and(|d| d >= due_from && d <= week_end)
        })
        .map(|r| wrap(r))
        .collect();
    due_this_week.sort_by(by_due_then_priority);
    let mut blocked: Vec<WeekTask> = open
        .iter()
        .filter(|r| r.task.status == TaskStatus::Blocked)
        .map(|r| wrap(r))
        .collect();
    blocked.sort_by(by_priority_then_due);
    let mut in_progress: Vec<WeekTask> = open
        .iter()
        .filter(|r| r.task.status == TaskStatus::InProgress)
        .filter(|r| match input.self_id {
            Some(me) => r.assignee.as_ref().is_some_and(|a| a.node.id == me),
            None => true,
        })
        .map(|r| wrap(r))
        .collect();
    in_progress.sort_by(by_priority_then_due);

    // Waiting-ons: open and not snoozed (the shared rules), then stale or expected by Sunday.
    let waiting: Vec<WaitingOnRow> = waiting_on::arrange(
        input.waiting.clone(),
        &WaitingOnFilter::default(),
        today,
        input.stale_days,
    )
    .into_iter()
    .filter(|w| w.stale || w.waiting.expected_by.is_some_and(|d| d <= week_end))
    .collect();

    // 1:1s dated this week, earliest first.
    let mut one_on_ones = notes::arrange(
        input.notes.clone(),
        &NoteFilter {
            kind: Some(NoteKind::OneOnOne),
            date_from: Some(week_start),
            date_to: Some(week_end),
            ..Default::default()
        },
    );
    one_on_ones.reverse();

    let days: Vec<WeekDay> = (0..7)
        .map(|i| {
            let date = week_start + Duration::days(i);
            WeekDay {
                date,
                is_today: date == today,
                tasks_due: open
                    .iter()
                    .filter(|r| r.task.due_date == Some(date))
                    .count() as u32,
                waiting_expected: input
                    .waiting
                    .iter()
                    .filter(|w| {
                        w.waiting.resolved_on.is_none() && w.waiting.expected_by == Some(date)
                    })
                    .count() as u32,
                one_on_ones: one_on_ones.iter().filter(|n| n.note_date == date).count() as u32,
            }
        })
        .collect();

    ThisWeek {
        week_start,
        week_end,
        prev_week_start: week_start - Duration::days(7),
        next_week_start: week_start + Duration::days(7),
        today,
        is_current_week: today >= week_start && today <= week_end,
        has_self: input.self_id.is_some(),
        days,
        overdue,
        due_this_week,
        blocked,
        in_progress,
        waiting,
        one_on_ones,
        reviews: crate::objectives::reviews_due(&input.objectives, today, week_end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeType, Task, WaitingOn};
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    // Wednesday 2027-03-03: the week is Mon 03-01 .. Sun 03-07.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn row(n: u128, title: &str, status: TaskStatus, due: Option<Date>, priority: u8) -> TaskRow {
        TaskRow {
            task: Task {
                links: Vec::new(),
                id: id(n),
                title: title.into(),
                description: String::new(),
                project_id: None,
                status,
                estimate_days: None,
                start_date: None,
                due_date: due,
                completed_at: None,
                priority,
                recurrence: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            project: None,
            assignee: None,
        }
    }

    fn assigned(mut r: TaskRow, person: u128) -> TaskRow {
        r.assignee = Some(NodeSummary {
            node: NodeRef::new(NodeType::Person, id(person)),
            label: format!("P{person}"),
            archived: false,
        });
        r
    }

    fn week(tasks: Vec<TaskRow>) -> ThisWeek {
        build(WeekInput {
            objectives: Vec::new(),
            tasks,
            blockers: HashMap::new(),
            waiting: vec![],
            notes: vec![],
            self_id: Some(id(1)),
            today: TODAY,
            week_of: None,
            stale_days: 7,
        })
    }

    fn ongoing_objective(
        n: u128,
        title: &str,
        every: Option<u32>,
        last: Option<Date>,
    ) -> minimap_types::Objective {
        minimap_types::Objective {
            id: id(n),
            title: title.into(),
            description: String::new(),
            target_date: None,
            status: minimap_types::ObjectiveStatus::OnTrack,
            priority: 3,
            ongoing: true,
            review_every_days: every,
            last_reviewed_on: last,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    #[test]
    fn the_week_lists_ongoing_objectives_whose_review_is_overdue_or_due_by_sunday() {
        let weeks = |week_of: Option<Date>| {
            build(WeekInput {
                objectives: vec![
                    ongoing_objective(1, "Overdue", Some(30), Some(date!(2027 - 01 - 20))),
                    ongoing_objective(2, "Friday", Some(30), Some(date!(2027 - 02 - 05))),
                    ongoing_objective(3, "Next month", Some(30), Some(date!(2027 - 02 - 20))),
                    ongoing_objective(4, "No rhythm", None, None),
                ],
                tasks: vec![],
                blockers: HashMap::new(),
                waiting: vec![],
                notes: vec![],
                self_id: None,
                today: TODAY,
                week_of,
                stale_days: 7,
            })
        };
        let w = weeks(None);
        let names: Vec<&str> = w
            .reviews
            .iter()
            .map(|r| r.objective.label.as_str())
            .collect();
        assert_eq!(names, ["Overdue", "Friday"]);
        assert_eq!(w.reviews[0].overdue_days, Some(12));
        assert_eq!(w.reviews[1].overdue_days, None);
        // A later week reaches further: by then "Next month" is due too.
        assert_eq!(weeks(Some(date!(2027 - 03 - 22))).reviews.len(), 3);
    }

    fn titles(v: &[WeekTask]) -> Vec<&str> {
        v.iter().map(|t| t.row.task.title.as_str()).collect()
    }

    fn waiting_item(n: u128, asked: Date, expected: Option<Date>) -> WaitingOnItem {
        WaitingOnItem {
            waiting: WaitingOn {
                id: id(n),
                description: format!("W{n}"),
                person_id: id(2),
                asked_on: asked,
                expected_by: expected,
                follow_up_on: None,
                resolved_on: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, id(2)),
                label: "Raj".into(),
                archived: false,
            },
            about: None,
        }
    }

    fn note(n: u128, title: &str, kind: NoteKind, on: Date) -> NoteItem {
        NoteItem {
            note: minimap_types::Note {
                id: id(n),
                title: title.into(),
                body: String::new(),
                note_date: on,
                kind,
                recurrence: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            mentions: vec![],
        }
    }

    // ---------------------------------------------------------------- weeks

    #[test]
    fn weeks_run_monday_to_sunday_whatever_date_you_ask_for() {
        let w = week(vec![]);
        assert_eq!(
            (w.week_start, w.week_end),
            (date!(2027 - 03 - 01), date!(2027 - 03 - 07))
        );
        assert!(w.is_current_week);
        assert_eq!(w.days.len(), 7);
        assert_eq!(
            (w.prev_week_start, w.next_week_start),
            (date!(2027 - 02 - 22), date!(2027 - 03 - 08))
        );
        assert_eq!(w.days[0].date, date!(2027 - 03 - 01));
        assert!(w.days[2].is_today && w.days.iter().filter(|d| d.is_today).count() == 1);
        for d in [
            date!(2027 - 03 - 01),
            date!(2027 - 03 - 07),
            date!(2027 - 03 - 04),
        ] {
            assert_eq!(monday_of(d), date!(2027 - 03 - 01), "{d}");
        }
        // Sunday belongs to the week that ends on it, not the next one.
        assert_eq!(monday_of(date!(2027 - 03 - 08)), date!(2027 - 03 - 08));
        // Another week.
        let next = build(WeekInput {
            objectives: Vec::new(),
            tasks: vec![],
            blockers: HashMap::new(),
            waiting: vec![],
            notes: vec![],
            self_id: None,
            today: TODAY,
            week_of: Some(date!(2027 - 03 - 10)),
            stale_days: 7,
        });
        assert_eq!(
            (next.week_start, next.is_current_week),
            (date!(2027 - 03 - 08), false)
        );
    }

    // ---------------------------------------------------------------- tasks

    #[test]
    fn overdue_is_everything_open_before_today_oldest_first_with_days_late() {
        let w = week(vec![
            row(
                1,
                "Yesterday",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 02)),
                3,
            ),
            row(
                2,
                "Last month",
                TaskStatus::InProgress,
                Some(date!(2027 - 02 - 01)),
                3,
            ),
            row(
                3,
                "Last month urgent",
                TaskStatus::Todo,
                Some(date!(2027 - 02 - 01)),
                1,
            ),
            row(
                4,
                "Done late",
                TaskStatus::Done,
                Some(date!(2027 - 02 - 01)),
                3,
            ),
            row(
                5,
                "Cancelled",
                TaskStatus::Cancelled,
                Some(date!(2027 - 02 - 01)),
                3,
            ),
            row(6, "Today", TaskStatus::Todo, Some(TODAY), 3),
            row(7, "No date", TaskStatus::Todo, None, 3),
        ]);
        assert_eq!(
            titles(&w.overdue),
            ["Last month urgent", "Last month", "Yesterday"]
        );
        assert_eq!(w.overdue[0].overdue_days, Some(30));
        assert_eq!(w.overdue[2].overdue_days, Some(1));
        // Due today is not overdue.
        assert!(w.overdue.iter().all(|t| t.row.task.title != "Today"));
    }

    #[test]
    fn due_this_week_runs_from_today_to_sunday() {
        let w = week(vec![
            row(
                1,
                "Monday (past)",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 01)),
                3,
            ),
            row(2, "Today", TaskStatus::Todo, Some(TODAY), 3),
            row(
                3,
                "Friday low",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 05)),
                4,
            ),
            row(
                4,
                "Friday high",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 05)),
                1,
            ),
            row(
                5,
                "Sunday",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 07)),
                3,
            ),
            row(
                6,
                "Next Monday",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 08)),
                3,
            ),
            row(
                7,
                "Finished",
                TaskStatus::Done,
                Some(date!(2027 - 03 - 04)),
                3,
            ),
        ]);
        assert_eq!(
            titles(&w.due_this_week),
            ["Today", "Friday high", "Friday low", "Sunday"]
        );
        // The past Monday is overdue, not "due this week".
        assert_eq!(titles(&w.overdue), ["Monday (past)"]);
    }

    #[test]
    fn a_future_week_shows_its_whole_week_and_a_past_week_nothing_new() {
        let tasks = vec![
            row(
                1,
                "Mon next",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 08)),
                3,
            ),
            row(
                2,
                "Sun next",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 14)),
                3,
            ),
            row(
                3,
                "Beyond",
                TaskStatus::Todo,
                Some(date!(2027 - 03 - 15)),
                3,
            ),
            row(4, "Old", TaskStatus::Todo, Some(date!(2027 - 02 - 24)), 3),
        ];
        let of = |d| {
            build(WeekInput {
                objectives: Vec::new(),
                tasks: tasks.clone(),
                blockers: HashMap::new(),
                waiting: vec![],
                notes: vec![],
                self_id: None,
                today: TODAY,
                week_of: Some(d),
                stale_days: 7,
            })
        };
        let next = of(date!(2027 - 03 - 10));
        assert_eq!(titles(&next.due_this_week), ["Mon next", "Sun next"]);
        assert_eq!(
            titles(&next.overdue),
            ["Old"],
            "overdue is about today, whatever week is shown"
        );
        let past = of(date!(2027 - 02 - 24));
        assert!(past.due_this_week.is_empty());
        assert_eq!(titles(&past.overdue), ["Old"]);
    }

    #[test]
    fn blocked_tasks_carry_what_blocks_them_and_sort_by_priority() {
        let blocker = NodeSummary {
            node: NodeRef::new(NodeType::Task, id(99)),
            label: "Legal review".into(),
            archived: false,
        };
        let mut blockers = HashMap::new();
        blockers.insert(id(1), vec![blocker.clone()]);
        let w = build(WeekInput {
            objectives: Vec::new(),
            tasks: vec![
                row(1, "Stuck", TaskStatus::Blocked, None, 2),
                row(
                    2,
                    "Stuck urgent",
                    TaskStatus::Blocked,
                    Some(date!(2027 - 03 - 20)),
                    1,
                ),
                row(3, "Fine", TaskStatus::Todo, None, 1),
            ],
            blockers,
            waiting: vec![],
            notes: vec![],
            self_id: None,
            today: TODAY,
            week_of: None,
            stale_days: 7,
        });
        assert_eq!(titles(&w.blocked), ["Stuck urgent", "Stuck"]);
        assert_eq!(w.blocked[1].blocked_by, vec![blocker]);
        assert!(w.blocked[0].blocked_by.is_empty());
    }

    #[test]
    fn in_progress_is_my_work_only_when_i_exist() {
        let tasks = vec![
            assigned(row(1, "Mine", TaskStatus::InProgress, None, 3), 1),
            assigned(row(2, "Theirs", TaskStatus::InProgress, None, 3), 5),
            row(3, "Nobody's", TaskStatus::InProgress, None, 3),
            assigned(row(4, "Mine todo", TaskStatus::Todo, None, 3), 1),
        ];
        let mine = week(tasks.clone());
        assert_eq!(titles(&mine.in_progress), ["Mine"]);
        assert!(mine.has_self);
        // Without a "me" person there is no "mine": show everything in progress.
        let anyone = build(WeekInput {
            objectives: Vec::new(),
            tasks,
            blockers: HashMap::new(),
            waiting: vec![],
            notes: vec![],
            self_id: None,
            today: TODAY,
            week_of: None,
            stale_days: 7,
        });
        assert_eq!(anyone.in_progress.len(), 3);
        assert!(!anyone.has_self);
    }

    #[test]
    fn a_task_can_be_in_several_sections() {
        let w = week(vec![assigned(
            row(
                1,
                "Overdue and stuck",
                TaskStatus::Blocked,
                Some(date!(2027 - 02 - 20)),
                2,
            ),
            1,
        )]);
        assert_eq!(w.overdue.len(), 1);
        assert_eq!(w.blocked.len(), 1);
    }

    // -------------------------------------------------------------- waiting

    #[test]
    fn waiting_ons_are_stale_or_expected_by_sunday_and_never_snoozed_or_resolved() {
        let mut snoozed = waiting_item(5, date!(2027 - 02 - 01), None);
        snoozed.waiting.follow_up_on = Some(date!(2027 - 03 - 20));
        let mut resolved = waiting_item(6, date!(2027 - 02 - 01), None);
        resolved.waiting.resolved_on = Some(date!(2027 - 02 - 10));
        let w = build(WeekInput {
            objectives: Vec::new(),
            tasks: vec![],
            blockers: HashMap::new(),
            waiting: vec![
                waiting_item(1, date!(2027 - 02 - 01), None), // stale (30 days)
                waiting_item(2, date!(2027 - 03 - 02), Some(date!(2027 - 03 - 05))), // due Friday
                waiting_item(3, date!(2027 - 03 - 02), Some(date!(2027 - 03 - 09))), // next week
                waiting_item(4, date!(2027 - 03 - 02), None), // young, no date
                snoozed,
                resolved,
            ],
            notes: vec![],
            self_id: None,
            today: TODAY,
            week_of: None,
            stale_days: 7,
        });
        let names: Vec<&str> = w
            .waiting
            .iter()
            .map(|x| x.waiting.description.as_str())
            .collect();
        assert_eq!(names, ["W1", "W2"]);
        assert!(w.waiting[0].stale && !w.waiting[1].stale);
        // The strip counts expected dates for every open one.
        assert_eq!(w.days[4].waiting_expected, 1);
    }

    // ----------------------------------------------------------------- 1:1s

    #[test]
    fn one_on_ones_are_this_weeks_notes_of_that_kind_earliest_first() {
        let w = build(WeekInput {
            objectives: Vec::new(),
            tasks: vec![],
            blockers: HashMap::new(),
            waiting: vec![],
            notes: vec![
                note(1, "Fri 1:1", NoteKind::OneOnOne, date!(2027 - 03 - 05)),
                note(2, "Mon 1:1", NoteKind::OneOnOne, date!(2027 - 03 - 01)),
                note(3, "Meeting", NoteKind::Meeting, date!(2027 - 03 - 02)),
                note(4, "Last week", NoteKind::OneOnOne, date!(2027 - 02 - 26)),
                note(5, "Next week", NoteKind::OneOnOne, date!(2027 - 03 - 08)),
            ],
            self_id: None,
            today: TODAY,
            week_of: None,
            stale_days: 7,
        });
        let names: Vec<&str> = w.one_on_ones.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(names, ["Mon 1:1", "Fri 1:1"]);
        assert_eq!(
            (
                w.days[0].one_on_ones,
                w.days[4].one_on_ones,
                w.days[1].one_on_ones
            ),
            (1, 1, 0)
        );
    }

    // ------------------------------------------------------------ the strip

    #[test]
    fn the_strip_counts_open_tasks_due_each_day() {
        let w = week(vec![
            row(1, "a", TaskStatus::Todo, Some(date!(2027 - 03 - 03)), 3),
            row(2, "b", TaskStatus::Todo, Some(date!(2027 - 03 - 03)), 3),
            row(3, "done", TaskStatus::Done, Some(date!(2027 - 03 - 03)), 3),
            row(4, "c", TaskStatus::Blocked, Some(date!(2027 - 03 - 06)), 3),
        ]);
        let counts: Vec<u32> = w.days.iter().map(|d| d.tasks_due).collect();
        assert_eq!(counts, [0, 0, 2, 0, 0, 1, 0]);
    }

    proptest! {
           /// Sections are exactly filters of the data: nothing closed, every date inside its
           /// bounds, and the strip adds up to the tasks due inside the week.
           #[test]
           fn sections_match_the_underlying_data(
               specs in proptest::collection::vec((0u8..5, proptest::option::of(-20i64..20), 1u8..6), 0..25),
               offset in -10i64..10,
           ) {
               let tasks: Vec<TaskRow> = specs.iter().enumerate().map(|(i, (s, due, p))| {
                   let status = [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Blocked, TaskStatus::Done, TaskStatus::Cancelled][*s as usize];
                   row(i as u128 + 100, &format!("T{i}"), status, due.map(|d| TODAY + Duration::days(d)), *p)
               }).collect();
               let w = build(WeekInput {
    objectives: Vec::new(),
                   tasks: tasks.clone(), blockers: HashMap::new(), waiting: vec![], notes: vec![],
                   self_id: None, today: TODAY, week_of: Some(TODAY + Duration::days(offset)), stale_days: 7,
               });
               let open = |t: &WeekTask| !matches!(t.row.task.status, TaskStatus::Done | TaskStatus::Cancelled);
               for t in w.overdue.iter().chain(&w.due_this_week).chain(&w.blocked).chain(&w.in_progress) {
                   prop_assert!(open(t));
               }
               let overdue_ok = w.overdue.iter().all(|t| t.row.task.due_date.unwrap() < TODAY && t.overdue_days.unwrap() >= 1);
               prop_assert!(overdue_ok);
               let due_ok = w.due_this_week.iter().all(|t| {
                   let d = t.row.task.due_date.unwrap();
                   d >= TODAY && d >= w.week_start && d <= w.week_end && t.overdue_days.is_none()
               });
               prop_assert!(due_ok);
               // Completeness: every open task with a due date in [today, Sunday] ∩ week is listed.
               let expect = tasks.iter().filter(|r| !matches!(r.task.status, TaskStatus::Done | TaskStatus::Cancelled))
                   .filter(|r| r.task.due_date.is_some_and(|d| d >= TODAY.max(w.week_start) && d <= w.week_end)).count();
               prop_assert_eq!(w.due_this_week.len(), expect);
               prop_assert_eq!(w.blocked.len(), tasks.iter().filter(|r| r.task.status == TaskStatus::Blocked).count());
               prop_assert!(w.overdue.windows(2).all(|p| p[0].row.task.due_date <= p[1].row.task.due_date));
               prop_assert_eq!(w.week_start.weekday(), time::Weekday::Monday);
               prop_assert_eq!((w.week_end - w.week_start).whole_days(), 6);
               let strip: u32 = w.days.iter().map(|d| d.tasks_due).sum();
               let in_week = tasks.iter().filter(|r| !matches!(r.task.status, TaskStatus::Done | TaskStatus::Cancelled))
                   .filter(|r| r.task.due_date.is_some_and(|d| d >= w.week_start && d <= w.week_end)).count() as u32;
               prop_assert_eq!(strip, in_week);
           }
       }
}
