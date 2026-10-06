//! Demo data (spec 24): what `seed_demo_data` added, for the Developer tab's message.

use serde::{Deserialize, Serialize};

/// How many of each thing the demo data added.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoSummary {
    pub objectives: u32,
    pub projects: u32,
    pub tasks: u32,
    pub people: u32,
    pub teams: u32,
    pub notes: u32,
    pub decisions: u32,
    pub waiting_ons: u32,
    /// Links between them (blocks, assignments, memberships, ...).
    pub links: u32,
}

/// How demo data was found in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemoSource {
    /// Demo data added by this version: the ids were written down when it was added.
    Recorded,
    /// Demo data added before ids were written down: recognised by its exact titles.
    Titles,
}

/// What removing the demo data does to things that are *not* demo data. Your own items are
/// never deleted: they are detached from the demo items they pointed at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoImpact {
    /// Links between a demo item and one of yours; they go with the demo item.
    pub your_links: u32,
    /// Your tasks in a demo project: they move to the inbox.
    pub tasks_to_inbox: u32,
    /// Your projects owned by a demo person: they lose the owner.
    pub owners_cleared: u32,
    /// Your teams inside a demo team: they become top-level teams.
    pub teams_unparented: u32,
    /// Demo people that stay, because a waiting-on of yours is about them.
    pub people_kept: u32,
}

/// Is there demo data to remove, and what would removing it do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoStatus {
    pub found: bool,
    pub source: Option<DemoSource>,
    /// What would be removed.
    pub items: DemoSummary,
    pub impact: DemoImpact,
}

/// What removing the demo data did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoRemoval {
    pub removed: DemoSummary,
    pub impact: DemoImpact,
    /// The backup saved just before the removal (full path).
    pub backup: Option<String>,
}

impl DemoSummary {
    /// "2 objectives, 3 projects, 40 tasks, ..." for a toast.
    pub fn describe(&self) -> String {
        let part =
            |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        [
            part(self.objectives, "objective", "objectives"),
            part(self.projects, "project", "projects"),
            part(self.tasks, "task", "tasks"),
            part(self.people, "person", "people"),
            part(self.teams, "team", "teams"),
            part(self.notes, "note", "notes"),
            part(self.decisions, "decision", "decisions"),
            part(self.waiting_ons, "waiting-on", "waiting-ons"),
            part(self.links, "link", "links"),
        ]
        .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_reads_as_a_list_with_plurals() {
        let s = DemoSummary {
            objectives: 1,
            projects: 3,
            tasks: 40,
            people: 8,
            teams: 2,
            notes: 3,
            decisions: 4,
            waiting_ons: 3,
            links: 120,
        };
        assert_eq!(
            s.describe(),
            "1 objective, 3 projects, 40 tasks, 8 people, 2 teams, 3 notes, 4 decisions, 3 waiting-ons, 120 links"
        );
    }
}
