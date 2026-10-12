//! Fog of war (spec 43): how long ago something that is in play was touched or confirmed.
//! Fog is a way of looking: it never changes health, status or the schedule.

use serde::{Deserialize, Serialize};
use time::Date;

use crate::NodeRef;

/// How stale an item in play is. `Haze` = a while; `Fog` = long enough to ask "is this still so?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FogLevel {
    Clear,
    Haze,
    Fog,
}

/// What the last sighting was, for the hover text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FogReason {
    /// The item (or what vouches for it) was last changed or written about.
    NoChange,
    /// The last sighting was someone saying "still accurate".
    OnlyConfirmed,
}

/// One item in play that has gone quiet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FogMark {
    pub node: NodeRef,
    /// The item's title, for lists that show only marks (the weekly review's Fog step).
    pub label: String,
    pub level: FogLevel,
    /// Whole days since it was last seen.
    pub age_days: u32,
    pub last_seen: Date,
    pub reason: FogReason,
}

/// Every item in play that is not clear, and how much of what is in play is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FogReport {
    pub today: Date,
    /// Only `Haze` and `Fog`, `Fog` first and then the oldest first.
    pub marks: Vec<FogMark>,
    /// Whole percent of what is in play that is clear (haze counts half); `None` when nothing is
    /// in play.
    pub visibility: Option<u8>,
    /// How many items are in play.
    pub in_play: u32,
    /// How many of them are clear.
    pub clear: u32,
}

impl FogReport {
    /// The mark of an item, if it is fogged.
    pub fn mark(&self, id: uuid::Uuid) -> Option<&FogMark> {
        self.marks.iter().find(|m| m.node.id == id)
    }
}

/// Days without a change after which an item in play hazes, and after which it is fog. Strictly
/// greater than: a task 7 days old is clear with the defaults, 8 is haze.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FogThresholds {
    pub task_haze: u32,
    pub task_fog: u32,
    pub project_haze: u32,
    pub project_fog: u32,
    pub objective_haze: u32,
    pub objective_fog: u32,
}

impl Default for FogThresholds {
    fn default() -> Self {
        Self {
            task_haze: 7,
            task_fog: 14,
            project_haze: 10,
            project_fog: 21,
            objective_haze: 14,
            objective_fog: 30,
        }
    }
}

impl FogThresholds {
    /// Haze must come before fog, and days are 1-365.
    pub fn validate(&self) -> Result<(), String> {
        let pair = |name: &str, haze: u32, fog: u32| -> Result<(), String> {
            if haze < 1 || fog > 365 || haze > 365 {
                return Err(format!("{name} must be between 1 and 365 days"));
            }
            if haze >= fog {
                return Err(format!("{name}: haze must come before fog"));
            }
            Ok(())
        };
        pair("Tasks", self.task_haze, self.task_fog)?;
        pair("Projects", self.project_haze, self.project_fog)?;
        pair("Objectives", self.objective_haze, self.objective_fog)
    }
}

/// The fogged tasks listed in This week's *Gone quiet* section, at most.
pub const QUIET_LIMIT: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid_and_haze_comes_before_fog() {
        assert!(FogThresholds::default().validate().is_ok());
        let bad = FogThresholds {
            task_haze: 14,
            task_fog: 14,
            ..Default::default()
        };
        assert!(bad
            .validate()
            .unwrap_err()
            .contains("haze must come before fog"));
    }

    #[test]
    fn days_stay_within_a_year() {
        let zero = FogThresholds {
            project_haze: 0,
            ..Default::default()
        };
        assert!(zero.validate().is_err());
        let long = FogThresholds {
            objective_fog: 366,
            ..Default::default()
        };
        assert!(long.validate().is_err());
    }

    #[test]
    fn levels_order_from_clear_to_fog() {
        assert!(FogLevel::Clear < FogLevel::Haze && FogLevel::Haze < FogLevel::Fog);
    }
}
