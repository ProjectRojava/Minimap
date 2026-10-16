//! Zoom (spec 52, docs/product-design/09-semantic-zoom.md): the path from the whole portfolio down
//! to one item, so the screen can move up and down it. A view of the links; it stores nothing.

use serde::{Deserialize, Serialize};

use crate::NodeSummary;

/// How close a step is to the work. Ordered: `World` is the farthest out, `Item` the closest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Everything (the Overview).
    World,
    /// One objective with its projects.
    Objective,
    /// One project with its tasks.
    Project,
    /// One task, note, decision, person...
    Item,
}

/// One step of a path from the whole portfolio to an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomStep {
    pub level: Level,
    /// What the step is; `None` for `World`.
    pub node: Option<NodeSummary>,
    /// An `Objective` step reached through a project that serves several objectives: the other
    /// objectives, in the order they were passed over. Empty everywhere else.
    #[serde(default)]
    pub alternatives: Vec<NodeSummary>,
}

impl ZoomStep {
    /// The first step of every path: the whole portfolio.
    pub fn world() -> Self {
        Self {
            level: Level::World,
            node: None,
            alternatives: Vec::new(),
        }
    }

    pub fn of(level: Level, node: NodeSummary) -> Self {
        Self {
            level,
            node: Some(node),
            alternatives: Vec::new(),
        }
    }

    /// What the crumb says: "World" or the node's name.
    pub fn label(&self) -> &str {
        match &self.node {
            Some(n) => &n.label,
            None => "World",
        }
    }
}
