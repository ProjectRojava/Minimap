//! Undo and redo (spec 25).

use serde::{Deserialize, Serialize};

/// What `undo_last` / `redo_last` did, as a sentence for a toast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoOutcome {
    /// Something changed (so screens should reload).
    pub done: bool,
    /// "Undone: archived task Fix login", "Nothing to undo", "Couldn't undo ...: ...".
    pub message: String,
}

/// Steps kept for undo (and for redo).
pub const UNDO_STEPS: usize = 20;
