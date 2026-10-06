//! Which weekdays count as working days (Settings, spec 23). Shared by the schedule, capacity and
//! weekly review (pure core) and by the Settings screen. Weekdays are numbered 0 = Monday to
//! 6 = Sunday.

use serde::{Deserialize, Serialize};
use time::Weekday;

pub const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// A non-empty set of weekdays. Stored in settings as a list of weekday numbers (`[0,1,2,3,4]`).
/// The default is Monday to Friday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<u8>", into = "Vec<u8>")]
pub struct WorkWeek(u8);

impl WorkWeek {
    pub const MON_FRI: WorkWeek = WorkWeek(0b001_1111);

    /// The set of these weekday numbers (0 = Monday). Refuses an empty set or a number above 6.
    pub fn from_days(days: &[u8]) -> Result<Self, String> {
        let mut mask = 0u8;
        for &d in days {
            if d > 6 {
                return Err("a weekday is a number from 0 (Monday) to 6 (Sunday)".into());
            }
            mask |= 1 << d;
        }
        if mask == 0 {
            return Err("pick at least one working day".into());
        }
        Ok(WorkWeek(mask))
    }

    /// Whether weekday `n` (0 = Monday) is a working day.
    pub fn contains(self, n: u8) -> bool {
        n < 7 && self.0 & (1 << n) != 0
    }

    pub fn contains_weekday(self, d: Weekday) -> bool {
        self.contains(d.number_days_from_monday())
    }

    /// The same week with weekday `n` switched on or off; switching off the last day is refused
    /// (`None`).
    pub fn with(self, n: u8, on: bool) -> Option<Self> {
        if n > 6 {
            return None;
        }
        let mask = if on {
            self.0 | (1 << n)
        } else {
            self.0 & !(1 << n)
        };
        (mask != 0).then_some(WorkWeek(mask))
    }

    /// How many working days a week has (1-7).
    pub fn days_per_week(self) -> u32 {
        self.0.count_ones()
    }

    /// The working weekday numbers, Monday first.
    pub fn days(self) -> Vec<u8> {
        (0..7).filter(|&d| self.contains(d)).collect()
    }

    /// How many working weekdays come before weekday `n` (0 = Monday) within a week.
    pub fn before(self, n: u8) -> u32 {
        (self.0 & ((1u16 << n.min(7)) - 1) as u8).count_ones()
    }

    /// The weekday number of the `rank`-th working day of a week (0-based; `rank` must be below
    /// `days_per_week`).
    pub fn nth(self, rank: u32) -> u8 {
        (0..7)
            .filter(|&d| self.contains(d))
            .nth(rank as usize)
            .unwrap_or(0)
    }

    /// `Mon-Fri`, `Mon, Wed, Fri`, `Sun-Thu`: a short phrase for summaries.
    pub fn summary(self) -> String {
        let days = self.days();
        let short = |d: u8| &WEEKDAY_NAMES[d as usize][..3];
        let contiguous = days.windows(2).all(|w| w[1] == w[0] + 1);
        match days.as_slice() {
            [only] => WEEKDAY_NAMES[*only as usize].to_owned(),
            [first, .., last] if contiguous && days.len() > 2 => {
                format!("{}-{}", short(*first), short(*last))
            }
            _ => days
                .iter()
                .map(|&d| short(d))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

impl Default for WorkWeek {
    fn default() -> Self {
        Self::MON_FRI
    }
}

impl TryFrom<Vec<u8>> for WorkWeek {
    type Error = String;
    fn try_from(days: Vec<u8>) -> Result<Self, String> {
        Self::from_days(&days)
    }
}

impl From<WorkWeek> for Vec<u8> {
    fn from(w: WorkWeek) -> Vec<u8> {
        w.days()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_monday_to_friday() {
        let w = WorkWeek::default();
        assert_eq!(w.days(), vec![0, 1, 2, 3, 4]);
        assert_eq!(w.days_per_week(), 5);
        assert_eq!(w.summary(), "Mon-Fri");
        assert!(w.contains_weekday(Weekday::Friday));
        assert!(!w.contains_weekday(Weekday::Saturday));
    }

    #[test]
    fn a_week_needs_at_least_one_valid_day() {
        assert!(WorkWeek::from_days(&[]).is_err());
        assert!(WorkWeek::from_days(&[7]).is_err());
        assert_eq!(WorkWeek::from_days(&[4, 0, 0]).unwrap().days(), vec![0, 4]);
        assert!(WorkWeek::MON_FRI.with(0, false).is_some());
        let one = WorkWeek::from_days(&[2]).unwrap();
        assert!(
            one.with(2, false).is_none(),
            "the last day can't be switched off"
        );
        assert_eq!(one.with(5, true).unwrap().days(), vec![2, 5]);
    }

    #[test]
    fn counting_and_picking_days_agree() {
        let w = WorkWeek::from_days(&[0, 2, 3, 6]).unwrap();
        assert_eq!(w.before(0), 0);
        assert_eq!(w.before(1), 1);
        assert_eq!(w.before(3), 2);
        assert_eq!(w.before(6), 3);
        for rank in 0..w.days_per_week() {
            assert_eq!(w.before(w.nth(rank)), rank);
        }
    }

    #[test]
    fn it_is_stored_as_a_list_of_weekday_numbers() {
        let w = WorkWeek::from_days(&[6, 0, 1, 2, 3]).unwrap();
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, "[0,1,2,3,6]");
        assert_eq!(serde_json::from_str::<WorkWeek>(&json).unwrap(), w);
        assert!(serde_json::from_str::<WorkWeek>("[]").is_err());
        assert!(serde_json::from_str::<WorkWeek>("[9]").is_err());
    }

    #[test]
    fn summaries_read_naturally() {
        assert_eq!(
            WorkWeek::from_days(&[6, 0, 1, 2, 3]).unwrap().summary(),
            "Mon, Tue, Wed, Thu, Sun"
        );
        assert_eq!(
            WorkWeek::from_days(&[0, 2, 4]).unwrap().summary(),
            "Mon, Wed, Fri"
        );
        assert_eq!(WorkWeek::from_days(&[0, 1]).unwrap().summary(), "Mon, Tue");
        assert_eq!(WorkWeek::from_days(&[2]).unwrap().summary(), "Wednesday");
        assert_eq!(
            WorkWeek::from_days(&[0, 1, 2, 3, 4, 5, 6])
                .unwrap()
                .summary(),
            "Mon-Sun"
        );
    }
}
