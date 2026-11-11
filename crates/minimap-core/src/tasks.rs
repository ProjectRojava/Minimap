//! Task list rules (filtering, ordering), estimate parsing and pasted-list parsing.

use std::cmp::Ordering;

use minimap_types::{
    Edge, EdgeType, Patch, Task, TaskFigure, TaskFilter, TaskRow, TaskStatus, UpdateTask, Uuid,
    WorkWeek,
};
use time::Date;

use crate::schedule::{date_of, end_index, working_index};

pub fn is_closed(s: TaskStatus) -> bool {
    matches!(s, TaskStatus::Done | TaskStatus::Cancelled)
}

/// The open tasks that wait on `task` (it blocks them) and are not on the backburner themselves:
/// what stays held up while `task` is set aside (spec 59). The schedule still counts a task on the
/// backburner, so these keep their place behind it; this is what the person is told.
pub fn held_up_by(task: Uuid, tasks: &[Task], edges: &[Edge]) -> Vec<Uuid> {
    let mut held: Vec<Uuid> = edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks && e.from_id == task && e.archived_at.is_none())
        .filter_map(|e| tasks.iter().find(|t| t.id == e.to_id))
        .filter(|t| t.archived_at.is_none() && !is_closed(t.status) && !t.is_parked())
        .map(|t| t.id)
        .collect();
    held.sort();
    held.dedup();
    held
}

pub fn filter(rows: Vec<TaskRow>, f: &TaskFilter) -> Vec<TaskRow> {
    let terms: Vec<String> = f
        .text
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    rows.into_iter()
        .filter(|r| match f.status {
            Some(s) => r.task.status == s,
            None => f.include_closed || !is_closed(r.task.status),
        })
        .filter(|r| !f.hide_parked || !r.task.is_parked())
        .filter(|r| !f.no_project || r.task.project_id.is_none())
        .filter(|r| {
            f.task_type
                .as_deref()
                .is_none_or(|t| r.task.task_type.as_deref() == Some(t))
        })
        .filter(|r| f.project_id.is_none_or(|p| r.task.project_id == Some(p)))
        .filter(|r| {
            f.assignee_id
                .is_none_or(|a| r.assignee.as_ref().is_some_and(|s| s.node.id == a))
        })
        .filter(|r| match (f.due_from, f.due_to) {
            (None, None) => true,
            (from, to) => r
                .task
                .due_date
                .is_some_and(|d| from.is_none_or(|f| d >= f) && to.is_none_or(|t| d <= t)),
        })
        .filter(|r| {
            if terms.is_empty() {
                return true;
            }
            let hay = format!(
                "{} {} {} {}",
                r.task.title,
                r.task.description,
                r.project.as_ref().map_or("", |p| p.label.as_str()),
                r.assignee.as_ref().map_or("", |a| a.label.as_str()),
            )
            .to_lowercase();
            terms.iter().all(|t| hay.contains(t))
        })
        .collect()
}

