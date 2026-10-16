//! Pings (docs/product-design/05-pings.md, ADR-0020): a marker on any item with a day and an
//! optional line of text, to come back to it. A ping is private bookkeeping: it never changes the
//! item, its dates, the schedule, capacity, health or impact.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{NodeRef, NodeSummary, Patch};

/// Longest note a ping can carry, in characters.
pub const MAX_PING_NOTE: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ping {
    pub id: Uuid,
    pub target: NodeRef,
    /// Plain text, trimmed; may be empty.
    pub note: String,
    /// The day to come back to the item.
    pub on_date: Date,
    /// The day it was cleared; `None` while it is live.
    pub cleared_on: Option<Date>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

impl Ping {
    /// Not archived and not cleared (whether its target is still active is the store's question).
    pub fn is_live(&self) -> bool {
        self.archived_at.is_none() && self.cleared_on.is_none()
    }
}

/// A ping with what it points at, for lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PingRow {
    pub ping: Ping,
    pub target: NodeSummary,
}

/// Pings a target that has a live one updates that one instead (one live ping per item).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatePing {
    pub target: NodeRef,
    #[serde(default)]
    pub note: String,
    pub on_date: Date,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePing {
    pub note: Option<String>,
    pub on_date: Option<Date>,
    pub cleared_on: Patch<Date>,
}

impl UpdatePing {
    pub fn apply(self, p: &mut Ping) {
        if let Some(note) = self.note {
            p.note = note;
        }
        if let Some(on) = self.on_date {
            p.on_date = on;
        }
        self.cleared_on.apply(&mut p.cleared_on);
    }
}

/// The live pings of items that are active, split by whether their day has come.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PingsView {
    /// `on_date <= today`: oldest day first, then the order they were made in.
    pub due: Vec<PingRow>,
    /// `on_date > today`: soonest first.
    pub upcoming: Vec<PingRow>,
}
