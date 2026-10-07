//! "This week" (spec 16): the landing screen. Weeks run Monday to Sunday.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{NodeSummary, NoteRow, ReviewDue, TaskRow, WaitingOnRow};

/// A task in one of the week's sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekTask {
    pub row: TaskRow,
    /// Calendar days past the due date, when overdue.
    pub overdue_days: Option<u32>,
    /// Open tasks that block this one.
    pub blocked_by: Vec<NodeSummary>,
}

/// One day of the week strip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekDay {
    pub date: Date,
    pub is_today: bool,
    /// Open tasks due that day.
    pub tasks_due: u32,
    /// Open waiting-ons expected that day.
    pub waiting_expected: u32,
    /// 1:1 notes dated that day.
    pub one_on_ones: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThisWeek {
    /// The Monday and Sunday of the week shown.
    pub week_start: Date,
    pub week_end: Date,
    /// The Mondays of the weeks before and after, for the previous/next buttons.
    pub prev_week_start: Date,
    pub next_week_start: Date,
    pub today: Date,
    /// The week contains today.
    pub is_current_week: bool,
    /// Whether a "me" person exists (for "my tasks in progress").
    pub has_self: bool,
    pub days: Vec<WeekDay>,
    /// Open tasks due before today, oldest first.
    pub overdue: Vec<WeekTask>,
    /// Open tasks due from today through Sunday (all the week's days when it is a future week).
    pub due_this_week: Vec<WeekTask>,
    /// Open tasks with status Blocked, and what blocks them.
    pub blocked: Vec<WeekTask>,
    /// My tasks in progress.
    pub in_progress: Vec<WeekTask>,
    /// Open waiting-ons that are stale, overdue, or expected by Sunday (snoozed ones excluded).
    pub waiting: Vec<WaitingOnRow>,
    /// 1:1 notes dated this week, earliest first.
    pub one_on_ones: Vec<NoteRow>,
    /// Ongoing objectives whose review is overdue or falls by Sunday, most overdue first.
    #[serde(default)]
    pub reviews: Vec<ReviewDue>,
}

/// Where a task id was last seen is irrelevant; kept so the UI can key rows.
impl WeekTask {
    pub fn id(&self) -> Uuid {
        self.row.task.id
    }
}
