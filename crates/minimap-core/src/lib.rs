//! Domain logic and graph algorithms. Pure: no IO, no SQLite.
//! Populated from M2 onward.

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
