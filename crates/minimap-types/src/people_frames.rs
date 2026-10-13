//! Your people (docs/product-design/06-your-people.md, spec 49): compact frames for you and the
//! people who report to you. They show load and neglect (yours, of them), never output: no
//! scores, no ranking, no health for people.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::NodeSummary;

/// A fact worth a pill on a frame, in the fixed order of this enum. They are facts, not
/// judgements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameFlag {
    /// This week's load is above 100% of their capacity.
    Over,
    /// More open tasks than the limit in Settings.
    ManyTasks,
    /// It has been this many days since the last 1:1 (more than the rhythm in Settings).
    OneOnOneDue { days: u32 },
    /// This many of the things you wait on them for are stale.
    WaitingStale { count: u32 },
    /// This many of their open tasks are blocked.
    Blocked { count: u32 },
    /// This many of their open tasks are past their due date.
    Overdue { count: u32 },
}

/// One person at a glance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub person: NodeSummary,
    pub role_title: String,
    pub is_self: bool,
    /// This week's load, the number the Capacity heatmap shows for the person's current-week
    /// cell; `None` when there is no capacity to measure it against.
    pub load_pct: Option<u32>,
    /// Load above 100%.
    pub over: bool,
    /// More open tasks than the limit in Settings.
    pub many_tasks: bool,
    pub open_tasks: u32,
    pub blocked_tasks: u32,
    pub overdue_tasks: u32,
    /// Open, not snoozed waiting-ons about them (what *you* wait on *them* for). Always 0 for you.
    pub waiting_on_them: u32,
    pub waiting_stale: u32,
    /// The newest 1:1 note that mentions them. `None` for you.
    pub last_one_on_one: Option<Date>,
    /// Days since the last 1:1 (or since they were added, when there has never been one), once it
    /// is more than the rhythm in Settings; `None` otherwise.
    pub one_on_one_due: Option<u32>,
    pub flags: Vec<FrameFlag>,
}

/// Why the card has no reports to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FramesHint {
    /// Nobody reports to you yet: set managers on People.
    NoReports,
}

/// The people who report to one person below you, shown under that manager.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportGroup {
    pub manager: Uuid,
    /// 1 = they report to one of your direct reports, 2 = to one of those, and so on.
    pub depth: u32,
    pub frames: Vec<Frame>,
}

/// You, then your direct reports by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeopleFrames {
    pub me: Frame,
    pub reports: Vec<Frame>,
    /// Everyone further down the reporting tree, a group for each manager that has people under
    /// them, each group followed by the groups of its members (so a manager's people sit right
    /// under them). Only with the setting on (it is, by default).
    pub below: Vec<ReportGroup>,
    pub hint: Option<FramesHint>,
}
