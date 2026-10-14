//! The radar (docs/product-design/10-radar-window.md, ADR-0021): a small window that shows your
//! world centred on you. Core decides *what* goes *where* in abstract polar units
//! (`minimap_core::radar::build`); the UI turns that into pixels. A view: it stores nothing.
//!
//! Angles are radians, clockwise from 12 o'clock. Radii run from 0 (the centre: you) to 1 (the
//! outer edge).

use serde::{Deserialize, Serialize};
use time::Date;

use crate::{FogLevel, NodeSummary, Severity, View};

/// The rings, as working days from today: this week, the next two weeks, this quarter.
pub const DEFAULT_RADAR_RINGS: [u32; 3] = [5, 10, 60];
/// The farthest a ring can be set (a working year).
pub const MAX_RADAR_RING_DAYS: u32 = 260;

/// Whether these rings can be drawn: each between 1 and [`MAX_RADAR_RING_DAYS`] and getting
/// larger.
pub fn validate_radar_rings(rings: [u32; 3]) -> Result<(), String> {
    if rings.iter().any(|d| !(1..=MAX_RADAR_RING_DAYS).contains(d)) {
        return Err(format!(
            "A ring is between 1 and {MAX_RADAR_RING_DAYS} working days."
        ));
    }
    if rings[0] >= rings[1] || rings[1] >= rings[2] {
        return Err(
            "The rings must get larger: the second is farther out than the first, \
                    the third farther than the second."
                .into(),
        );
    }
    Ok(())
}

/// What a blip is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlipKind {
    Task,
    Project,
    /// Today's or tomorrow's meeting: a small hollow square, no health.
    Meeting,
    /// A ping that has come due on something that is not itself a blip.
    Ping,
}

/// How big a blip is drawn: from priority (1 large, 2 medium, the rest small).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlipSize {
    S,
    M,
    L,
}

/// The shape a blip is drawn in, so health reads without colour: filled red, half amber, empty
/// green, a dash for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadarMark {
    Red,
    Amber,
    Green,
    None,
}

/// A slice of the circle: one objective (or "Other", which serves none).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sector {
    /// `None` is the "Other" sector (items that serve no objective), always last.
    pub objective: Option<NodeSummary>,
    /// The objective's colour rank: its place among all active objectives by id, which is what
    /// gives it its hue everywhere else (ADR-0012). `None` for "Other".
    pub hue_rank: Option<u32>,
    /// Where it starts (radians clockwise from 12 o'clock) and how wide it is. The sweeps add up
    /// to a full turn.
    pub start: f64,
    pub sweep: f64,
}

/// One item that matters, placed by how soon it is due and which objective it serves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blip {
    pub node: NodeSummary,
    pub kind: BlipKind,
    /// 0 = the centre (you, now) to 1 = the outer edge.
    pub r: f64,
    pub theta: f64,
    pub size: BlipSize,
    pub mark: RadarMark,
    pub fog: FogLevel,
    /// The item has a live ping (the pin badge).
    pub pinged: bool,
    /// The hover text: "Gateway auth · EU Region · blocked · due Fri".
    pub label: String,
}

/// Trouble approaching your work (spec 45), at the rim, in the sector of what it will reach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeMark {
    pub theta: f64,
    pub severity: Severity,
    /// "Gateway auth (EU Region) is blocked; reaches Pen test in 3 days."
    pub label: String,
    /// The thing in trouble.
    pub source: NodeSummary,
}

/// The line at the bottom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footer {
    /// Open tasks past their due date.
    pub overdue: u32,
    pub due_today: u32,
    /// Pings whose day has come.
    pub pings_due: u32,
    /// Items approaching your work that hit or threaten it.
    pub approaching: u32,
    /// The share of work in play that is clear (fog of war), when anything is in play.
    pub visibility: Option<u8>,
    /// Blips left off because there were more than the radar shows.
    pub hidden_blips: u32,
}

impl Footer {
    /// "2 overdue · 1 ping today · visibility 64%"; "Nothing in range this week" when there is
    /// nothing to say.
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if self.overdue > 0 {
            parts.push(format!("{} overdue", self.overdue));
        }
        if self.due_today > 0 {
            parts.push(format!("{} due today", self.due_today));
        }
        if self.pings_due > 0 {
            parts.push(format!(
                "{} {} today",
                self.pings_due,
                if self.pings_due == 1 { "ping" } else { "pings" }
            ));
        }
        if self.approaching > 0 {
            parts.push(format!("{} approaching", self.approaching));
        }
        if parts.is_empty() && self.hidden_blips == 0 {
            return "Nothing in range this week".to_owned();
        }
        if self.hidden_blips > 0 {
            parts.push(format!("{} more not shown", self.hidden_blips));
        }
        if let Some(v) = self.visibility {
            parts.push(format!("visibility {v}%"));
        }
        parts.join(" · ")
    }
}

/// Everything the radar draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Radar {
    pub today: Date,
    /// The view the radar was worked out in (My view falls back to Portfolio without a "me").
    pub view: View,
    /// The rings, as working days from today.
    pub rings: [u32; 3],
    pub sectors: Vec<Sector>,
    /// In angle order (clockwise from 12 o'clock): the order `Tab` walks them.
    pub blips: Vec<Blip>,
    pub edge_marks: Vec<EdgeMark>,
    pub footer: Footer,
    /// The activity rowid at build time: the radar polls `get_data_revision` and asks again when
    /// it moves.
    pub revision: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_must_grow_and_stay_in_range() {
        assert!(validate_radar_rings(DEFAULT_RADAR_RINGS).is_ok());
        assert!(validate_radar_rings([1, 2, 260]).is_ok());
        assert!(validate_radar_rings([0, 2, 3]).is_err());
        assert!(validate_radar_rings([5, 5, 60]).is_err());
        assert!(validate_radar_rings([5, 10, 10]).is_err());
        assert!(validate_radar_rings([10, 5, 60]).is_err());
        assert!(validate_radar_rings([5, 10, 261]).is_err());
    }

    #[test]
    fn the_footer_reads_like_the_example() {
        let footer = Footer {
            overdue: 2,
            pings_due: 1,
            visibility: Some(64),
            ..Footer::default()
        };
        assert_eq!(footer.text(), "2 overdue · 1 ping today · visibility 64%");
        let more = Footer {
            pings_due: 3,
            hidden_blips: 4,
            ..Footer::default()
        };
        assert_eq!(more.text(), "3 pings today · 4 more not shown");
    }

    #[test]
    fn an_empty_footer_says_so() {
        assert_eq!(Footer::default().text(), "Nothing in range this week");
        let quiet = Footer {
            visibility: Some(90),
            ..Footer::default()
        };
        assert_eq!(quiet.text(), "Nothing in range this week");
    }
}
