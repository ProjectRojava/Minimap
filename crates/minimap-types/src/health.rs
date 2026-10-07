//! Health scoring and the portfolio overview (spec 15).

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{NodeSummary, ObjectiveStatus, ProjectStatus, WaitingOnRow};

/// `Idle` = not scored (done, cancelled, paused, nothing open or linked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Green,
    Amber,
    Red,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReason {
    /// How bad this reason is on its own (`Green` = good news or context).
    pub level: HealthLevel,
    pub text: String,
}

/// Level, a 0-100 score (100 = healthy; level and score always agree: above 60 green, above 25
/// amber, otherwise red) and the reasons, worst first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub level: HealthLevel,
    pub score: u8,
    pub reasons: Vec<HealthReason>,
}

/// When a signal turns a project amber or red. Configurable in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthThresholds {
    /// Projected this many working days past the target.
    pub late_amber_days: u32,
    pub late_red_days: u32,
    /// Share (percent) of open tasks that are blocked or overdue.
    pub risky_amber_pct: u32,
    pub risky_red_pct: u32,
    /// Share (percent) of open tasks without an estimate.
    pub unestimated_amber_pct: u32,
    pub unestimated_red_pct: u32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            late_amber_days: 1,
            late_red_days: 5,
            risky_amber_pct: 15,
            risky_red_pct: 35,
            unestimated_amber_pct: 50,
            unestimated_red_pct: 80,
        }
    }
}

impl HealthThresholds {
    /// Amber must not be above red, days are 1-365 and shares 1-100.
    pub fn validate(&self) -> Result<(), String> {
        let pair = |name: &str, amber: u32, red: u32, max: u32| -> Result<(), String> {
            if amber < 1 || red > max || amber > max {
                return Err(format!("{name} must be between 1 and {max}"));
            }
            if amber > red {
                return Err(format!("{name}: amber can't be higher than red"));
            }
            Ok(())
        };
        pair(
            "Lateness (days)",
            self.late_amber_days,
            self.late_red_days,
            365,
        )?;
        pair(
            "Blocked or overdue (%)",
            self.risky_amber_pct,
            self.risky_red_pct,
            100,
        )?;
        pair(
            "Unestimated (%)",
            self.unestimated_amber_pct,
            self.unestimated_red_pct,
            100,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskKind {
    Project,
    Task,
}

/// One entry of "top risks".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskItem {
    pub kind: RiskKind,
    pub node: NodeSummary,
    pub health: Health,
    /// Higher = more urgent: `(100 - health score) x importance x scope`.
    pub risk_score: f64,
    /// The priority it was weighed at: its own, or that of the highest-priority objective it
    /// feeds when that is higher.
    pub priority: u8,
    /// Set when the priority came from an objective.
    pub via_objective: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectHealthRow {
    pub project: NodeSummary,
    pub status: ProjectStatus,
    pub priority: u8,
    pub health: Health,
    pub projected_finish: Option<Date>,
    pub target_date: Option<Date>,
    pub open_tasks: u32,
    pub overdue_tasks: u32,
    pub blocked_tasks: u32,
    pub unestimated_tasks: u32,
    /// The `contributes_to` weight when nested under an objective.
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveHealthRow {
    pub objective: NodeSummary,
    /// Your own assessment, shown next to the computed health.
    pub status: ObjectiveStatus,
    pub priority: u8,
    pub target_date: Option<Date>,
    /// Ongoing (spec 30): no target date; judged by its work and its review rhythm.
    #[serde(default)]
    pub ongoing: bool,
    #[serde(default)]
    pub review_due: Option<Date>,
    /// Days its review is overdue by.
    #[serde(default)]
    pub review_overdue_days: Option<u32>,
    pub health: Health,
    pub projects: Vec<ProjectHealthRow>,
}

/// Someone over capacity this week or next (the Capacity screen has the full heatmap), or with
/// more open tasks than the configured limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverloadedPerson {
    pub person: NodeSummary,
    /// The heavier of this week and next, as a percentage of capacity (over 100 = overloaded).
    pub load_pct: f64,
    /// Monday of that heaviest week.
    pub peak_week: Option<Date>,
    pub open_tasks: u32,
    /// They have more open tasks than the limit in Settings.
    pub over_task_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewCounts {
    pub red: u32,
    pub amber: u32,
    pub green: u32,
    pub idle: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioOverview {
    pub today: Date,
    pub counts: OverviewCounts,
    pub objectives: Vec<ObjectiveHealthRow>,
    /// Active projects under no objective.
    pub unlinked_projects: Vec<ProjectHealthRow>,
    /// The five worst, most urgent first.
    pub risks: Vec<RiskItem>,
    /// How many more there are beyond the five.
    pub more_risks: u32,
    pub overloaded: Vec<OverloadedPerson>,
    pub stale_waiting: Vec<WaitingOnRow>,
    /// Things that limited the picture (e.g. a loop in `blocks` links).
    pub warnings: Vec<String>,
    pub thresholds: HealthThresholds,
}

/// Looks up a project's row by id (UI helper kept here so both sides agree).
impl PortfolioOverview {
    pub fn project_health(&self, id: Uuid) -> Option<&ProjectHealthRow> {
        self.objectives
            .iter()
            .flat_map(|o| o.projects.iter())
            .chain(self.unlinked_projects.iter())
            .find(|p| p.project.node.id == id)
    }

    pub fn objective_health(&self, id: Uuid) -> Option<&ObjectiveHealthRow> {
        self.objectives.iter().find(|o| o.objective.node.id == id)
    }
}
