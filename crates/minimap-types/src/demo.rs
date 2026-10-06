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
