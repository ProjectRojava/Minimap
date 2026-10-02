use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{EdgeType, NodeRef, NodeType};

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
