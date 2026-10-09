//! Dependency graph (spec 18): nodes and edges already laid out left to right, ready to draw.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::NodeRef;

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
}

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub level: GraphLevel,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub width: f64,
    pub height: f64,
    pub critical_nodes: u32,
    /// Unlinked nodes left out (turn on "include unlinked" to see them).
    pub hidden_unlinked: u32,
    pub warnings: Vec<String>,
}
