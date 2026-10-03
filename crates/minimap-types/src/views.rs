//! Read models: what list and detail screens need, assembled by the store.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    NodeSummary, Objective, Person, Project, ProjectStatus, Task, TaskStatus, Team, WaitingOn,
};

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

// ---------------------------------------------------------------- objectives

/// How the objectives list is arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveGrouping {
    /// One flat list.
    #[default]
    None,
    /// Calendar quarter of the target date, plus a "No date" bucket.
    Quarter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveRow {
    pub objective: Objective,
    /// Active projects and tasks that contribute to it.
    pub contribution_count: u32,
}

/// A heading and its objectives. `label` is `None` for the single group of a flat list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveGroup {
    pub label: Option<String>,
    pub rows: Vec<ObjectiveRow>,
}

/// A project or task that contributes to an objective (`contributes_to` edge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    pub edge_id: Uuid,
    pub node: NodeSummary,
    /// The contributor's own status, e.g. `active` or `blocked`.
    pub status: String,
    /// 0-1; `None` means unset (treated as full weight by roll-ups).
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveDetail {
    pub objective: Objective,
    pub contributions: Vec<Contribution>,
}

// ------------------------------------------------------------------ projects

/// How the projects screen is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLayout {
    /// Grouped by objective (a project appears under each objective it contributes to).
    #[default]
    List,
    /// One column per status.
    Board,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectFilter {
    pub status: Option<ProjectStatus>,
    pub owner_person_id: Option<Uuid>,
    pub objective_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectRow {
    pub project: Project,
    pub owner: Option<NodeSummary>,
    /// Active objectives the project contributes to.
    pub objectives: Vec<NodeSummary>,
    /// Active tasks in the project, and how many of them are done.
    pub task_count: u32,
    pub done_task_count: u32,
}

/// A list section (one objective, or "No objective") or a board column (one status).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectGroup {
    pub label: String,
    pub objective: Option<NodeSummary>,
    pub status: Option<ProjectStatus>,
    pub rows: Vec<ProjectRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectTask {
    pub node: NodeSummary,
    pub status: TaskStatus,
    pub due_date: Option<time::Date>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub project: Project,
    pub owner: Option<NodeSummary>,
    /// Objectives it contributes to (with the objective's status and the link weight).
    pub objectives: Vec<Contribution>,
    /// Projects this one depends on.
    pub depends_on: Vec<LinkedNode>,
    /// Projects that depend on this one.
    pub needed_by: Vec<NodeSummary>,
    pub tasks: Vec<ProjectTask>,
}

/// What to do with a project's tasks when it is archived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDisposition {
    /// Archive them with the project.
    Archive,
    /// Keep them, without a project (they land in the inbox).
    Inbox,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectArchivePreview {
    pub tasks: Vec<NodeSummary>,
}

// --------------------------------------------------------------------- tasks

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskFilter {
    /// Only this status (done and cancelled included when asked for explicitly).
    pub status: Option<TaskStatus>,
    pub project_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
    /// Inclusive due-date range; tasks without a due date never match a range.
    pub due_from: Option<time::Date>,
    pub due_to: Option<time::Date>,
    /// Every word must appear in the title, description, project or assignee name.
    pub text: Option<String>,
    /// The inbox: tasks that belong to no project.
    pub no_project: bool,
    /// Include done and cancelled tasks (unless a `status` is chosen).
    pub include_closed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRow {
    pub task: Task,
    pub project: Option<NodeSummary>,
    pub assignee: Option<NodeSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDetail {
    pub task: Task,
    pub project: Option<NodeSummary>,
    pub assignee: Option<NodeSummary>,
}

pub const DEFAULT_HOURS_PER_DAY: f64 = 8.0;
/// A waiting-on older than this many days (or past its expected date) counts as stale.
pub const DEFAULT_STALE_WAITING_DAYS: u32 = 7;
/// Dark is the default look. The UI owns the list of themes; the backend only stores the id.
pub const DEFAULT_THEME: &str = "minimap-dark";

/// App settings stored in the database. Spec 23 grows this; today it has one entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Working hours in a day, used to turn `4h` estimates into days.
    pub hours_per_day: f64,
    /// Id of the colour theme (`minimap-dark`, `nord`, ...) or `system` to follow the OS.
    pub theme: String,
    /// Open waiting-ons older than this many days are stale.
    pub stale_waiting_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hours_per_day: DEFAULT_HOURS_PER_DAY,
            theme: DEFAULT_THEME.to_owned(),
            stale_waiting_days: DEFAULT_STALE_WAITING_DAYS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub hours_per_day: Option<f64>,
    pub theme: Option<String>,
    pub stale_waiting_days: Option<u32>,
}

// ---------------------------------------------------------------- waiting-on

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaitingOnFilter {
    pub person_id: Option<Uuid>,
    /// Include ones already resolved.
    pub include_resolved: bool,
    /// Include ones snoozed until a future date.
    pub include_snoozed: bool,
}

/// A waiting-on with who it is from and what it is about, before ages and flags are worked out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitingOnItem {
    pub waiting: WaitingOn,
    pub person: NodeSummary,
    /// The task or project it is for (`about` link), if any.
    pub about: Option<NodeSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitingOnRow {
    pub waiting: WaitingOn,
    pub person: NodeSummary,
    pub about: Option<NodeSummary>,
    /// Days since it was asked (never negative).
    pub age_days: i64,
    /// Open, not snoozed, and older than the stale threshold or past its expected date.
    pub stale: bool,
    /// Open and snoozed until a future date.
    pub snoozed: bool,
    /// Open, not snoozed, and past its expected date.
    pub overdue: bool,
}
