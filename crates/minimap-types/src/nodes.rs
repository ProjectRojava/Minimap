//! Node records plus their `Create*` / `Update*` inputs.
//! Dates serialize as `YYYY-MM-DD`; timestamps as RFC 3339.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{
    DecisionStatus, NodeType, NoteKind, ObjectiveStatus, Patch, ProjectStatus, TaskStatus,
};

pub const DEFAULT_PRIORITY: u8 = 3;
pub const DEFAULT_WEEKLY_CAPACITY_HOURS: f64 = 40.0;

/// Points at any node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef {
    pub node_type: NodeType,
    pub id: Uuid,
}

impl NodeRef {
    pub fn new(node_type: NodeType, id: Uuid) -> Self {
        Self { node_type, id }
    }
}

// ---------------------------------------------------------------- Objective

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Objective {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub target_date: Option<Date>,
    pub status: ObjectiveStatus,
    pub priority: u8,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateObjective {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub target_date: Option<Date>,
    #[serde(default)]
    pub status: Option<ObjectiveStatus>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateObjective {
    pub title: Option<String>,
    pub description: Option<String>,
    pub target_date: Patch<Date>,
    pub status: Option<ObjectiveStatus>,
    pub priority: Option<u8>,
}

impl UpdateObjective {
    pub fn apply(self, o: &mut Objective) {
        set(&mut o.title, self.title);
        set(&mut o.description, self.description);
        self.target_date.apply(&mut o.target_date);
        set(&mut o.status, self.status);
        set(&mut o.priority, self.priority);
    }
}

// ------------------------------------------------------------------ Project

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_person_id: Option<Uuid>,
    pub start_date: Option<Date>,
    pub target_date: Option<Date>,
    pub status: ProjectStatus,
    pub priority: u8,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProject {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub owner_person_id: Option<Uuid>,
    #[serde(default)]
    pub start_date: Option<Date>,
    #[serde(default)]
    pub target_date: Option<Date>,
    #[serde(default)]
    pub status: Option<ProjectStatus>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateProject {
    pub title: Option<String>,
    pub description: Option<String>,
    pub owner_person_id: Patch<Uuid>,
    pub start_date: Patch<Date>,
    pub target_date: Patch<Date>,
    pub status: Option<ProjectStatus>,
    pub priority: Option<u8>,
}

impl UpdateProject {
    pub fn apply(self, p: &mut Project) {
        set(&mut p.title, self.title);
        set(&mut p.description, self.description);
        self.owner_person_id.apply(&mut p.owner_person_id);
        self.start_date.apply(&mut p.start_date);
        self.target_date.apply(&mut p.target_date);
        set(&mut p.status, self.status);
        set(&mut p.priority, self.priority);
    }
}

// --------------------------------------------------------------------- Task

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub project_id: Option<Uuid>,
    pub status: TaskStatus,
    pub estimate_days: Option<f64>,
    pub start_date: Option<Date>,
    pub due_date: Option<Date>,
    /// Set by the store when status moves to/from `done`.
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    pub priority: u8,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub estimate_days: Option<f64>,
    #[serde(default)]
    pub start_date: Option<Date>,
    #[serde(default)]
    pub due_date: Option<Date>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub project_id: Patch<Uuid>,
    pub status: Option<TaskStatus>,
    pub estimate_days: Patch<f64>,
    pub start_date: Patch<Date>,
    pub due_date: Patch<Date>,
    pub priority: Option<u8>,
}

impl UpdateTask {
    pub fn apply(self, t: &mut Task) {
        set(&mut t.title, self.title);
        set(&mut t.description, self.description);
        self.project_id.apply(&mut t.project_id);
        set(&mut t.status, self.status);
        self.estimate_days.apply(&mut t.estimate_days);
        self.start_date.apply(&mut t.start_date);
        self.due_date.apply(&mut t.due_date);
        set(&mut t.priority, self.priority);
    }
}

// ------------------------------------------------------------------- Person

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: Uuid,
    pub name: String,
    pub role_title: String,
    pub email: Option<String>,
    pub weekly_capacity_hours: f64,
    /// Exactly one person is the user.
    pub is_self: bool,
    pub notes: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePerson {
    pub name: String,
    #[serde(default)]
    pub role_title: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub weekly_capacity_hours: Option<f64>,
    #[serde(default)]
    pub is_self: bool,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePerson {
    pub name: Option<String>,
    pub role_title: Option<String>,
    pub email: Patch<String>,
    pub weekly_capacity_hours: Option<f64>,
    pub notes: Option<String>,
}

impl UpdatePerson {
    pub fn apply(self, p: &mut Person) {
        set(&mut p.name, self.name);
        set(&mut p.role_title, self.role_title);
        self.email.apply(&mut p.email);
        set(&mut p.weekly_capacity_hours, self.weekly_capacity_hours);
        set(&mut p.notes, self.notes);
    }
}

// --------------------------------------------------------------------- Team

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub parent_team_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTeam {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parent_team_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateTeam {
    pub name: Option<String>,
    pub description: Option<String>,
    pub parent_team_id: Patch<Uuid>,
}

impl UpdateTeam {
    pub fn apply(self, t: &mut Team) {
        set(&mut t.name, self.name);
        set(&mut t.description, self.description);
        self.parent_team_id.apply(&mut t.parent_team_id);
    }
}

// --------------------------------------------------------------------- Note

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    /// Markdown.
    pub body: String,
    pub note_date: Date,
    pub kind: NoteKind,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNote {
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Defaults to today (UTC).
    #[serde(default)]
    pub note_date: Option<Date>,
    #[serde(default)]
    pub kind: Option<NoteKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub body: Option<String>,
    pub note_date: Option<Date>,
    pub kind: Option<NoteKind>,
}

impl UpdateNote {
    pub fn apply(self, n: &mut Note) {
        set(&mut n.title, self.title);
        set(&mut n.body, self.body);
        set(&mut n.note_date, self.note_date);
        set(&mut n.kind, self.kind);
    }
}

// ----------------------------------------------------------------- Decision

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub id: Uuid,
    pub title: String,
    pub context: String,
    pub decision: String,
    pub rationale: String,
    pub decided_on: Option<Date>,
    pub status: DecisionStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDecision {
    pub title: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub decided_on: Option<Date>,
    #[serde(default)]
    pub status: Option<DecisionStatus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateDecision {
    pub title: Option<String>,
    pub context: Option<String>,
    pub decision: Option<String>,
    pub rationale: Option<String>,
    pub decided_on: Patch<Date>,
    pub status: Option<DecisionStatus>,
}

impl UpdateDecision {
    pub fn apply(self, d: &mut Decision) {
        set(&mut d.title, self.title);
        set(&mut d.context, self.context);
        set(&mut d.decision, self.decision);
        set(&mut d.rationale, self.rationale);
        self.decided_on.apply(&mut d.decided_on);
        set(&mut d.status, self.status);
    }
}

// ---------------------------------------------------------------- WaitingOn

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitingOn {
    pub id: Uuid,
    pub description: String,
    /// Who you're waiting on.
    pub person_id: Uuid,
    pub asked_on: Date,
    pub expected_by: Option<Date>,
    pub resolved_on: Option<Date>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWaitingOn {
    pub description: String,
    pub person_id: Uuid,
    /// Defaults to today (UTC).
    #[serde(default)]
    pub asked_on: Option<Date>,
    #[serde(default)]
    pub expected_by: Option<Date>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateWaitingOn {
    pub description: Option<String>,
    pub person_id: Option<Uuid>,
    pub asked_on: Option<Date>,
    pub expected_by: Patch<Date>,
    pub resolved_on: Patch<Date>,
}

impl UpdateWaitingOn {
    pub fn apply(self, w: &mut WaitingOn) {
        set(&mut w.description, self.description);
        set(&mut w.person_id, self.person_id);
        set(&mut w.asked_on, self.asked_on);
        self.expected_by.apply(&mut w.expected_by);
        self.resolved_on.apply(&mut w.resolved_on);
    }
}

fn set<T>(target: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *target = v;
    }
}
