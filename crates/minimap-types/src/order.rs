//! The order of work (spec 60, ADR-0024): one person's tasks as steps, top to bottom.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{NodeSummary, TaskStatus};

/// A task as the Order screen shows it, with the dates the schedule gives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderCard {
    pub id: Uuid,
    pub title: String,
    pub project: Option<NodeSummary>,
    pub status: TaskStatus,
    pub priority: u8,
    #[serde(default)]
    pub task_type: Option<String>,
    pub estimate_days: Option<f64>,
    /// When the schedule starts and finishes it (with the order counted).
    pub start: Option<Date>,
    pub finish: Option<Date>,
    pub due: Option<Date>,
    /// The finish is after the due date.
    pub past_due: bool,
    /// Already in progress: it keeps its own start.
    pub started: bool,
    /// How much of the person's time it takes (`allocation_pct`, 100 when not set).
    pub allocation_pct: u32,
}

/// One step: the tasks done at the same time, and when the step runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderStep {
    /// 1 = done first.
    pub step: u32,
    pub start: Option<Date>,
    pub finish: Option<Date>,
    /// The allocations of the step's tasks added up (over 100 = more than one person's time).
    pub load_pct: u32,
    pub cards: Vec<OrderCard>,
}

/// A person's order of work, and their open tasks that are not in it yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkOrder {
    pub person: NodeSummary,
    pub steps: Vec<OrderStep>,
    /// Open tasks assigned to the person that are not in the order: due date first, then
    /// priority, then title.
    pub unordered: Vec<OrderCard>,
    /// Parts of the order the schedule can't follow because a `blocks` link says otherwise, in
    /// words.
    pub conflicts: Vec<String>,
    /// The working days the Order screen draws its bars on (spec 61): 15 of them, from the
    /// earlier of today and the first start, never more than 5 working days back.
    #[serde(default)]
    pub ruler: Vec<Date>,
    /// The day the dates were worked out for.
    #[serde(default)]
    pub today: Option<Date>,
}

/// One column of *My team* on the Order screen (spec 61): a person's order and this week's load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamMember {
    pub order: WorkOrder,
    pub is_self: bool,
    /// This week's load, as the Capacity screen shows it; `None` without capacity to measure.
    pub load_pct: Option<u32>,
}

/// Everyone in Minimap who isn't archived, each with their order (spec 61).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TeamOrders {
    /// You first, then everyone else by name.
    pub members: Vec<TeamMember>,
}
