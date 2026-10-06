//! Recurring items (spec 27): the rule a task or note repeats by, and how it reads.
//! The date arithmetic is in `minimap_core::recurrence`.

use serde::{Deserialize, Serialize};

use crate::WEEKDAY_NAMES;

/// How often something comes round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cadence {
    /// Every day.
    Daily,
    /// Every `every` weeks (1-52) on `weekday` (0 = Monday ... 6 = Sunday).
    Weekly { every: u32, weekday: u8 },
    /// On this day of the month (1-31); a month without that day uses its last day.
    Monthly { day: u8 },
}

/// A repeat rule. `template` is the text each new *note* starts with (a task copies its own
/// fields instead).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recurrence {
    pub cadence: Cadence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

impl From<Cadence> for Recurrence {
    fn from(cadence: Cadence) -> Self {
        Recurrence {
            cadence,
            template: None,
        }
    }
}

/// 1st, 2nd, 3rd, 4th, 11th, 21st, ...
fn ordinal(n: u8) -> String {
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

impl Cadence {
    /// "every day", "every Monday", "every 2 weeks on Monday", "monthly on the 15th".
    pub fn describe(&self) -> String {
        match *self {
            Cadence::Daily => "every day".to_owned(),
            Cadence::Weekly { every: 1, weekday } => {
                format!("every {}", WEEKDAY_NAMES[usize::from(weekday.min(6))])
            }
            Cadence::Weekly { every, weekday } => format!(
                "every {every} weeks on {}",
                WEEKDAY_NAMES[usize::from(weekday.min(6))]
            ),
            Cadence::Monthly { day } if day > 28 => {
                format!("monthly on the {} (or the last day)", ordinal(day))
            }
            Cadence::Monthly { day } => format!("monthly on the {}", ordinal(day)),
        }
    }

    /// The text that makes this rule again (what `every:` takes in quick-add and what the
    /// Repeats box shows): `day`, `mon`, `2w:mon`, `month:15`.
    pub fn shorthand(&self) -> String {
        match *self {
            Cadence::Daily => "day".to_owned(),
            Cadence::Weekly { every: 1, weekday } => {
                WEEKDAY_NAMES[usize::from(weekday.min(6))][..3].to_lowercase()
            }
            Cadence::Weekly { every, weekday } => format!(
                "{every}w:{}",
                WEEKDAY_NAMES[usize::from(weekday.min(6))][..3].to_lowercase()
            ),
            Cadence::Monthly { day } => format!("month:{day}"),
        }
    }
}

impl Recurrence {
    pub fn describe(&self) -> String {
        self.cadence.describe()
    }

    pub fn shorthand(&self) -> String {
        self.cadence.shorthand()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_read_as_sentences() {
        assert_eq!(Cadence::Daily.describe(), "every day");
        assert_eq!(
            Cadence::Weekly {
                every: 1,
                weekday: 0
            }
            .describe(),
            "every Monday"
        );
        assert_eq!(
            Cadence::Weekly {
                every: 2,
                weekday: 4
            }
            .describe(),
            "every 2 weeks on Friday"
        );
        assert_eq!(Cadence::Monthly { day: 1 }.describe(), "monthly on the 1st");
        assert_eq!(
            Cadence::Monthly { day: 12 }.describe(),
            "monthly on the 12th"
        );
        assert_eq!(
            Cadence::Monthly { day: 22 }.describe(),
            "monthly on the 22nd"
        );
        assert_eq!(
            Cadence::Monthly { day: 23 }.describe(),
            "monthly on the 23rd"
        );
        assert_eq!(
            Cadence::Monthly { day: 31 }.describe(),
            "monthly on the 31st (or the last day)"
        );
    }

    #[test]
    fn the_shorthand_is_what_every_takes() {
        assert_eq!(Cadence::Daily.shorthand(), "day");
        assert_eq!(
            Cadence::Weekly {
                every: 1,
                weekday: 2
            }
            .shorthand(),
            "wed"
        );
        assert_eq!(
            Cadence::Weekly {
                every: 3,
                weekday: 6
            }
            .shorthand(),
            "3w:sun"
        );
        assert_eq!(Cadence::Monthly { day: 15 }.shorthand(), "month:15");
    }

    #[test]
    fn it_is_stored_as_plain_json() {
        let r = Recurrence::from(Cadence::Weekly {
            every: 2,
            weekday: 0,
        });
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"cadence":{"kind":"weekly","every":2,"weekday":0}}"#
        );
        let with = Recurrence {
            cadence: Cadence::Monthly { day: 1 },
            template: Some("## Agenda".into()),
        };
        let back: Recurrence =
            serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
        assert_eq!(back, with);
        assert_eq!(
            serde_json::from_str::<Recurrence>(r#"{"cadence":{"kind":"daily"}}"#).unwrap(),
            Cadence::Daily.into()
        );
    }
}
