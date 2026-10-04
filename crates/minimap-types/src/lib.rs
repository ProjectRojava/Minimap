//! Shared DTOs and enums. No IO; must compile to wasm.

use serde::{Deserialize, Serialize};

mod activity;
mod edges;
mod enums;
mod health;
mod impact;
mod mention;
mod nodes;
mod patch;
mod quick_add;
mod schedule;
pub mod timefmt;
mod views;

pub use activity::*;
pub use edges::*;
pub use enums::*;
pub use health::*;
pub use impact::*;
pub use mention::mention_token;
pub use nodes::*;
pub use patch::*;
pub use quick_add::*;
pub use schedule::*;
pub use time::Date;
pub use uuid::Uuid;
pub use views::*;

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
