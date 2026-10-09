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
    /// Open tasks due that day (meetings are counted apart).
    pub tasks_due: u32,
    /// Open meetings that day (spec 38).
    #[serde(default)]
    pub meetings: u32,
    /// Open waiting-ons expected that day.
    pub waiting_expected: u32,
    /// 1:1 notes dated that day.
    pub one_on_ones: u32,
}

/// One of a task's latest notes as This week shows it: the day, and a short read of the text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskNote {
    pub id: Uuid,
    pub note_date: Date,
    pub kind: crate::NoteKind,
    /// The first lines of the note with markup stripped and the closing "About @task" line
    /// left out, as one run of text.
    pub snippet: String,
    /// The start of the note's Markdown (the closing "About @task" line left out, cut at
    /// `PREVIEW_CHARS`), for the screen to render.
    #[serde(default)]
    pub body: String,
}

/// How much of a note's Markdown This week receives.
pub const PREVIEW_CHARS: usize = 600;

/// The latest notes that mention a task (newest first, at most `RECENT_NOTES`) and how many
/// there are in all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskNotes {
    pub task: Uuid,
    pub total: u32,
    pub notes: Vec<TaskNote>,
}

/// How many notes This week shows under each task.
pub const RECENT_NOTES: usize = 3;

/// A task in focus (spec 37) that has no red flag, so it is shown in the Focus section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusTask {
    pub task: WeekTask,
    /// Days since the task, or a note about it, last changed (0 = today). The Focus section
    /// asks "still the one?" once this reaches `FOCUS_QUIET_DAYS`.
    pub quiet_days: u32,
}

/// More tasks in focus than this and This week suggests choosing: a highlight that holds
/// everything highlights nothing.
pub const FOCUS_SOFT_LIMIT: usize = 5;

/// A task in focus that has not changed, and has had no note, for this many days is asked about.
pub const FOCUS_QUIET_DAYS: u32 = 14;

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
    /// The latest notes of each task in `attention`, `focus` and `priorities` that has any (notes dated
    /// after the day shown are left out when looking back).
    #[serde(default)]
    pub task_notes: Vec<TaskNotes>,
    /// Open tasks in focus that are not red flags (spec 37), most important first (priority,
    /// then due date, undated last). A focused task that is overdue, due today or blocked is in
    /// `attention` instead.
    #[serde(default)]
    pub focus: Vec<FocusTask>,
    /// Every open task in focus today, flagged or not (for the "too many" nudge).
    #[serde(default)]
    pub focus_count: u32,
    /// Open meetings (to do or in progress) from today through Sunday, by day then start time
    /// (spec 38). Meetings are not tasks to chase: they are in none of the other lists, and are
    /// never overdue or "due today" flags; the clock closes them when they end.
    #[serde(default)]
    pub meetings: Vec<WeekTask>,
}

/// Where a task id was last seen is irrelevant; kept so the UI can key rows.
impl WeekTask {
    pub fn id(&self) -> Uuid {
        self.row.task.id
    }
}
