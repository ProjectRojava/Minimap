//! Focus (spec 37): a task the user wants in front of them on This week every day, whatever its
//! due date. It is a way of looking at the task, not part of the plan: it never moves a date,
//! the schedule, health or capacity.

use serde::{Deserialize, Serialize};
use time::Date;

/// A task in focus. `until` is the last day it stays in focus (inclusive); without it the task
/// stays until the user takes it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Focus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Date>,
}

impl Focus {
    /// In focus until the user removes it.
    pub const PINNED: Focus = Focus { until: None };

    /// Still in focus on `today`: it has no end, or its last day has not passed.
    pub fn is_active(&self, today: Date) -> bool {
        self.until.is_none_or(|last| last >= today)
    }

    /// "until you remove it", "until 2027-03-31" (what the panel and the tooltips say).
    pub fn describe(&self) -> String {
        match self.until {
            None => "until you remove it".to_owned(),
            Some(last) => format!("until {last}"),
        }
    }
}

/// What the user picks in the Focus box of a task, before it is turned into a `Focus` (`Today`
/// and `Until` need today's date, which the backend knows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FocusChoice {
    /// Take the task out of focus.
    Off,
    /// In focus every day until it is taken out.
    Pinned,
    /// In focus for today only.
    Today,
    /// In focus every day through this date.
    Until { date: Date },
}

impl FocusChoice {
    /// What the Focus box shows for a task's focus on `today`: the choice that makes it again.
    pub fn of(focus: Option<&Focus>, today: Date) -> FocusChoice {
        match focus {
            None => FocusChoice::Off,
            Some(Focus { until: None }) => FocusChoice::Pinned,
            Some(Focus { until: Some(d) }) if *d == today => FocusChoice::Today,
            Some(Focus { until: Some(d) }) => FocusChoice::Until { date: *d },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn a_pin_has_no_end_and_a_dated_focus_ends_after_its_last_day() {
        let today = date!(2027 - 03 - 03);
        assert!(Focus::PINNED.is_active(today));
        let until = |d| Focus { until: Some(d) };
        assert!(until(date!(2027 - 03 - 03)).is_active(today));
        assert!(until(date!(2027 - 04 - 01)).is_active(today));
        assert!(!until(date!(2027 - 03 - 02)).is_active(today));
    }

    #[test]
    fn it_reads_back_from_json_with_and_without_a_date() {
        let pinned = serde_json::to_string(&Focus::PINNED).unwrap();
        assert_eq!(pinned, "{}");
        assert_eq!(
            serde_json::from_str::<Focus>(&pinned).unwrap(),
            Focus::PINNED
        );
        let dated = Focus {
            until: Some(date!(2027 - 03 - 31)),
        };
        let text = serde_json::to_string(&dated).unwrap();
        assert_eq!(text, r#"{"until":"2027-03-31"}"#);
        assert_eq!(serde_json::from_str::<Focus>(&text).unwrap(), dated);
    }

    #[test]
    fn it_describes_itself() {
        assert_eq!(Focus::PINNED.describe(), "until you remove it");
        let f = Focus {
            until: Some(date!(2027 - 03 - 31)),
        };
        assert_eq!(f.describe(), "until 2027-03-31");
    }
}
