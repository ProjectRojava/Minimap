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

/// Why a task is a red flag on This week, most serious first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    /// Its due date has passed and it is still open.
    Overdue,
    /// Due today and still open.
    DueToday,
    /// Its status is Blocked.
    Blocked,
}

/// A red-flag task: what is wrong with it (most serious first; a task can be overdue and blocked).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlaggedTask {
    pub task: WeekTask,
    pub flags: Vec<Flag>,
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
    /// "Today" for this view: the real today, or the past day chosen with `as_of`.
    pub today: Date,
    /// Set when the screen shows the week as it stood on a past day (`today` is that day).
    #[serde(default)]
    pub as_of: Option<Date>,
    /// The real today (it differs from `today` when `as_of` is set).
    pub real_today: Date,
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
    /// Every task that is overdue, due today or blocked, once each: overdue first (most days
    /// late first), then due today, then blocked; then priority.
    #[serde(default)]
    pub attention: Vec<FlaggedTask>,
    /// The week's plan without the red flags: open tasks due later this week and my tasks in
    /// progress, once each, most important first (priority, then due date, undated last).
    #[serde(default)]
    pub priorities: Vec<WeekTask>,
}

/// Where a task id was last seen is irrelevant; kept so the UI can key rows.
impl WeekTask {
    pub fn id(&self) -> Uuid {
        self.row.task.id
    }
}
