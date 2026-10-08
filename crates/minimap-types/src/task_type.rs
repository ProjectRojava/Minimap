//! Task types (spec 32): the kind of work a task is (design, decision, bug ...). The list is the
//! user's own, kept in Settings; each task points at one entry by its `id`.
//!
//! Also the "planned against actual" reading of a task's dates: when it was due, when it was
//! finished and how often the due date moved (the activity log has the history).

use serde::{Deserialize, Serialize};
use time::Date;

use crate::{timefmt, Activity};

/// One entry of the task-type list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskType {
    /// Stable handle that tasks store (`design`, `legal-review`). Made from the name when the
    /// type is added (leave it empty in a new entry) and never changed by a rename.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Colour as a hue, 0-359; the editor offers `TASK_TYPE_HUES`.
    pub hue: u16,
    /// A retired type: kept on the tasks that have it, not offered for new ones.
    #[serde(default)]
    pub archived: bool,
}

/// The colours the editor offers: far apart, the same set objectives use.
pub const TASK_TYPE_HUES: [u16; 8] = [215, 25, 145, 285, 340, 75, 180, 250];

/// At most this many types (archived ones count).
pub const MAX_TASK_TYPES: usize = 40;
/// At most this many characters in a type's name.
pub const MAX_TASK_TYPE_NAME: usize = 30;

/// The list a new installation starts with, until it is edited.
pub fn default_task_types() -> Vec<TaskType> {
    [
        ("design", "Design", 285),
        ("build", "Build", 215),
        ("decision", "Decision", 25),
        ("review", "Review", 250),
        ("research", "Research", 180),
        ("bug", "Bug", 340),
        ("admin", "Admin", 145),
    ]
    .into_iter()
    .map(|(id, name, hue)| TaskType {
        id: id.to_owned(),
        name: name.to_owned(),
        hue,
        archived: false,
    })
    .collect()
}

impl TaskType {
    /// The type of `id` in `types`, archived or not.
    pub fn find<'a>(types: &'a [TaskType], id: &str) -> Option<&'a TaskType> {
        types.iter().find(|t| t.id == id)
    }
}

// ------------------------------------------------------- planned against actual

/// A finished task against its due date, in calendar days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Early(i64),
    OnTime,
    Late(i64),
}

/// How a task finished against the date it was planned for.
pub fn finish_timing(planned: Date, finished: Date) -> Finish {
    match (finished - planned).whole_days() {
        0 => Finish::OnTime,
        d if d < 0 => Finish::Early(-d),
        d => Finish::Late(d),
    }
}

impl Finish {
    /// "on time", "2 days late", "1 day early".
    pub fn text(self) -> String {
        match self {
            Finish::OnTime => "on time".to_owned(),
            Finish::Early(d) => format!("{} early", days(d)),
            Finish::Late(d) => format!("{} late", days(d)),
        }
    }
}

