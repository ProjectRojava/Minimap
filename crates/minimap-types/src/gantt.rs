//! The Gantt screen (spec 57): the plan as bars on a calendar, objectives → projects → tasks,
//! and what dragging a bar would do. Positions are working-day offsets from today, the same as
//! `Schedule`, so the screen needs no date arithmetic of its own.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{ObjectiveStatus, ProjectStatus, ScheduledTask, Zone};

/// What the screen asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GanttFilter {
    /// My view (spec 50): only what touches you.
    pub mine: bool,
}

/// The span a row covers: the work itself, the date it is pinned to start on, and the date it has
/// to be done by. Everything is optional because an objective may have no dates at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Band {
    /// First and last day the open work occupies, as offsets (`work_to` is the end of the last day).
    pub work_from: Option<f64>,
    pub work_to: Option<f64>,
    /// The project's own start date: no task starts before it (spec 57).
    pub start_date: Option<Date>,
    pub start_offset: Option<f64>,
    /// The target date and the end of that day as an offset.
    pub target_date: Option<Date>,
    pub target_offset: Option<f64>,
    /// Working days the work runs past the target, when it does.
    pub late_by_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttObjective {
    pub id: Uuid,
    pub title: String,
    pub status: ObjectiveStatus,
    /// No end: its bar fades out and its target can't be dragged (ADR-0014).
    pub ongoing: bool,
}

/// One task as a row: the schedule's numbers and the dates the person wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttTask {
    pub schedule: ScheduledTask,
    pub start_date: Option<Date>,
    pub due_date: Option<Date>,
    /// The end of the due day, as an offset (for the due tick).
    pub due_offset: Option<f64>,
    pub estimate_days: Option<f64>,
    pub priority: u8,
    /// The task this is a sub-task of, when that task is in the same project (organisation only).
    pub parent: Option<Uuid>,
    /// Open and past its due date.
    pub overdue: bool,
    /// The people it is assigned to (`assigned_to`), for the Gantt's person filter and rows by
    /// person (spec 58).
    #[serde(default)]
    pub assignees: Vec<Uuid>,
}