/// Due date first (earliest first, undated last), then priority (1 = highest), then title.
pub fn sort(rows: &mut [TaskRow]) {
    rows.sort_by(|a, b| {
        let (a, b) = (&a.task, &b.task);
        match (a.due_date, b.due_date) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        // Meetings on one day go by the time they start (spec 38).
        .then_with(|| a.start_minute.cmp(&b.start_minute))
        .then_with(|| a.priority.cmp(&b.priority))
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        .then_with(|| a.id.cmp(&b.id))
    });
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Estimates look like 3d or 4h (a plain number means days)")]
pub struct EstimateError;

/// `3d`, `1.5d`, `4h`, or a bare number (days) -> days. Hours convert at `hours_per_day`.
/// Empty input is not an estimate (callers treat it as "clear").
pub fn parse_estimate(text: &str, hours_per_day: f64) -> Result<f64, EstimateError> {
    let t = text.trim().to_lowercase().replace(',', ".");
    let (number, per_unit) = match t.chars().last() {
        Some('d') => (&t[..t.len() - 1], 1.0),
        Some('h') => (&t[..t.len() - 1], 1.0 / hours_per_day),
        Some(c) if c.is_ascii_digit() || c == '.' => (t.as_str(), 1.0),
        _ => return Err(EstimateError),
    };
    let n: f64 = number.trim().parse().map_err(|_| EstimateError)?;
    let days = n * per_unit;
    if !days.is_finite() || days < 0.0 {
        return Err(EstimateError);
    }
    // Round away float noise (e.g. 1h at 3h/day) without losing real precision.
    Ok((days * 10_000.0).round() / 10_000.0)
}

/// One task title per non-empty line; list markers (`-`, `*`, `1.`, `[ ]`) are dropped.
pub fn parse_lines(text: &str) -> Vec<String> {
    text.lines().filter_map(clean_line).collect()
}

fn clean_line(line: &str) -> Option<String> {
    let mut s = line.trim();
    for bullet in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = s.strip_prefix(bullet) {
            s = rest.trim_start();
            break;
        }
    }
    // "1. " or "2) "
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &s[digits..];
        if let Some(rest) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            s = rest.trim_start();
        }
    }
    for checkbox in ["[ ] ", "[x] ", "[X] "] {
        if let Some(rest) = s.strip_prefix(checkbox) {
            s = rest.trim_start();
            break;
        }
    }
    (!s.is_empty()).then(|| s.to_owned())
}

/// What a task can work out from two of its start date, due date and estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Third {
    Due(Date),
    /// In working days.
    Estimate(f64),
}

/// The due date (from the start and the estimate) or the estimate (from the start and the due
/// date) when that one is missing, in working days of `week`, the way the schedule counts them:
/// the start day is the first day of the work and the due date its last, so a start on Monday
/// with 5 days is due on Friday, a part day takes the whole day, and Monday to Friday is 5 days.
/// A missing start is never worked out from the due date and the estimate: a start date means
/// "not before", so it would hold the task back to the last moment instead of letting it start
/// as soon as it can. `None` when there is nothing to work out, or the two that are set give
/// nothing (a due date before the start, an estimate of 0, a week with no working day).
pub fn third(
    start: Option<Date>,
    due: Option<Date>,
    estimate: Option<f64>,
    week: WorkWeek,
) -> Option<Third> {
    if week.days_per_week() == 0 {
        return None;
    }
    // Whole working days, at least one.
    let days = |e: f64| (e > 0.0 && e.is_finite()).then(|| (e.ceil() as i64).max(1));
    match (start, due, estimate) {
        (Some(s), Some(d), None) => {
            let n = end_index(week, d) - working_index(week, s);
            (n > 0).then_some(Third::Estimate(n as f64))
        }
        (Some(s), None, Some(e)) => {
            let n = days(e)?;
            Some(Third::Due(date_of(week, working_index(week, s) + n - 1)))
        }
        _ => None,
    }
}

