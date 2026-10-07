//! Display text for enums and numbers. Presentation only; rules live in core.

use std::str::FromStr;

use minimap_types::{
    Date, DecisionStatus, NoteKind, ObjectiveStatus, ProjectStatus, SubtaskProgress, TaskStatus,
};

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

/// The tone of a dropdown's stored value (`in_progress`), for the status dropdowns. A value
/// that is not a status of that kind is neutral.
pub fn task_status_value_tone(v: &str) -> Tone {
    TaskStatus::from_str(v).map_or(Tone::Neutral, task_status_tone)
}

pub fn project_status_value_tone(v: &str) -> Tone {
    ProjectStatus::from_str(v).map_or(Tone::Neutral, project_status_tone)
}

pub fn objective_status_value_tone(v: &str) -> Tone {
    ObjectiveStatus::from_str(v).map_or(Tone::Neutral, objective_status_tone)
}

pub fn decision_status_value_tone(v: &str) -> Tone {
    DecisionStatus::from_str(v).map_or(Tone::Neutral, decision_status_tone)
}

/// The tone of a status word of any kind of item (a list that mixes projects, tasks and
/// objectives): `done` is good, `blocked` needs attention, and so on.
pub fn status_word_tone(word: &str) -> Tone {
    if let Ok(s) = TaskStatus::from_str(word) {
        return task_status_tone(s);
    }
    if let Ok(s) = ProjectStatus::from_str(word) {
        return project_status_tone(s);
    }
    objective_status_value_tone(word)
}

/// What a tinted dropdown takes: its stored value to a tone.
pub type TintOf = fn(&str) -> Tone;

pub const TASK_STATUS_TINT: TintOf = task_status_value_tone;
pub const PROJECT_STATUS_TINT: TintOf = project_status_value_tone;
pub const OBJECTIVE_STATUS_TINT: TintOf = objective_status_value_tone;
pub const DECISION_STATUS_TINT: TintOf = decision_status_value_tone;
pub const PRIORITY_TINT: TintOf = priority_value_tone;

/// P1 and P2 need attention; the rest is nothing special.
pub fn priority_tone(p: u8) -> Tone {
    if p <= 2 {
        Tone::Warning
    } else {
        Tone::Neutral
    }
}

/// The tone of a dropdown's priority value (`"1"`).
pub fn priority_value_tone(v: &str) -> Tone {
    v.parse().map_or(Tone::Neutral, priority_tone)
}

/// A deadline starts to warm this many days before it (a fortnight).
pub const HEAT_WINDOW_DAYS: i64 = 14;
/// The share of red (percent) in the fill on the due day; past it, one step more.
const HEAT_AT_DEADLINE: i64 = 30;
const HEAT_OVERDUE: i64 = 36;

/// How red an open task's fill is, in percent of the danger colour mixed into its normal
/// background: nothing until [`HEAT_WINDOW_DAYS`] before the due date, then rising a little each
/// day to [`HEAT_AT_DEADLINE`] on the day, and [`HEAT_OVERDUE`] once it is past. `None` for a
/// closed task, no due date or an unknown today.
pub fn deadline_heat(due: Option<Date>, today: Option<Date>, open: bool) -> Option<u8> {
    let (due, today) = (due?, today?);
    if !open {
        return None;
    }
    let days_left = (due - today).whole_days();
    let heat = if days_left < 0 {
        HEAT_OVERDUE
    } else if days_left >= HEAT_WINDOW_DAYS {
        0
    } else {
        HEAT_AT_DEADLINE * (HEAT_WINDOW_DAYS - days_left) / HEAT_WINDOW_DAYS
    };
    u8::try_from(heat).ok().filter(|h| *h > 0)
}

/// The "2/5" shown on a task that has subtasks, and its tone: green once all are done.
pub fn subtask_chip(p: SubtaskProgress) -> Option<(String, Tone)> {
    (p.total > 0).then(|| {
        let tone = if p.done == p.total {
            Tone::Success
        } else {
            Tone::Neutral
        };
        (format!("{}/{}", p.done, p.total), tone)
    })
}

/// The overlay strength (`--heat`, 0 to 1) for a heat in percent: `30` -> `0.30`.
pub fn heat_strength(percent: u8) -> String {
    format!("{:.2}", f32::from(percent) / 100.0)
}

