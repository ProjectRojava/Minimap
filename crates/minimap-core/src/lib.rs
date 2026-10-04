//! Domain logic and graph algorithms. Pure: no IO, no SQLite.

pub mod cycles;
pub mod decisions;
pub mod edge_rules;
pub mod notes;
pub mod objectives;
pub mod projects;
pub mod quick_add;
pub mod search;
pub mod slug;
pub mod tasks;
pub mod waiting_on;

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