/// The change that works out one of a task's start date, due date and estimate from the other
/// two, in working days of `week` (the same counting as [`third`]; the start is the latest day
/// the work can begin to end on the due date). A person asked for it, so unlike [`third`] it can
/// give the start. The estimate is set in the same change when a date is, so it is kept as it
/// is. `Err` says in words what is missing or does not fit.
pub fn work_out(
    figure: TaskFigure,
    start: Option<Date>,
    due: Option<Date>,
    estimate: Option<f64>,
    week: WorkWeek,
) -> Result<UpdateTask, String> {
    if week.days_per_week() == 0 {
        return Err("The working week has no working days. Set one in Settings.".to_owned());
    }
    let missing = |what: &str| format!("Needs {what} to work this out. Set them first.");
    let days = |e: Option<f64>| match e {
        Some(e) if e > 0.0 && e.is_finite() => Ok((e.ceil() as i64).max(1)),
        Some(_) => Err("The estimate is 0, so there is nothing to work out from.".to_owned()),
        None => Err(missing("an estimate and a start date or due date")),
    };
    match figure {
        TaskFigure::Estimate => {
            let (Some(s), Some(d)) = (start, due) else {
                return Err(missing("a start date and a due date"));
            };
            let n = end_index(week, d) - working_index(week, s);
            if n <= 0 {
                return Err("The due date is before the start date. Change one of them.".to_owned());
            }
            Ok(UpdateTask {
                estimate_days: Patch::Set(n as f64),
                ..Default::default()
            })
        }
        TaskFigure::Due => {
            let Some(s) = start else {
                return Err(missing("a start date and an estimate"));
            };
            let n = days(estimate)?;
            Ok(UpdateTask {
                due_date: Patch::Set(date_of(week, working_index(week, s) + n - 1)),
                estimate_days: estimate.map_or(Patch::Keep, Patch::Set),
                ..Default::default()
            })
        }
        TaskFigure::Start => {
            let Some(d) = due else {
                return Err(missing("a due date and an estimate"));
            };
            let n = days(estimate)?;
            Ok(UpdateTask {
                start_date: Patch::Set(date_of(week, end_index(week, d) - n)),
                estimate_days: estimate.map_or(Patch::Keep, Patch::Set),
                ..Default::default()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Task, Uuid};
    use proptest::prelude::*;
    use time::{macros::date, Date, OffsetDateTime};

    fn blocks(from: u128, to: u128) -> Edge {
        Edge {
            id: Uuid::from_u128(1000 + from * 10 + to),
            edge_type: EdgeType::Blocks,
            from_type: NodeType::Task,
            from_id: Uuid::from_u128(from),
            to_type: NodeType::Task,
            to_id: Uuid::from_u128(to),
            attrs: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    #[test]
    fn tasks_on_the_backburner_are_hidden_only_when_the_filter_says_so() {
        let mut parked = row(2, "Parked", TaskStatus::Todo, 3, None);
        parked.task.backburner_since = Some(date!(2027 - 03 - 01));
        let rows = vec![row(1, "Live", TaskStatus::Todo, 3, None), parked];
        let ids = |hide| -> Vec<u128> {
            filter(
                rows.clone(),
                &TaskFilter {
                    hide_parked: hide,
                    ..Default::default()
                },
            )
            .iter()
            .map(|r| r.task.id.as_u128())
            .collect()
        };
        assert_eq!(ids(true), vec![1]);
        // Everything else that asks for tasks (pickers, search, links) still sees them.
        assert_eq!(ids(false), vec![1, 2]);
    }

    #[test]
    fn what_waits_on_a_task_is_the_open_unparked_tasks_it_blocks() {
        let mut parked = row(4, "Parked", TaskStatus::Todo, 3, None).task;
        parked.backburner_since = Some(date!(2027 - 03 - 01));
        let tasks = vec![
            row(1, "Blocker", TaskStatus::Todo, 3, None).task,
            row(2, "Waits", TaskStatus::Todo, 3, None).task,
            row(3, "Finished", TaskStatus::Done, 3, None).task,
            parked,
            row(5, "Unrelated", TaskStatus::Todo, 3, None).task,
        ];
        let mut gone = blocks(1, 5);
        gone.archived_at = Some(OffsetDateTime::UNIX_EPOCH);
        let edges = vec![blocks(1, 2), blocks(1, 3), blocks(1, 4), gone, blocks(2, 5)];
        assert_eq!(
            held_up_by(Uuid::from_u128(1), &tasks, &edges),
            [Uuid::from_u128(2)]
        );
        assert!(held_up_by(Uuid::from_u128(5), &tasks, &edges).is_empty());
    }

    fn summary(t: NodeType, n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(t, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn row(n: u128, title: &str, status: TaskStatus, priority: u8, due: Option<Date>) -> TaskRow {
        TaskRow {
            waiting: None,
            task: Task {
                order_step: None,
                confirmed_on: None,

                links: Vec::new(),
                task_type: None,
                focus: None,
                backburner_since: None,
                start_minute: None,
                length_minutes: None,
                id: Uuid::from_u128(n),
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
            link_count: 0,
            attachment_count: 0,
            parent: None,
            subtask_count: 0,
            subtasks_done: 0,
        }
    }

    fn titles(rows: &[TaskRow]) -> Vec<&str> {
        rows.iter().map(|r| r.task.title.as_str()).collect()
    }

    #[test]
    fn estimates() {
        assert_eq!(parse_estimate("3d", 8.0), Ok(3.0));
        assert_eq!(parse_estimate(" 1.5D ", 8.0), Ok(1.5));
        assert_eq!(parse_estimate("1,5d", 8.0), Ok(1.5));
        assert_eq!(parse_estimate("2", 8.0), Ok(2.0)); // bare number = days
        assert_eq!(parse_estimate("4h", 8.0), Ok(0.5));
        assert_eq!(parse_estimate("4h", 6.0), Ok(0.6667)); // the hours-per-day setting
        assert_eq!(parse_estimate("0d", 8.0), Ok(0.0));
        for bad in [
            "", "d", "h", "abc", "-1d", "1x", "3 days", "1d2", "inf", "NaNd",
        ] {
            assert_eq!(parse_estimate(bad, 8.0), Err(EstimateError), "{bad:?}");
        }
    }

    #[test]
    fn pasted_lists_become_titles() {
        let text = "Write spec\n\n  - Review budget  \n* Call Raj\n1. First\n2) Second\n[ ] Send deck\n- [x] Old thing\r\n   \nPlain";
        assert_eq!(
            parse_lines(text),
            vec![
                "Write spec",
                "Review budget",
                "Call Raj",
                "First",
                "Second",
                "Send deck",
                "Old thing",
                "Plain"
            ]
        );
        assert!(parse_lines("  \n\n").is_empty());
        // A number that is part of the title stays.
        assert_eq!(parse_lines("2024 planning"), vec!["2024 planning"]);
        assert_eq!(parse_lines("3.5 hours of work"), vec!["3.5 hours of work"]);
    }

    #[test]
    fn sorts_by_due_then_priority_then_title() {
        let mut rows = vec![
            row(1, "undated p1", TaskStatus::Todo, 1, None),
            row(2, "late", TaskStatus::Todo, 1, Some(date!(2027 - 02 - 01))),
            row(
                3,
                "early p3",
                TaskStatus::Todo,
                3,
                Some(date!(2027 - 01 - 01)),
            ),
            row(
                4,
                "early p1",
                TaskStatus::Todo,
                1,
                Some(date!(2027 - 01 - 01)),
            ),
            row(
                5,
                "Early p1 b",
                TaskStatus::Todo,
                1,
                Some(date!(2027 - 01 - 01)),
            ),
        ];
        sort(&mut rows);
        assert_eq!(
            titles(&rows),
            vec!["early p1", "Early p1 b", "early p3", "late", "undated p1"]
        );
    }

    #[test]
    fn filters() {
        let mut a = row(
            1,
            "Fix login",
            TaskStatus::Todo,
            1,
            Some(date!(2027 - 01 - 10)),
        );
        a.task.project_id = Some(Uuid::from_u128(100));
        a.project = Some(summary(NodeType::Project, 100, "API Launch"));
        a.assignee = Some(summary(NodeType::Person, 200, "Priya"));
        let b = row(2, "Write docs", TaskStatus::Done, 2, None);
        let mut c = row(
            3,
            "Plan trip",
            TaskStatus::Cancelled,
            2,
            Some(date!(2027 - 03 - 01)),
        );
        c.task.description = "flights and hotel".into();
        let rows = vec![a, b, c];
        let ids = |f: &TaskFilter| -> Vec<u128> {
            filter(rows.clone(), f)
                .iter()
                .map(|r| r.task.id.as_u128())
                .collect()
        };

        // Closed tasks are hidden unless asked for.
        assert_eq!(ids(&TaskFilter::default()), vec![1]);
        assert_eq!(
            ids(&TaskFilter {
                include_closed: true,
                ..Default::default()
            }),
            vec![1, 2, 3]
        );
        // An explicit status shows it even when closed.
        assert_eq!(
            ids(&TaskFilter {
                status: Some(TaskStatus::Done),
                ..Default::default()
            }),
            vec![2]
        );
        // Inbox: no project (and open).
        assert!(ids(&TaskFilter {
            no_project: true,
            ..Default::default()
        })
        .is_empty());
        assert_eq!(
            ids(&TaskFilter {
                no_project: true,
                include_closed: true,
                ..Default::default()
            }),
            vec![2, 3]
        );
        assert_eq!(
            ids(&TaskFilter {
                project_id: Some(Uuid::from_u128(100)),
                ..Default::default()
            }),
            vec![1]
        );
        assert_eq!(
            ids(&TaskFilter {
                assignee_id: Some(Uuid::from_u128(200)),
                include_closed: true,
                ..Default::default()
            }),
            vec![1]
        );
        // Due range is inclusive and skips undated tasks.
        let range = |from, to| TaskFilter {
            due_from: from,
            due_to: to,
            include_closed: true,
            ..Default::default()
        };
        assert_eq!(
            ids(&range(
                Some(date!(2027 - 01 - 10)),
                Some(date!(2027 - 01 - 10))
            )),
            vec![1]
        );
        assert_eq!(ids(&range(Some(date!(2027 - 02 - 01)), None)), vec![3]);
        assert_eq!(ids(&range(None, Some(date!(2027 - 12 - 31)))), vec![1, 3]);
        // Text: every word, any of title/description/project/assignee, any case.
        let text = |t: &str| TaskFilter {
            text: Some(t.into()),
            include_closed: true,
            ..Default::default()
        };
        assert_eq!(ids(&text("LOGIN priya")), vec![1]);
        assert_eq!(ids(&text("api")), vec![1]);
        assert_eq!(ids(&text("hotel")), vec![3]);
        assert!(ids(&text("login hotel")).is_empty());
        assert_eq!(ids(&text("   ")), vec![1, 2, 3]);
    }

    proptest! {
        /// Parsing never panics, and anything it accepts is a finite non-negative number of days.
        #[test]
        fn parse_estimate_is_total(text in ".{0,12}", hours in 1.0f64..24.0) {
            if let Ok(days) = parse_estimate(&text, hours) {
                prop_assert!(days.is_finite() && days >= 0.0);
            }
        }

        /// Whole days and hours round-trip through the setting.
        #[test]
        fn hours_convert_back(h in 0u32..200, per_day in 1u32..=24) {
            let days = parse_estimate(&format!("{h}h"), f64::from(per_day)).unwrap();
            prop_assert!((days * f64::from(per_day) - f64::from(h)).abs() < 0.01);
        }

        #[test]
        fn parse_lines_never_returns_blank_titles(text in "(?s).{0,200}") {
            for t in parse_lines(&text) {
                prop_assert!(!t.trim().is_empty() && !t.contains('\n'));
            }
        }
    }

    #[test]
    fn any_two_of_start_due_and_estimate_give_the_third() {
        let week = WorkWeek::MON_FRI;
        let mon = date!(2027 - 03 - 01);
        let fri = date!(2027 - 03 - 05);
        // Start and due: the working days from the first to the last, both counted.
        assert_eq!(
            third(Some(mon), Some(fri), None, week),
            Some(Third::Estimate(5.0))
        );
        assert_eq!(
            third(Some(mon), Some(mon), None, week),
            Some(Third::Estimate(1.0))
        );
        // Fri 9 Oct to Fri 23 Oct 2026, over two weekends: 11 working days.
        assert_eq!(
            third(
                Some(date!(2026 - 10 - 09)),
                Some(date!(2026 - 10 - 23)),
                None,
                week
            ),
            Some(Third::Estimate(11.0))
        );
        // Start and estimate: the last working day of the work.
        assert_eq!(
            third(Some(mon), None, Some(5.0), week),
            Some(Third::Due(fri))
        );
        assert_eq!(
            third(Some(fri), None, Some(2.0), week),
            Some(Third::Due(date!(2027 - 03 - 08)))
        );
        // A part day takes the whole day.
        assert_eq!(
            third(Some(mon), None, Some(0.5), week),
            Some(Third::Due(mon))
        );
        assert_eq!(
            third(Some(mon), None, Some(2.5), week),
            Some(Third::Due(date!(2027 - 03 - 03)))
        );
        // Due and estimate leave the start empty: the task starts as soon as it can.
        assert_eq!(third(None, Some(fri), Some(5.0), week), None);
        // A start on a Sunday begins on the Monday.
        assert_eq!(
            third(Some(date!(2027 - 02 - 28)), None, Some(1.0), week),
            Some(Third::Due(mon))
        );
        // Nothing to work out, or nothing sensible.
        assert_eq!(third(Some(mon), Some(fri), Some(2.0), week), None);
        assert_eq!(third(Some(mon), None, None, week), None);
        assert_eq!(third(Some(fri), Some(mon), None, week), None);
        assert_eq!(third(Some(mon), None, Some(0.0), week), None);
        let sat = date!(2027 - 03 - 06);
        assert_eq!(
            third(Some(sat), Some(date!(2027 - 03 - 07)), None, week),
            None
        );
    }

    proptest! {
        #[test]
        fn the_third_agrees_with_the_other_two(start in 0i64..2000, days in 1i64..60) {
            let week = WorkWeek::MON_FRI;
            let s = date!(2026 - 01 - 05) + time::Duration::days(start);
            let Some(Third::Due(d)) = third(Some(s), None, Some(days as f64), week) else {
                return Err(TestCaseError::fail("no due date"));
            };
            prop_assert!(d >= s);
            // The two dates give the estimate again.
            prop_assert_eq!(third(Some(s), Some(d), None, week), Some(Third::Estimate(days as f64)));
        }
    }

    #[test]
    fn work_out_gives_the_missing_figure_in_working_days() {
        use time::macros::date;
        let week = WorkWeek::MON_FRI;
        let (mon, fri) = (date!(2027 - 03 - 01), date!(2027 - 03 - 05));
        let p = work_out(TaskFigure::Estimate, Some(mon), Some(fri), Some(2.0), week).unwrap();
        assert_eq!(p.estimate_days, Patch::Set(5.0));
        let p = work_out(TaskFigure::Due, Some(mon), None, Some(5.0), week).unwrap();
        assert_eq!(p.due_date, Patch::Set(fri));
        // The estimate is set again so a date change does not re-count it (1.5 days stay 1.5).
        let p = work_out(TaskFigure::Due, Some(mon), Some(mon), Some(1.5), week).unwrap();
        assert_eq!(
            (p.due_date, p.estimate_days),
            (Patch::Set(date!(2027 - 03 - 02)), Patch::Set(1.5))
        );
        // The latest start that still ends on the due date; over a weekend too.
        let p = work_out(TaskFigure::Start, None, Some(fri), Some(5.0), week).unwrap();
        assert_eq!(p.start_date, Patch::Set(mon));
        let p = work_out(
            TaskFigure::Start,
            None,
            Some(date!(2027 - 03 - 08)),
            Some(2.0),
            week,
        )
        .unwrap();
        assert_eq!(p.start_date, Patch::Set(fri));
        // What is missing or does not fit is said in words.
        assert!(work_out(TaskFigure::Estimate, Some(fri), Some(mon), None, week).is_err());
        assert!(work_out(TaskFigure::Estimate, None, Some(fri), None, week).is_err());
        assert!(work_out(TaskFigure::Due, None, None, Some(2.0), week).is_err());
        assert!(work_out(TaskFigure::Start, None, Some(fri), Some(0.0), week).is_err());
    }
}