/// How a date stands against today for something still open: red when past, amber on the day,
/// grey otherwise (and always grey when it is closed or today is unknown).
pub fn date_tone(date: Date, today: Option<Date>, open: bool) -> Tone {
    match today {
        Some(t) if open && date < t => Tone::Danger,
        Some(t) if open && date == t => Tone::Warning,
        _ => Tone::Neutral,
    }
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
    fn dropdown_values_and_status_words_get_the_tone_of_their_status() {
        assert_eq!(task_status_value_tone("blocked"), Tone::Warning);
        assert_eq!(task_status_value_tone("in_progress"), Tone::Accent);
        assert_eq!(project_status_value_tone("paused"), Tone::Warning);
        assert_eq!(objective_status_value_tone("off_track"), Tone::Danger);
        assert_eq!(decision_status_value_tone("decided"), Tone::Success);
        assert_eq!(task_status_value_tone("nonsense"), Tone::Neutral);
        // A mixed list: every status of every kind reads the same as its typed tone.
        for &s in TaskStatus::ALL {
            assert_eq!(status_word_tone(s.as_str()), task_status_tone(s));
        }
        for &s in ProjectStatus::ALL {
            let word = status_word_tone(s.as_str());
            // `done` and `cancelled` are also task statuses, with the same tones.
            assert_eq!(word, project_status_tone(s), "{s:?}");
        }
        for &s in &[
            ObjectiveStatus::OnTrack,
            ObjectiveStatus::AtRisk,
            ObjectiveStatus::OffTrack,
        ] {
            assert_eq!(status_word_tone(s.as_str()), objective_status_tone(s));
        }
    }

    #[test]
    fn a_deadline_warms_from_a_fortnight_out_and_never_cools() {
        let today = Date::from_calendar_date(2027, time::Month::March, 1).unwrap();
        let heat =
            |days: i64| deadline_heat(Some(today + time::Duration::days(days)), Some(today), true);
        assert_eq!(heat(HEAT_WINDOW_DAYS), None);
        assert_eq!(heat(30), None);
        assert!(heat(HEAT_WINDOW_DAYS - 1).is_some());
        assert_eq!(heat(0), Some(30));
        assert_eq!(heat_strength(30), "0.30");
        assert_eq!(heat_strength(7), "0.07");
        assert_eq!(heat(-1), Some(36));
        assert_eq!(heat(-90), Some(36));
        // Each day closer is at least as red as the day before.
        let mut last = 0;
        for days in (-3..=HEAT_WINDOW_DAYS).rev() {
            let now = heat(days).unwrap_or(0);
            assert!(now >= last, "{days}: {now} < {last}");
            last = now;
        }
        // Nothing to warm: closed, no due date, no today.
        let due = Some(today);
        assert_eq!(deadline_heat(due, Some(today), false), None);
        assert_eq!(deadline_heat(None, Some(today), true), None);
        assert_eq!(deadline_heat(due, None, true), None);
    }

    #[test]
    fn the_subtask_chip_shows_progress_and_turns_green_when_all_are_done() {
        let chip = |done, total| subtask_chip(SubtaskProgress { done, total });
        assert_eq!(chip(0, 0), None);
        assert_eq!(chip(2, 5), Some(("2/5".to_owned(), Tone::Neutral)));
        assert_eq!(chip(3, 3), Some(("3/3".to_owned(), Tone::Success)));
    }

    #[test]
    fn priority_and_dates_are_coloured_by_what_needs_attention() {
        assert_eq!(priority_tone(1), Tone::Warning);
        assert_eq!(priority_tone(2), Tone::Warning);
        assert_eq!(priority_tone(3), Tone::Neutral);
        assert_eq!(priority_value_tone("1"), Tone::Warning);
        assert_eq!(priority_value_tone("x"), Tone::Neutral);
        let (past, today, later) = (
            Date::from_calendar_date(2027, time::Month::March, 1).unwrap(),
            Date::from_calendar_date(2027, time::Month::March, 3).unwrap(),
            Date::from_calendar_date(2027, time::Month::March, 9).unwrap(),
        );
        assert_eq!(date_tone(past, Some(today), true), Tone::Danger);
        assert_eq!(date_tone(today, Some(today), true), Tone::Warning);
        assert_eq!(date_tone(later, Some(today), true), Tone::Neutral);
        assert_eq!(date_tone(past, Some(today), false), Tone::Neutral);
        assert_eq!(date_tone(past, None, true), Tone::Neutral);
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
