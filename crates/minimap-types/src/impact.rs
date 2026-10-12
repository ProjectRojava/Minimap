//! Impact analysis (spec 14): "what if this slips?". A scenario is one or more slips; the
//! report says what each ripples into. Times are working days (see `schedule`).

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{NodeRef, NodeSummary};

/// "This task (or project) runs `days` working days late." A task starts that much later; a
/// project's open tasks all do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slip {
    /// A task or a project.
    pub node: NodeRef,
    pub days: u32,
}

/// A task the slip reaches. `delay_days` is how far its own finish moves; the rest of what
/// arrived (`incoming_days`) was soaked up by slack on the way (`absorbed_days`), so
/// `incoming_days = absorbed_days + delay_days`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactTask {
    pub id: Uuid,
    pub title: String,
    pub project_id: Option<Uuid>,
    pub project_title: Option<String>,
    /// Slipped by the scenario itself (or as part of a slipped project).
    pub direct: bool,
    pub incoming_days: f64,
    pub absorbed_days: f64,
    pub delay_days: f64,
    pub old_start: Date,
    pub new_start: Date,
    pub old_finish: Date,
    pub new_finish: Date,
    pub due_date: Option<Date>,
    /// Working days past the due date before / after.
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
    /// On time before, late after.
    pub newly_late: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactProject {
    pub id: Uuid,
    pub title: String,
    pub direct: bool,
    pub delay_days: f64,
    pub affected_tasks: u32,
    pub old_finish: Option<Date>,
    pub new_finish: Option<Date>,
    pub target_date: Option<Date>,
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
    pub newly_late: bool,
}

/// An objective whose contributors (projects or tasks) finish later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactObjective {
    pub id: Uuid,
    pub title: String,
    pub delay_days: f64,
    pub old_finish: Option<Date>,
    pub new_finish: Option<Date>,
    pub target_date: Option<Date>,
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
    pub newly_late: bool,
    /// The late contributors, by name.
    pub contributors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactPersonTask {
    pub id: Uuid,
    pub title: String,
    pub delay_days: f64,
    pub new_finish: Date,
}

/// Someone whose assigned work moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactPerson {
    pub id: Uuid,
    pub name: String,
    /// The largest delay among their tasks.
    pub delay_days: f64,
    pub tasks: Vec<ImpactPersonTask>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactSummary {
    /// Tasks whose finish moves.
    pub moved_tasks: u32,
    /// Tasks the slip reached but slack absorbed completely.
    pub absorbing_tasks: u32,
    pub newly_late_tasks: u32,
    pub newly_late_projects: u32,
    pub newly_late_objectives: u32,
    pub people: u32,
    /// The largest delay anywhere, in working days.
    pub worst_delay_days: f64,
}

/// What a step of a recalculated route does (spec 46). The words are in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnKind {
    /// A slipped task: where the delay enters.
    Slips,
    /// The delay arrives and goes on whole.
    PassesOn,
    /// Slack soaks all of it up: it stops here.
    Absorbs,
    /// Slack soaks up some of it and the rest goes on.
    PartlyAbsorbs,
    /// Now on the route, and wasn't.
    JoinsRoute,
    /// Was on the route, and isn't.
    LeavesRoute,
    /// The project's last task: where it ends up.
    Arrives,
    /// "... N more steps": a run of steps left out of a long route.
    More,
}

/// One step of the turn-by-turn list of a project's recalculated route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub task: NodeSummary,
    pub kind: TurnKind,
    /// What reached the task, soaked up and passed on (working days); `incoming = absorbed + delay`.
    pub incoming_days: f64,
    pub absorbed_days: f64,
    pub delay_days: f64,
    /// Where the task finishes after the slip (the project's finish for `Arrives`).
    pub new_finish: Date,
    /// For `PassesOn`, the other steps of the same delay folded into this one ("and 2 more");
    /// for `More`, how many steps were left out.
    pub also: u32,
}

/// A task drawn on a route timeline: where it was and where it is now, as working-day offsets from
/// today, and whether it is on the old or the new route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteBar {
    pub id: Uuid,
    pub title: String,
    pub old_es: f64,
    pub old_ef: f64,
    pub new_es: f64,
    pub new_ef: f64,
    pub on_old_route: bool,
    pub on_new_route: bool,
}

/// The route to a project's finish before and after the scenario (spec 46): the critical chain, the
/// new finish, and a turn-by-turn list of where the slip enters, is absorbed, passes on and arrives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectRoute {
    pub project_id: Uuid,
    pub title: String,
    /// The baseline critical chain, in order from start to finish.
    pub before: Vec<Uuid>,
    /// The scenario's critical chain.
    pub after: Vec<Uuid>,
    /// `before != after` as sequences.
    pub changed: bool,
    pub old_finish: Option<Date>,
    pub new_finish: Option<Date>,
    pub target_date: Option<Date>,
    /// Working days the finish moves.
    pub delay_days: f64,
    /// Working days past the target after the slip.
    pub late_after: Option<u32>,
    /// On time before, late after.
    #[serde(default)]
    pub newly_late: bool,
    /// At most `MAX_TURNS`; the last is always `Arrives`.
    pub turns: Vec<Turn>,
    /// The tasks to draw: both routes and everything the slip reached in the project.
    pub bars: Vec<RouteBar>,
    /// Offset of `days[0]`, and the working days the timeline covers (today, every bar and the
    /// target), so the screen can label its axis without date arithmetic.
    pub first_offset: i64,
    pub days: Vec<Date>,
    /// End of the target day, as an offset.
    pub target_offset: Option<f64>,
}

/// The most steps a turn-by-turn list shows; a longer route folds its middle into a `More` step.
pub const MAX_TURNS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactReport {
    /// The scenario, named.
    pub slips: Vec<(NodeSummary, u32)>,
    pub summary: ImpactSummary,
    /// Every task the slip reaches, direct ones first then by start date. Includes tasks that
    /// absorbed it completely (delay 0): that is where the slip stops.
    pub tasks: Vec<ImpactTask>,
    pub projects: Vec<ImpactProject>,
    pub objectives: Vec<ImpactObjective>,
    pub people: Vec<ImpactPerson>,
    /// The route to the finish of each affected project, before and after (spec 46), in the order
    /// of `projects`.
    #[serde(default)]
    pub routes: Vec<ProjectRoute>,
}

/// One date change "apply" would make: the task starts on `start_date` (pinned) and its due
/// date, if it has one, moves to `due_date`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateChange {
    pub task_id: Uuid,
    pub title: String,
    pub old_start: Option<Date>,
    pub new_start: Date,
    pub old_due: Option<Date>,
    pub new_due: Option<Date>,
}

/// What recording the scenario in the plan would change; shown before it is applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyPreview {
    pub changes: Vec<DateChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyResult {
    pub tasks_changed: u32,
}
