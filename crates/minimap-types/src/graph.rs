//! Dependency graph (spec 18): nodes and edges already laid out left to right, ready to draw.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use time::Date;

use crate::{HealthLevel, NodeRef, NodeSummary, ProjectStatus, TaskStatus, Zone};

/// What the nodes are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphLevel {
    /// Tasks and their `blocks` links.
    #[default]
    Tasks,
    /// Projects and their `depends_on` links.
    Projects,
}

/// What to show. Filters combine (all must hold); with none, everything linked is shown.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphFilter {
    pub level: GraphLevel,
    /// Tasks: only this project's tasks. (Not used for the project level.)
    pub project_id: Option<Uuid>,
    /// Tasks: only work assigned to members of this team or its sub-teams.
    pub team_id: Option<Uuid>,
    /// Only work that contributes to this objective (a project's tasks count through it).
    pub objective_id: Option<Uuid>,
    /// Include finished work.
    pub include_done: bool,
    /// Include nodes with no links at all.
    pub include_isolated: bool,
    /// My view (spec 50): only the work in `MyScope`: your tasks, what blocks them (up to four
    /// links back) and what they block (one hop), or the projects in the same way. The usual
    /// one-hop dimmed context still applies.
    pub mine: bool,
    /// The one box laid out large enough to show its details (spec 47): the layout makes room
    /// for it. Ignored when it isn't in the graph.
    pub expanded: Option<Uuid>,
    /// An unlinked task (or project) dropped onto the graph from the unlinked list: drawn even
    /// though nothing links to it, in a place of its own below the graph, so the next one dropped
    /// can be linked to it. Ignored when it isn't a candidate for the graph.
    pub waiting: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub node: NodeRef,
    pub label: String,
    /// A second line: the project (tasks) or the status and finish (projects).
    pub subtitle: String,
    /// Top-left corner and size, in the graph's own pixels.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Column, 0 = leftmost.
    pub layer: u32,
    /// Shown only because it is linked to something the filter selected.
    pub context: bool,
    pub done: bool,
    /// On the critical path (decides its project's finish).
    pub critical: bool,
    /// Projected past its deadline (negative slack for tasks; late against target for projects).
    pub late: bool,
    pub blocked: bool,
    /// An unfinished task that waits, directly or further down the chain, on a blocked task.
    pub held_up: bool,
    /// A task's status (spec 47); `None` for a project.
    #[serde(default)]
    pub task_status: Option<TaskStatus>,
    /// A project's status; `None` for a task.
    #[serde(default)]
    pub project_status: Option<ProjectStatus>,
    #[serde(default)]
    pub priority: u8,
    /// A task's first assignee, or a project's owner.
    #[serde(default)]
    pub assignee: Option<NodeSummary>,
    /// A task's project, or the project itself: for the objective colour.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// When it is projected to finish.
    #[serde(default)]
    pub finish: Option<Date>,
    /// A task's due date, or a project's target.
    #[serde(default)]
    pub due: Option<Date>,
    /// A task's slack in working days (negative: the target can't be met).
    #[serde(default)]
    pub slack_days: Option<f64>,
    /// Working days past its due date or target, when late.
    #[serde(default)]
    pub late_by_days: Option<u32>,
    #[serde(default)]
    pub estimate_days: Option<f64>,
    #[serde(default)]
    pub task_type: Option<String>,
    /// A project's computed health.
    #[serde(default)]
    pub health: Option<HealthLevel>,
    /// A project's safe zone (spec 44).
    #[serde(default)]
    pub zone: Zone,
    /// A project's tasks: how many are done, and how many there are.
    #[serde(default)]
    pub progress: Option<(u32, u32)>,
    /// How many of the drawn nodes it waits on, through any number of links.
    #[serde(default)]
    pub upstream: u32,
    /// How many of the drawn nodes it holds up, through any number of links.
    #[serde(default)]
    pub downstream: u32,
    /// Laid out large, to show its details (`GraphFilter::expanded`).
    #[serde(default)]
    pub expanded: bool,
    /// The unlinked item dropped onto the graph (`GraphFilter::waiting`).
    #[serde(default)]
    pub waiting: bool,
}

/// How big an expanded box is laid out, in the graph's own pixels (a task's, a project's).
pub const EXPANDED_W: f64 = 340.0;
pub const EXPANDED_TASK_H: f64 = 380.0;
pub const EXPANDED_PROJECT_H: f64 = 300.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: Uuid,
    pub from: Uuid,
    pub to: Uuid,
    /// `blocks` lag, when set.
    pub lag_days: Option<u32>,
    /// Both ends are critical and nothing slack separates them: this link drives the finish.
    pub critical: bool,
    /// The route, from the right edge of `from` to the left edge of `to`, through the layers
    /// in between.
    pub points: Vec<(f64, f64)>,
    /// A task link between two projects (or a project and the inbox).
    #[serde(default)]
    pub cross_project: bool,
    /// A project link: the working days between the end of the project needed and the start of
    /// the one that depends on it.
    #[serde(default)]
    pub room_days: Option<u32>,
}

/// A node left out of the drawing because nothing links to it (spec 47): listed in a panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphUnlinked {
    pub node: NodeRef,
    pub label: String,
    /// The project, or the status and finish for a project.
    pub subtitle: String,
    /// The node as it would be drawn, with every fact a box says (status, owner, priority, due,
    /// health...), so the list can look like the graph. Its place and size are zero and its
    /// links none.
    pub look: GraphNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub level: GraphLevel,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub width: f64,
    pub height: f64,
    pub critical_nodes: u32,
    /// Unlinked nodes left out (tick "Draw unlinked" to see them in the graph).
    pub hidden_unlinked: u32,
    /// The same nodes, by name (at most `MAX_UNLINKED_LISTED`).
    #[serde(default)]
    pub unlinked: Vec<GraphUnlinked>,
    pub warnings: Vec<String>,
}

/// The most unlinked nodes listed by name (a panel searches them); `hidden_unlinked` still counts
/// them all.
pub const MAX_UNLINKED_LISTED: usize = 500;
