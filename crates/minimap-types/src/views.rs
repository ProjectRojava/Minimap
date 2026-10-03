//! Read models: what list and detail screens need, assembled by the store.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{NodeSummary, Person, Team, WaitingOn};

/// A team membership (`member_of` edge) as seen from either end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Membership {
    pub edge_id: Uuid,
    /// The node on the other end: the team (from a person) or the person (from a team).
    pub node: NodeSummary,
    /// `lead` or `member`.
    pub role: String,
}

/// An edge to a single related node (e.g. someone's manager).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedNode {
    pub edge_id: Uuid,
    pub node: NodeSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonRow {
    pub person: Person,
    pub teams: Vec<NodeSummary>,
    /// Assigned tasks that are todo, in progress or blocked.
    pub active_task_count: u32,
    pub open_waiting_on_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonDetail {
    pub person: Person,
    pub memberships: Vec<Membership>,
    pub manager: Option<LinkedNode>,
    pub reports: Vec<NodeSummary>,
    /// Unresolved waiting-ons on this person.
    pub waiting_ons: Vec<WaitingOn>,
    pub active_task_count: u32,
}

/// Shown before archiving a person: the work that loses its assignee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonArchivePreview {
    pub assigned_tasks: Vec<NodeSummary>,
}

/// One row of the team tree, parents before children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamRow {
    pub team: Team,
    /// 0 for top-level teams.
    pub depth: u32,
    pub member_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamDetail {
    pub team: Team,
    pub parent: Option<NodeSummary>,
    pub children: Vec<NodeSummary>,
    pub members: Vec<Membership>,
}
