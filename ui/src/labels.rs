//! Display text for enums and numbers. Presentation only; rules live in core.

use minimap_types::{DecisionStatus, NoteKind, ObjectiveStatus, ProjectStatus, TaskStatus};

use crate::components::page::Tone;

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

pub fn decision_status_label(s: DecisionStatus) -> &'static str {
    match s {
        DecisionStatus::Proposed => "Proposed",
        DecisionStatus::Decided => "Decided",
        DecisionStatus::Superseded => "Superseded",
    }
}

/// Where each status stands, in colour (see `Tone`).
pub fn project_status_tone(s: ProjectStatus) -> Tone {
    match s {
        ProjectStatus::Planned | ProjectStatus::Cancelled => Tone::Neutral,
        ProjectStatus::Active => Tone::Accent,
        ProjectStatus::Paused => Tone::Warning,
        ProjectStatus::Done => Tone::Success,
    }
}

pub fn objective_status_tone(s: ObjectiveStatus) -> Tone {
    match s {
        ObjectiveStatus::OnTrack => Tone::Success,
        ObjectiveStatus::AtRisk => Tone::Warning,
        ObjectiveStatus::OffTrack => Tone::Danger,
        ObjectiveStatus::Done => Tone::Neutral,
    }
}

pub fn task_status_tone(s: TaskStatus) -> Tone {
    match s {
        TaskStatus::Todo | TaskStatus::Cancelled => Tone::Neutral,
        TaskStatus::InProgress => Tone::Accent,
        TaskStatus::Blocked => Tone::Warning,
        TaskStatus::Done => Tone::Success,
    }
}

pub fn decision_status_tone(s: DecisionStatus) -> Tone {
    match s {
        DecisionStatus::Proposed => Tone::Accent,
        DecisionStatus::Decided => Tone::Success,
        DecisionStatus::Superseded => Tone::Neutral,
    }
}

pub fn note_kind_tone(k: NoteKind) -> Tone {
    match k {
        NoteKind::OneOnOne => Tone::Accent,
        NoteKind::Meeting | NoteKind::General => Tone::Neutral,
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

    #[test]
    fn statuses_have_sensible_tones() {
        // Good is green, trouble is amber or red, under way is the accent, the rest is grey.
        assert_eq!(task_status_tone(TaskStatus::Done), Tone::Success);
        assert_eq!(task_status_tone(TaskStatus::InProgress), Tone::Accent);
        assert_eq!(task_status_tone(TaskStatus::Blocked), Tone::Warning);
        assert_eq!(task_status_tone(TaskStatus::Todo), Tone::Neutral);
        assert_eq!(project_status_tone(ProjectStatus::Active), Tone::Accent);
        assert_eq!(project_status_tone(ProjectStatus::Paused), Tone::Warning);
        assert_eq!(project_status_tone(ProjectStatus::Done), Tone::Success);
        assert_eq!(
            objective_status_tone(ObjectiveStatus::OnTrack),
            Tone::Success
        );
        assert_eq!(
            objective_status_tone(ObjectiveStatus::AtRisk),
            Tone::Warning
        );
        assert_eq!(
            objective_status_tone(ObjectiveStatus::OffTrack),
            Tone::Danger
        );
        assert_eq!(decision_status_tone(DecisionStatus::Decided), Tone::Success);
        assert_eq!(
            decision_status_tone(DecisionStatus::Superseded),
            Tone::Neutral
        );
        assert_eq!(note_kind_tone(NoteKind::OneOnOne), Tone::Accent);
        assert_eq!(note_kind_tone(NoteKind::General), Tone::Neutral);
        // Only trouble is ever red.
        for &s in ProjectStatus::ALL {
            assert_ne!(project_status_tone(s), Tone::Danger);
        }
    }
}