fn days(n: i64) -> String {
    if n == 1 {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// What the activity log says about a task's due date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanHistory {
    /// The first due date the task had, if it ever had one.
    pub first: Option<Date>,
    /// How many times a date that was set was changed to another one (earlier or later).
    pub moves: u32,
}

/// Reads `due_date` changes out of a task's activity rows (any order).
pub fn plan_history(rows: &[Activity]) -> PlanHistory {
    let mut rows: Vec<&Activity> = rows.iter().collect();
    rows.sort_by_key(|r| r.at);
    let mut out = PlanHistory::default();
    for row in rows {
        let Some(pair) = row.diff.get("due_date").and_then(|v| v.as_array()) else {
            continue;
        };
        let date = |i: usize| {
            pair.get(i)
                .and_then(|v| v.as_str())
                .and_then(|s| timefmt::parse_date(s).ok())
        };
        let (old, new) = (date(0), date(1));
        if out.first.is_none() {
            out.first = old.or(new);
        }
        if let (Some(old), Some(new)) = (old, new) {
            if old != new {
                out.moves += 1;
            }
        }
    }
    out
}

impl PlanHistory {
    /// A finished task against the first date it was planned for (only once the date has moved;
    /// before that the current due date is the first one).
    pub fn first_timing(&self, finished: Date) -> Option<Finish> {
        let first = self.first.filter(|_| self.moves > 0)?;
        Some(finish_timing(first, finished))
    }

    /// "Originally planned for <date>, moved twice" once the date has moved, else nothing.
    pub fn moved_text(&self, show: impl Fn(Date) -> String) -> Option<String> {
        let first = self.first?;
        if self.moves == 0 {
            return None;
        }
        let times = match self.moves {
            1 => "moved once".to_owned(),
            2 => "moved twice".to_owned(),
            n => format!("moved {n} times"),
        };
        Some(format!("Originally planned for {}, {times}", show(first)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityAction, NodeType};
    use serde_json::json;
    use time::macros::date;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn row(secs: i64, diff: serde_json::Value) -> Activity {
        Activity {
            id: Uuid::nil(),
            at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(secs),
            node_type: NodeType::Task,
            node_id: Uuid::nil(),
            action: ActivityAction::Updated,
            diff,
        }
    }

    #[test]
    fn defaults_have_unique_ids_and_names_and_known_hues() {
        let types = default_task_types();
        assert_eq!(types.len(), 7);
        for (i, a) in types.iter().enumerate() {
            assert!(
                TASK_TYPE_HUES.contains(&a.hue),
                "{} uses an odd hue",
                a.name
            );
            assert!(!a.archived);
            for b in &types[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.name, b.name);
            }
        }
        assert!(TaskType::find(&types, "decision").is_some());
        assert!(TaskType::find(&types, "nope").is_none());
    }

    #[test]
    fn finishing_is_early_on_time_or_late() {
        let due = date!(2026 - 10 - 12);
        assert_eq!(finish_timing(due, date!(2026 - 10 - 12)), Finish::OnTime);
        assert_eq!(finish_timing(due, date!(2026 - 10 - 14)), Finish::Late(2));
        assert_eq!(finish_timing(due, date!(2026 - 10 - 11)), Finish::Early(1));
        assert_eq!(Finish::Late(2).text(), "2 days late");
        assert_eq!(Finish::Early(1).text(), "1 day early");
        assert_eq!(Finish::OnTime.text(), "on time");
    }

    #[test]
    fn the_first_date_and_the_moves_come_from_the_log() {
        let rows = vec![
            row(30, json!({"due_date": ["2026-10-19", "2026-10-26"]})),
            row(10, json!({"due_date": [null, "2026-10-12"]})),
            row(
                20,
                json!({"due_date": ["2026-10-12", "2026-10-19"], "title": ["a", "b"]}),
            ),
            row(40, json!({"priority": [3, 2]})),
        ];
        let h = plan_history(&rows);
        assert_eq!(h.first, Some(date!(2026 - 10 - 12)));
        assert_eq!(h.moves, 2);
        assert_eq!(
            h.moved_text(timefmt::fmt_date).as_deref(),
            Some("Originally planned for 2026-10-12, moved twice")
        );
        assert_eq!(
            h.first_timing(date!(2026 - 10 - 27)),
            Some(Finish::Late(15))
        );
    }

    #[test]
    fn a_date_set_once_or_cleared_is_not_a_move() {
        let set_late = vec![row(1, json!({"due_date": [null, "2026-10-12"]}))];
        assert_eq!(plan_history(&set_late).moves, 0);
        assert!(plan_history(&set_late)
            .first_timing(date!(2026 - 10 - 20))
            .is_none());
        assert!(plan_history(&set_late)
            .moved_text(|_| String::new())
            .is_none());
        let cleared = vec![
            row(1, json!({"due_date": [null, "2026-10-12"]})),
            row(2, json!({"due_date": ["2026-10-12", null]})),
        ];
        assert_eq!(plan_history(&cleared).moves, 0);
        assert_eq!(plan_history(&[]), PlanHistory::default());
    }

    #[test]
    fn a_task_whose_log_starts_mid_way_takes_the_old_date_as_the_plan() {
        let rows = vec![row(1, json!({"due_date": ["2026-10-05", "2026-10-09"]}))];
        let h = plan_history(&rows);
        assert_eq!(h.first, Some(date!(2026 - 10 - 05)));
        assert_eq!(h.moves, 1);
    }
}
