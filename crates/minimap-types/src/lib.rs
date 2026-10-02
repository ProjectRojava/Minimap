//! Shared DTOs and enums. No IO; must compile to wasm.

use serde::{Deserialize, Serialize};

/// Response of the `ping` command (M0 smoke test of the UI <-> Rust bridge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingResponse {
    pub message: String,
    /// Schema version of the opened database (`PRAGMA user_version`).
    pub schema_version: u32,
}

/// Error returned by every command; serializes to `{ code, message }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
