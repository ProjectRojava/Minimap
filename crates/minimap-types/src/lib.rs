//! Shared DTOs and enums. No IO; must compile to wasm.

use serde::{Deserialize, Serialize};

mod activity;
mod backup;
mod capacity;
mod demo;
mod edges;
mod enums;
mod export;
mod graph;
mod health;
mod impact;
mod links;
mod mention;
mod nodes;
mod patch;
mod quick_add;
mod recurrence;
mod review;
mod schedule;
mod security;
mod sync;
pub mod timefmt;
mod undo;
mod views;
mod week;
mod work_week;

pub use activity::*;
pub use backup::*;
pub use capacity::*;
pub use demo::*;
pub use edges::*;
pub use enums::*;
pub use export::*;
pub use graph::*;
pub use health::*;
pub use impact::*;
pub use links::*;
pub use mention::mention_token;
pub use nodes::*;
pub use patch::*;
pub use quick_add::*;
pub use recurrence::*;
pub use review::*;
pub use schedule::*;
pub use security::*;
pub use sync::*;
pub use time::Date;
pub use undo::*;
pub use uuid::Uuid;
pub use views::*;
pub use week::*;
pub use work_week::*;

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
