//! Schedule and critical path (spec 13). All times are **working days** (Mon-Fri unless Settings say otherwise) counted from
//! today's working day: offset 0 is today (or the next Monday when today is a weekend), 1 the
//! next working day, negative offsets are the past. A task spanning `es..ef` occupies those
//! working days; `ef` is the end of the last one.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::TaskStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleScope {
    /// Every open task, each project against its own target or projected finish.
    Portfolio,
    Project(Uuid),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub title: String,
    pub project_id: Option<Uuid>,
    pub project_title: Option<String>,
    pub status: TaskStatus,
    /// Finished: fixed at its actual dates, never critical.
    pub done: bool,
    /// No estimate, so it was scheduled as one day.
    pub unestimated: bool,
    pub duration_days: f64,
    /// Earliest start / finish, as working-day offsets and as dates (the first and last
    /// working day it occupies).
    pub es: f64,
    pub ef: f64,
    pub start: Date,
    pub finish: Date,
    /// Latest start / finish that still meets the deadline (the target date, or the project's
    /// projected finish when there is none).
    pub ls: f64,
    pub lf: f64,
    pub latest_start: Date,
    pub latest_finish: Date,
    /// `ls - es`. Zero when it can't move without moving the finish; **negative** when the
    /// target date can't be met.
    pub slack_days: f64,
    /// Among the tasks that decide the project's finish (the least slack in the project).
    pub critical: bool,
    /// Working days late, when slack is negative.
    pub late_by_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectForecast {
    pub project_id: Uuid,
    pub title: String,
    /// When the last open task is expected to finish (the last finish when all are done).
    pub projected_finish: Option<Date>,
    pub finish_offset: Option<f64>,
    pub target_date: Option<Date>,
    /// End of the target day, as an offset (for drawing).
    pub target_offset: Option<f64>,
    /// Working days the projected finish is past the target (only when it is).
    pub late_by_days: Option<u32>,
    pub open_tasks: u32,
    pub unestimated_tasks: u32,
    pub critical_tasks: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub scope: ScheduleScope,
    pub today: Date,
    /// Offset of `days[0]`.
    pub first_offset: i64,
    /// The working days covering every bar, today and the target: `days[i]` has offset
    /// `first_offset + i`. Lets a timeline label its axis without date arithmetic.
    pub days: Vec<Date>,
    /// Ordered by earliest start.
    pub tasks: Vec<ScheduledTask>,
    pub projects: Vec<ProjectForecast>,
}
