//! Display text for enums and numbers. Presentation only; rules live in core.

use minimap_types::{ObjectiveStatus, ProjectStatus, TaskStatus};

/// The user's own call on an objective (computed health is shown separately, spec 15).
pub fn objective_status_label(s: ObjectiveStatus) -> &'static str {
    match s {
        ObjectiveStatus::OnTrack => "On track",
        ObjectiveStatus::AtRisk => "At risk",
        ObjectiveStatus::OffTrack => "Off track",
        ObjectiveStatus::Done => "Done",
    }
}

pub fn project_status_label(s: ProjectStatus) -> &'static str {
    match s {
        ProjectStatus::Planned => "Planned",
        ProjectStatus::Active => "Active",
        ProjectStatus::Paused => "Paused",
        ProjectStatus::Done => "Done",
        ProjectStatus::Cancelled => "Cancelled",
    }
}

pub fn task_status_label(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Todo => "To do",
        TaskStatus::InProgress => "In progress",
        TaskStatus::Blocked => "Blocked",
        TaskStatus::Done => "Done",
        TaskStatus::Cancelled => "Cancelled",
    }
}

/// An estimate in days as people write it: `3d`, `0.5d`; empty when unset.
pub fn estimate_text(days: Option<f64>) -> String {
    days.map(|d| format!("{d}d")).unwrap_or_default()
}

/// 1 is the highest priority.
pub fn priority_short(p: u8) -> String {
    format!("P{p}")
}

pub fn priority_option(p: u8) -> String {
    match p {
        1 => "1 · highest".to_owned(),
        5 => "5 · lowest".to_owned(),
        _ => p.to_string(),
    }
}

/// `in_progress` -> `in progress`.
pub fn humanize(snake: &str) -> String {
    snake.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(objective_status_label(ObjectiveStatus::AtRisk), "At risk");
        assert_eq!(project_status_label(ProjectStatus::Cancelled), "Cancelled");
        assert_eq!(task_status_label(TaskStatus::InProgress), "In progress");
        assert_eq!(estimate_text(Some(3.0)), "3d");
        assert_eq!(estimate_text(Some(0.5)), "0.5d");
        assert_eq!(estimate_text(None), "");
        assert_eq!(priority_short(2), "P2");
        assert_eq!(priority_option(1), "1 · highest");
        assert_eq!(priority_option(3), "3");
        assert_eq!(humanize("in_progress"), "in progress");
        for &s in ObjectiveStatus::ALL {
            assert!(!objective_status_label(s).is_empty());
        }
    }
}
