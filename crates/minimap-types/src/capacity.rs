//! Capacity (spec 17): how loaded each person is, week by week. Weeks run Monday to Sunday and
//! only the Monday-to-Friday working days carry work.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::NodeSummary;

/// Work one person has in one week, from one task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapacityTask {
    pub id: Uuid,
    pub title: String,
    pub project_title: Option<String>,
    /// Working days of effort in this week: the days the task is scheduled that week, times
    /// the assignment's `allocation_pct`.
    pub days: f64,
    pub allocation_pct: u32,
    /// The whole task's scheduled first and last day.
    pub start: Date,
    pub finish: Date,
    /// No estimate, so it was scheduled as one day: the load is a guess.
    pub unestimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekLoad {
    pub week_start: Date,
    /// Days of work scheduled that week, and the days of capacity (`weekly hours / hours per day`).
    pub load_days: f64,
    pub capacity_days: f64,
    /// `load_days / capacity_days` as a percentage; over 100 is overloaded.
    pub load_pct: f64,
    pub over: bool,
    /// What makes up the load, biggest first.
    pub tasks: Vec<CapacityTask>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonCapacity {
    pub person: NodeSummary,
    pub weekly_capacity_hours: f64,
    /// One entry per week of the window, in order.
    pub weeks: Vec<WeekLoad>,
    /// The heaviest week in the window.
    pub peak_pct: f64,
    pub peak_week: Option<Date>,
    pub over_weeks: u32,
    /// Open tasks assigned to them, scheduled or not.
    pub active_tasks: u32,
    /// More open tasks than the configured limit (the simple flag, independent of the heatmap).
    pub over_task_limit: bool,
    /// Open assigned tasks without an estimate; the load figures treat each as one day.
    pub unestimated_tasks: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capacity {
    pub today: Date,
    /// The Monday of today's week (to mark the current column).
    pub current_week_start: Date,
    /// The Mondays of the weeks shown.
    pub weeks: Vec<Date>,
    /// The first Monday and the last Sunday covered.
    pub from: Date,
    pub to: Date,
    /// Where the previous and next window of the same length start.
    pub prev_from: Date,
    pub next_from: Date,
    /// Overloaded people first (heaviest peak), then by name.
    pub people: Vec<PersonCapacity>,
    /// The "too many open tasks" threshold in effect.
    pub task_limit: u32,
    pub hours_per_day: f64,
    /// Things that limited the picture (e.g. a loop in `blocks` links).
    pub warnings: Vec<String>,
}
