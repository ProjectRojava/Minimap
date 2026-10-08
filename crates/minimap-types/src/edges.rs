use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{EdgeType, NodeRef, NodeSummary, NodeType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub edge_type: EdgeType,
    pub from_type: NodeType,
    pub from_id: Uuid,
    pub to_type: NodeType,
    pub to_id: Uuid,
    /// Type-specific attributes (`lag_days`, `weight`, ...). Validated in core, not here.
    pub attrs: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

impl Edge {
    pub fn from(&self) -> NodeRef {
        NodeRef::new(self.from_type, self.from_id)
    }

    pub fn to(&self) -> NodeRef {
        NodeRef::new(self.to_type, self.to_id)
    }
}

/// How the *other* task relates to the one a link is made from (the link dialog, "Link a new
/// task"). Two families: the order of the work (`blocks`) and what is part of what (`subtask_of`,
/// spec 33), which has no effect on the schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkRelation {
    /// The other task blocks this one: it has to be done first.
    Blocks,
    /// The other task is blocked by this one: it waits for this one.
    BlockedBy,
    /// Just related, with no order.
    RelatesTo,
    /// The other task is this one's parent: this one is part of it.
    Parent,
    /// The other task is a sub-task of this one: it is part of this one.
    Subtask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEdge {
    pub edge_type: EdgeType,
    pub from: NodeRef,
    pub to: NodeRef,
    #[serde(default = "empty_attrs")]
    pub attrs: serde_json::Value,
}

fn empty_attrs() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// An edge as seen from one node: which way it points and what is on the other end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeLink {
    pub edge: Edge,
    /// True when the queried node is the `from` end.
    pub outgoing: bool,
    pub other: NodeSummary,
}

/// What an edge attribute holds, so editors can offer the right control and validation can
/// share the same definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttrKind {
    WholeNumber { min: u32, max: Option<u32> },
    Number { min: f64, max: f64 },
    Choice { options: Vec<String> },
    Text,
}

/// One attribute an edge type may carry (all attributes are optional).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttrSpec {
    pub key: String,
    pub label: String,
    pub kind: AttrKind,
    /// Completes "<key> must be ..." in validation errors.
    pub hint: String,
}

/// A relation the user can add from a node of some type: which edge type, in which direction,
/// to which kinds of node, and which attributes it takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkOption {
    pub edge_type: EdgeType,
    /// True when the node is the `from` end (it blocks, it depends on, ...).
    pub outgoing: bool,
    /// Node types allowed on the other end.
    pub others: Vec<NodeType>,
    pub attrs: Vec<AttrSpec>,
}
