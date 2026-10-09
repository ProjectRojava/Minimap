//! Meetings (spec 38): a built-in kind of task that happens at a time. It has the task's due date
//! (the day) and a start time and length. The times are the clock on the wall where the user is
//! ("10:30"), not an instant: a weekly 10:30 stays 10:30 across summer time and between devices.

use serde::{Deserialize, Serialize};
use time::Date;

use crate::{AssigneeChoice, Task};

/// The id of the built-in task type that makes a task a meeting. The list in Settings always
/// holds it; its name and colour can be edited, its id and its behaviour cannot.
pub const MEETING_TYPE: &str = "meeting";

/// How long a meeting lasts when nothing else is said.
pub const DEFAULT_MEETING_MINUTES: u32 = 60;

/// The longest a meeting can be: a day.
pub const MAX_MEETING_MINUTES: u32 = 24 * 60;

impl Task {
    /// The task is a meeting (its type is the built-in one).
    pub fn is_meeting(&self) -> bool {
        self.task_type.as_deref() == Some(MEETING_TYPE)
    }

    /// How long the meeting lasts, in minutes.
    pub fn meeting_minutes(&self) -> u32 {
        self.length_minutes.unwrap_or(DEFAULT_MEETING_MINUTES)
    }
}

/// Minutes since a fixed day zero, so wall-clock moments compare and subtract as plain numbers.
pub fn absolute_minutes(date: Date, minute: u16) -> i64 {
    i64::from(date.to_julian_day()) * 1440 + i64::from(minute)
}

impl Task {
    /// Minutes from `now` until the meeting starts (negative once it has), if it has a time.
    pub fn minutes_to_start(&self, now: &Clock) -> Option<i64> {
        Some(absolute_minutes(self.due_date?, self.start_minute?) - now.absolute())
    }

    /// Minutes from `now` until the meeting ends (negative once it has).
    pub fn minutes_to_end(&self, now: &Clock) -> Option<i64> {
        Some(self.minutes_to_start(now)? + i64::from(self.meeting_minutes()))
    }
}

impl Clock {
    /// This moment on the scale of [`absolute_minutes`].
    pub fn absolute(&self) -> i64 {
        absolute_minutes(self.date, self.minute)
    }
}

/// "09:30": a minute of the day as the clock shows it.
pub fn fmt_clock(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// A time typed as `9:30`, `09:30` or `0930` (24-hour clock) as a minute of the day.
pub fn parse_clock(text: &str) -> Result<u16, String> {
    let bad = || {
        format!(
            "“{}” is not a time. Use the 24-hour clock, like 09:30 or 14:00.",
            text.trim()
        )
    };
    let t = text.trim();
    let (h, m) = match t.split_once(':') {
        Some((h, m)) => (h, m),
        None if t.len() == 4 => t.split_at(2),
        None => return Err(bad()),
    };
    let digits = |s: &str, max: usize| {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit())
    };
    if !digits(h, 2) || !digits(m, 2) || m.len() != 2 {
        return Err(bad());
    }
    let (h, m): (u16, u16) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if h > 23 || m > 59 {
        return Err(bad());
    }
    Ok(h * 60 + m)
}

/// "10:30–11:30" for a meeting that starts at `start` and lasts `length` minutes (a meeting that
/// runs past midnight shows the end as "00:30 (+1)").
pub fn fmt_range(start: u16, length: u32) -> String {
    let end = u32::from(start) + length;
    let clock = fmt_clock((end % 1440) as u16);
    if end >= 1440 {
        format!("{}–{} (+{})", fmt_clock(start), clock, end / 1440)
    } else {
        format!("{}–{}", fmt_clock(start), clock)
    }
}

/// "1h", "30m", "1h 30m".
pub fn fmt_length(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// A length typed as `45`, `45m`, `1h`, `1.5h` or `1h30m`, in minutes.
pub fn parse_length(text: &str) -> Result<u32, String> {
    let bad = || format!("“{}” is not a length. Try 30m, 1h or 1h30m.", text.trim());
    let t = text.trim().to_lowercase().replace(' ', "");
    if t.is_empty() {
        return Err(bad());
    }
    let minutes = if let Some((h, rest)) = t.split_once('h') {
        let hours: f64 = h.parse().map_err(|_| bad())?;
        let extra = match rest.trim_end_matches('m') {
            "" => 0.0,
            m => m.parse::<f64>().map_err(|_| bad())?,
        };
        hours * 60.0 + extra
    } else {
        t.trim_end_matches('m').parse::<f64>().map_err(|_| bad())?
    };
    if !minutes.is_finite() || minutes < 1.0 || minutes > f64::from(MAX_MEETING_MINUTES) {
        return Err(format!(
            "A meeting lasts from 1 minute to {} hours.",
            MAX_MEETING_MINUTES / 60
        ));
    }
    Ok(minutes.round() as u32)
}

/// The clock on the user's wall right now, as the screen reads it. The backend has no reliable
/// way to know the local time zone, so the screen says what time it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    pub date: Date,
    /// Minute of the day, 0-1439.
    pub minute: u16,
    /// Minutes the local time is ahead of UTC (+330 in India, -300 in New York in winter).
    pub utc_offset_minutes: i32,
}

/// When a meeting is and how long it lasts: what the screen sends to make or move one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTime {
    pub date: Date,
    /// `HH:MM`, as typed.
    pub time: String,
    /// Minutes; the default length when left out.
    #[serde(default)]
    pub length_minutes: Option<u32>,
}

/// A new meeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMeeting {
    pub title: String,
    pub when: MeetingTime,
    #[serde(default)]
    pub project_id: Option<crate::Uuid>,
    #[serde(default)]
    pub assignee: AssigneeChoice,
}

/// What the clock tick changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AdvanceResult {
    /// Meetings that moved to in progress or done.
    pub changed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_read_and_print() {
        assert_eq!(parse_clock("9:30"), Ok(570));
        assert_eq!(parse_clock("09:30"), Ok(570));
        assert_eq!(parse_clock(" 0930 "), Ok(570));
        assert_eq!(parse_clock("23:59"), Ok(1439));
        assert_eq!(parse_clock("00:00"), Ok(0));
        for bad in [
            "", "24:00", "9:60", "9", "9:5", "ab:cd", "10:30pm", "-1:00", "930",
        ] {
            assert!(parse_clock(bad).is_err(), "{bad}");
        }
        assert_eq!(fmt_clock(570), "09:30");
        assert_eq!(fmt_clock(0), "00:00");
    }

    #[test]
    fn ranges_and_lengths_read_naturally() {
        assert_eq!(fmt_range(630, 60), "10:30–11:30");
        assert_eq!(fmt_range(23 * 60 + 30, 60), "23:30–00:30 (+1)");
        assert_eq!(fmt_length(60), "1h");
        assert_eq!(fmt_length(30), "30m");
        assert_eq!(fmt_length(90), "1h 30m");
        for (text, minutes) in [
            ("45", 45),
            ("45m", 45),
            ("1h", 60),
            ("1.5h", 90),
            ("1h30m", 90),
            ("1h 30", 90),
            ("2H", 120),
        ] {
            assert_eq!(parse_length(text), Ok(minutes), "{text}");
        }
        for bad in ["", "0", "0m", "soon", "25h", "-5m"] {
            assert!(parse_length(bad).is_err(), "{bad}");
        }
    }
}
