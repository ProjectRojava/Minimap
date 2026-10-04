//! Impact analysis (spec 14): "what if this slips?". A scenario is one or more slips; the
//! report says what each ripples into. Times are working days (see `schedule`).

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{NodeRef, NodeSummary};

/// "This task (or project) runs `days` working days late." A task starts that much later; a
/// project's open tasks all do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slip {
    /// A task or a project.
    pub node: NodeRef,
    pub days: u32,
}

/// A task the slip reaches. `delay_days` is how far its own finish moves; the rest of what
/// arrived (`incoming_days`) was soaked up by slack on the way (`absorbed_days`), so
/// `incoming_days = absorbed_days + delay_days`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactTask {
    pub id: Uuid,
    pub title: String,
    pub project_id: Option<Uuid>,
    pub project_title: Option<String>,
    /// Slipped by the scenario itself (or as part of a slipped project).
    pub direct: bool,
    pub incoming_days: f64,
    pub absorbed_days: f64,
    pub delay_days: f64,
    pub old_start: Date,
    pub new_start: Date,
    pub old_finish: Date,
    pub new_finish: Date,
    pub due_date: Option<Date>,
    /// Working days past the due date before / after.
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
    /// On time before, late after.
    pub newly_late: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactProject {
    pub id: Uuid,
    pub title: String,
    pub direct: bool,
    pub delay_days: f64,
    pub affected_tasks: u32,
    pub old_finish: Option<Date>,
    pub new_finish: Option<Date>,
    pub target_date: Option<Date>,
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
    pub newly_late: bool,
}

/// An objective whose contributors (projects or tasks) finish later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactObjective {
    pub id: Uuid,
    pub title: String,
    pub delay_days: f64,
    pub old_finish: Option<Date>,
    pub new_finish: Option<Date>,
    pub target_date: Option<Date>,
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
    pub newly_late: bool,
    /// The late contributors, by name.
    pub contributors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactPersonTask {
    pub id: Uuid,
    pub title: String,
    pub delay_days: f64,
    pub new_finish: Date,
}

/// Someone whose assigned work moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactPerson {
    pub id: Uuid,
    pub name: String,
    /// The largest delay among their tasks.
    pub delay_days: f64,
    pub tasks: Vec<ImpactPersonTask>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactSummary {
    /// Tasks whose finish moves.
    pub moved_tasks: u32,
    /// Tasks the slip reached but slack absorbed completely.
    pub absorbing_tasks: u32,
    pub newly_late_tasks: u32,
    pub newly_late_projects: u32,
    pub newly_late_objectives: u32,
    pub people: u32,
    /// The largest delay anywhere, in working days.
    pub worst_delay_days: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactReport {
    /// The scenario, named.
    pub slips: Vec<(NodeSummary, u32)>,
    pub summary: ImpactSummary,
    /// Every task the slip reaches, direct ones first then by start date. Includes tasks that
    /// absorbed it completely (delay 0): that is where the slip stops.
    pub tasks: Vec<ImpactTask>,
    pub projects: Vec<ImpactProject>,
    pub objectives: Vec<ImpactObjective>,
    pub people: Vec<ImpactPerson>,
}

/// One date change "apply" would make: the task starts on `start_date` (pinned) and its due
/// date, if it has one, moves to `due_date`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateChange {
    pub task_id: Uuid,
    pub title: String,
    pub old_start: Option<Date>,
    pub new_start: Date,
    pub old_due: Option<Date>,
    pub new_due: Option<Date>,
}

/// What recording the scenario in the plan would change; shown before it is applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyPreview {
    pub changes: Vec<DateChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyResult {
    pub tasks_changed: u32,
}
