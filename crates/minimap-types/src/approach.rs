//! Approaching (spec 45): trouble elsewhere that will reach what you are looking at, through
//! `blocks` and `depends_on` links. A view of the schedule; it stores nothing.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::NodeSummary;

/// What is being looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproachScope {
    /// The open tasks of one project (and the project itself).
    Project(Uuid),
    /// The open tasks assigned to the self person, and the projects they own.
    Me,
    /// The tasks of the projects that contribute to an objective, and tasks that contribute to it
    /// directly.
    Objective(Uuid),
}

/// Why something outside is in trouble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Its due date has passed and it is still open.
    Overdue,
    /// Its status is Blocked.
    Blocked,
    /// It is scheduled to finish after its due date.
    Late,
    /// Its slack is negative: the project's target can't be met with it.
    NoSlack,
    /// A project whose forecast finishes after its target.
    ProjectLate,
    /// A project whose computed health is red.
    ProjectRed,
}

/// How much it matters to what is being looked at, worst first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The target is already late and this is what holds it up.
    Hits,
    /// The target has little or no slack.
    Threatens,
    /// There is room: worth knowing, not worth an alarm.
    Watch,
}

/// One piece of trouble and how it reaches what is being looked at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approaching {
    /// The thing in trouble (outside what is being looked at).
    pub source: NodeSummary,
    /// The thing it will reach (inside).
    pub target: NodeSummary,
    /// The chain from the source to the target, both included.
    pub path: Vec<NodeSummary>,
    /// Why the source is in trouble, in the order of `Reason`.
    pub reasons: Vec<Reason>,
    /// Calendar days past the due date, when the source is overdue.
    #[serde(default)]
    pub overdue_days: Option<u32>,
    /// Working days from today until the target is due to start (0 when it is under way).
    pub arrives_in: u32,
    pub severity: Severity,
    /// The target's slack in working days (a project's room before its target), if it has any.
    pub target_slack: Option<i32>,
}

/// The trouble coming from one project (or the inbox).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApproachGroup {
    /// `None` = tasks that are in no project.
    pub source_project: Option<NodeSummary>,
    /// Hits first, then threatens, then watch; then the soonest.
    pub items: Vec<Approaching>,
}

/// What the strip says: how many, how soon, how bad. Counts `Hits` and `Threatens` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Headline {
    pub count: u32,
    /// Working days until the first of them arrives.
    pub first_in_days: u32,
    pub worst: Severity,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApproachReport {
    /// Groups ordered by their worst item. `Watch` items are in here too.
    pub groups: Vec<ApproachGroup>,
    /// `None` when nothing is hitting or threatening.
    pub headline: Option<Headline>,
}

/// How many `blocks` links a chain may have between the trouble and what it reaches.
pub const MAX_HOPS: usize = 4;
/// Slack of this many working days or fewer makes trouble a threat.
pub const THREAT_SLACK: i32 = 2;
