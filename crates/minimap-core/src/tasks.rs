//! Task list rules (filtering, ordering), estimate parsing and pasted-list parsing.

use std::cmp::Ordering;

use minimap_types::{TaskFilter, TaskRow, TaskStatus};

pub fn is_closed(s: TaskStatus) -> bool {
    matches!(s, TaskStatus::Done | TaskStatus::Cancelled)
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
        .filter(|r| !f.no_project || r.task.project_id.is_none())
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

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Task, Uuid};
    use proptest::prelude::*;
    use time::{macros::date, Date, OffsetDateTime};

    fn summary(t: NodeType, n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(t, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn row(n: u128, title: &str, status: TaskStatus, priority: u8, due: Option<Date>) -> TaskRow {
        TaskRow {
            parent: None,
            subtasks: Default::default(),
            task: Task {
                links: Vec::new(),
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
}