/// A project whose bar is also under another objective.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttRef {
    pub id: Uuid,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttProject {
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub status: ProjectStatus,
    pub band: Band,
    pub zone: Zone,
    pub open_tasks: u32,
    pub blocked_tasks: u32,
    pub done_tasks: u32,
    /// The other objectives it serves; it is drawn once, under the first.
    pub also_under: Vec<GanttRef>,
    /// The tasks that decide the project's finish, in order (spec 46).
    pub chain: Vec<Uuid>,
    /// In row order: a task, then its sub-tasks.
    pub tasks: Vec<GanttTask>,
    /// Late, out of room, or with blocked work (spec 58): opens on its own and passes the
    /// *Needs a look* filter. Never for a finished project.
    #[serde(default)]
    pub needs_a_look: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttGroup {
    /// None: the projects that serve no objective.
    pub objective: Option<GanttObjective>,
    pub band: Band,
    pub projects: Vec<GanttProject>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GanttLinkKind {
    /// Task → task: the second can't start before the first is done (plus lag).
    Blocks,
    /// Project → project: the second needs the first.
    DependsOn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttLink {
    pub kind: GanttLinkKind,
    pub from: Uuid,
    pub to: Uuid,
    pub lag_days: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GanttData {
    pub today: Date,
    /// Offset of `days[0]`; `days` are the working days the axis covers, with room to the right.
    pub first_offset: i64,
    pub days: Vec<Date>,
    pub groups: Vec<GanttGroup>,
    pub links: Vec<GanttLink>,
    /// Everyone assigned to a drawn task, by name (spec 58).
    #[serde(default)]
    pub people: Vec<GanttRef>,
}

// ------------------------------------------------------------------ dragging a bar

/// Which part of a task's bar was dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DragEdge {
    /// The middle: the whole bar moves, `to` is its new first day.
    Move,
    /// The left edge: `to` is the new first day, the end stays.
    Start,
    /// The right edge: `to` is the new last day, the start stays.
    End,
}

/// A drag, said in days of the calendar (always a working day), so it means the same thing a few
/// seconds later even if the screen's offsets have moved on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDrag {
    pub task_id: Uuid,
    pub edge: DragEdge,
    pub to: Date,
}

/// What stops a bar from starting where it is dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EarliestReason {
    Today,
    ProjectStart,
    BlockedBy { task_id: Uuid, title: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DragRefusal {
    /// Not on the plan: a meeting, a cancelled or archived task.
    NotPlanned,
    /// Finished work stays at the dates it was done on.
    Finished,
    /// The task can't start that early.
    TooEarly {
        earliest: Date,
        reason: EarliestReason,
    },
    /// The new first day is after the last one.
    AfterEnd,
    /// The new last day is before the first one.
    BeforeStart,
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `Thu 22 Oct`.
pub fn short_day(d: Date) -> String {
    format!(
        "{} {} {}",
        WEEKDAYS[d.weekday().number_days_from_monday() as usize],
        d.day(),
        MONTHS[d.month() as usize - 1]
    )
}

impl DragRefusal {
    /// What to tell the person (docs/voice.md: what is true, then what to do).
    pub fn text(&self) -> String {
        match self {
            DragRefusal::NotPlanned => {
                "This isn't part of the plan, so it has no bar to move.".to_owned()
            }
            DragRefusal::Finished => {
                "Finished work stays on the days it was done. Reopen it to move it.".to_owned()
            }
            DragRefusal::TooEarly { earliest, reason } => match reason {
                EarliestReason::Today => {
                    format!(
                        "Can't start before today. The earliest is {}.",
                        short_day(*earliest)
                    )
                }
                EarliestReason::ProjectStart => format!(
                    "Can't start before its project does, on {}.",
                    short_day(*earliest)
                ),
                EarliestReason::BlockedBy { title, .. } => format!(
                    "Can't start before {title} is done, on {}.",
                    short_day(*earliest)
                ),
            },
            DragRefusal::AfterEnd => "The start can't be after the end.".to_owned(),
            DragRefusal::BeforeStart => "The end can't be before the start.".to_owned(),
        }
    }
}

/// A bar's place before and after a drag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovedBar {
    pub task_id: Uuid,
    pub es: f64,
    pub ef: f64,
    pub start: Date,
    pub finish: Date,
}

/// A due date a drag pushes later because the task it follows now finishes later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuePush {
    pub task_id: Uuid,
    pub title: String,
    pub project_title: Option<String>,
    pub old_due: Date,
    pub new_due: Date,
    pub days: u32,
}

/// A project whose finish a drag changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectEffect {
    pub project_id: Uuid,
    pub title: String,
    pub finish_before: Option<Date>,
    pub finish_after: Option<Date>,
    pub late_before: Option<u32>,
    pub late_after: Option<u32>,
}

/// What a drag would do (and, once dropped, did). When `refusal` is set nothing else is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DragPreview {
    pub refusal: Option<DragRefusal>,
    /// The new values for the dragged task; None = left alone.
    pub start_date: Option<Date>,
    pub due_date: Option<Date>,
    pub estimate_days: Option<f64>,
    /// Every bar that lands somewhere else, the dragged one included.
    pub moved: Vec<MovedBar>,
    /// Due dates pushed later on tasks that follow (never earlier).
    pub pushes: Vec<DuePush>,
    pub projects: Vec<ProjectEffect>,
}

impl DragPreview {
    pub fn refused(why: DragRefusal) -> Self {
        Self {
            refusal: Some(why),
            ..Self::default()
        }
    }

    /// Nothing would change.
    pub fn is_noop(&self) -> bool {
        self.refusal.is_none()
            && self.start_date.is_none()
            && self.due_date.is_none()
            && self.estimate_days.is_none()
    }
}

/// The answer to dropping a bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DragApplied {
    pub preview: DragPreview,
    /// Due dates pushed on other tasks.
    pub pushed: u32,
}
