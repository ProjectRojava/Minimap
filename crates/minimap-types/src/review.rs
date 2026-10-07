//! Weekly review and the status report (spec 19).

use serde::{Deserialize, Serialize};
use time::Date;

use crate::{
    DecisionRow, NodeSummary, ObjectiveHealthRow, OverloadedPerson, OverviewCounts, ReviewDue,
    RiskItem, WaitingOnRow, WeekTask,
};

/// Something that slipped during the review week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlipKind {
    /// A task's due date was moved later during the week.
    DueMoved,
    /// A task came due during the week and is still open.
    Overdue,
    /// A project's or objective's target date was moved later during the week.
    TargetMoved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewSlip {
    pub node: NodeSummary,
    pub kind: SlipKind,
    /// The project a task belongs to.
    pub project: Option<String>,
    pub assignee: Option<NodeSummary>,
    /// The date before the move (`DueMoved`, `TargetMoved`).
    pub from: Option<Date>,
    /// The date now: where it was moved to, or the due date that passed.
    pub to: Option<Date>,
    /// Working days later (`DueMoved`, `TargetMoved`) or calendar days overdue (`Overdue`).
    pub days: u32,
}

/// An open task with status Blocked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewBlocked {
    pub task: WeekTask,
    /// It became blocked during the review week.
    pub newly_blocked: bool,
}

/// A task finished during the week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDone {
    pub node: NodeSummary,
    pub project: Option<String>,
    pub assignee: Option<String>,
    pub priority: u8,
    pub completed_on: Date,
}

/// A project or objective marked done during the week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewFinished {
    pub node: NodeSummary,
    pub on: Date,
}

/// What the weekly review shows (and the status report is made from). Weeks run Monday to Sunday.
/// "Slipped", "blocked since", "done" and "finished" come from the activity log and the week's
/// dates; capacity, stale waiting-ons, health and risks are the state right now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeeklyReview {
    pub week_start: Date,
    pub week_end: Date,
    pub prev_week_start: Date,
    pub next_week_start: Date,
    pub today: Date,
    pub is_current_week: bool,
    pub slipped: Vec<ReviewSlip>,
    /// Every task blocked now, tasks that became blocked this week first.
    pub blocked: Vec<ReviewBlocked>,
    /// Over capacity this week or next, or over the open-task limit.
    pub overloaded: Vec<OverloadedPerson>,
    /// Open waiting-ons that are stale, oldest first.
    pub waiting: Vec<WaitingOnRow>,
    /// Waiting-ons resolved during the week.
    pub waiting_resolved: Vec<WaitingOnRow>,
    /// Decisions made (not merely proposed) during the week, newest first.
    pub decisions: Vec<DecisionRow>,
    pub done: Vec<ReviewDone>,
    pub finished: Vec<ReviewFinished>,
    /// How many projects are red, amber, green.
    pub counts: OverviewCounts,
    pub objectives: Vec<ObjectiveHealthRow>,
    /// Ongoing objectives whose review is overdue or falls by the end of the week.
    #[serde(default)]
    pub reviews: Vec<ReviewDue>,
    pub risks: Vec<RiskItem>,
    pub more_risks: u32,
    pub warnings: Vec<String>,
}

/// Which report `export_markdown` / `render_report` produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    /// The board / executive summary of a week.
    WeeklyStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportParams {
    /// Any date in the week to report on (this week when omitted).
    pub week_start: Option<Date>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
}

/// `{{name}}` placeholders a report template may use, with what each stands for. Shared so the
/// renderer, the validator and the Settings help agree.
pub const REPORT_PLACEHOLDERS: &[(&str, &str)] = &[
    ("title", "The report title"),
    ("week_start", "First day of the week (Monday)"),
    ("week_end", "Last day of the week (Sunday)"),
    ("generated_on", "The day the report was made"),
    ("summary", "Headline numbers"),
    ("objectives", "Objectives with their health, as a table"),
    ("risks", "The top risks"),
    ("done", "What got done"),
    ("slipped", "What slipped"),
    ("blocked", "What is blocked"),
    ("decisions", "Decisions made"),
    ("capacity", "People over capacity"),
    ("waiting", "Waiting-ons that are stale or were resolved"),
];

/// The template used until the user edits it (Settings).
pub const DEFAULT_REPORT_TEMPLATE: &str = "\
# {{title}}

Week of {{week_start}} to {{week_end}} · prepared {{generated_on}}

## Summary

{{summary}}

## Objectives

{{objectives}}

## Top risks

{{risks}}

## Completed

{{done}}

## Slipped

{{slipped}}

## Blocked

{{blocked}}

## Decisions

{{decisions}}

## People and capacity

{{capacity}}

## Waiting on others

{{waiting}}
";
