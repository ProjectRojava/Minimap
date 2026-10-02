use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{ActivityAction, NodeType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub node_type: NodeType,
    pub node_id: Uuid,
    pub action: ActivityAction,
    /// `{field: [old, new]}`.
    pub diff: serde_json::Value,
}
