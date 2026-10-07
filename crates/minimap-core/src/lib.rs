//! Domain logic and graph algorithms. Pure: no IO, no SQLite.

pub mod attachments;
pub mod backup;
pub mod capacity;
pub mod cycles;
pub mod decisions;
pub mod dependency_graph;
pub mod edge_rules;
pub mod export_md;
pub mod health;
pub mod impact;
pub mod layout;
pub mod links;
pub mod notes;
pub mod objectives;
pub mod overview;
pub mod projects;
pub mod quick_add;
pub mod recurrence;
pub mod report;
pub mod schedule;
pub mod search;
pub mod slug;
pub mod subtasks;
pub mod sync;
pub mod tasks;
pub mod this_week;
pub mod undo;
pub mod waiting_on;
pub mod weekly_review;

use minimap_types::PingResponse;

/// Builds the ping reply. Lives in core only to exercise the crate wiring in M0.
pub fn pong(schema_version: u32) -> PingResponse {
    PingResponse {
        message: "pong".to_owned(),
        schema_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pong_says_pong() {
        assert_eq!(pong(1).message, "pong");
    }
}
