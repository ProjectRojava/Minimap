//! App-wide reactive state, provided as contexts from `App`.

use std::time::Duration;

use leptos::prelude::*;
use minimap_types::{AppError, NodeRef};

use crate::nav::move_cursor;

/// The node shown in the detail pane, if any.
#[derive(Clone, Copy)]
pub struct Selection(pub RwSignal<Option<NodeRef>>);

impl Selection {
    pub fn open(&self, node: NodeRef) {
        self.0.set(Some(node));
    }

    pub fn close(&self) {
        self.0.set(None);
    }
}

/// Rows of the list screen currently shown, so `j`/`k`/`Enter` work on any list.
/// A list screen calls `set_items` with its rows and renders them with `NodeRow`.
#[derive(Clone, Copy)]
pub struct ListNav {
    items: RwSignal<Vec<NodeRef>>,
    pub cursor: RwSignal<Option<usize>>,
}

impl ListNav {
    pub fn new() -> Self {
        Self {
            items: RwSignal::new(Vec::new()),
            cursor: RwSignal::new(None),
        }
    }

    pub fn set_items(&self, items: Vec<NodeRef>) {
        self.items.set(items);
        self.cursor.set(None);
    }

    pub fn clear(&self) {
        self.set_items(Vec::new());
    }

    pub fn step(&self, delta: isize) {
        let len = self.items.with_untracked(Vec::len);
        self.cursor.update(|c| *c = move_cursor(*c, len, delta));
    }

    /// The node under the cursor.
    pub fn current(&self) -> Option<NodeRef> {
        let c = self.cursor.get_untracked()?;
        self.items.with_untracked(|i| i.get(c).copied())
    }
}

impl Default for ListNav {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub is_error: bool,
    pub message: String,
    pub code: Option<String>,
}

const TOAST_SECONDS: u64 = 6;

#[derive(Clone, Copy)]
pub struct Toasts {
    pub items: RwSignal<Vec<Toast>>,
    next_id: RwSignal<u64>,
}

impl Toasts {
    pub fn new() -> Self {
        Self {
            items: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
        }
    }

    fn push(&self, is_error: bool, message: String, code: Option<String>) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.items.update(|v| {
            v.push(Toast {
                id,
                is_error,
                message,
                code,
            })
        });
        let this = *self;
        set_timeout(move || this.dismiss(id), Duration::from_secs(TOAST_SECONDS));
    }

    pub fn error(&self, e: &AppError) {
        self.push(true, e.message.clone(), Some(e.code.clone()));
    }

    #[allow(dead_code)] // first used by save confirmations in feature 03+
    pub fn info(&self, message: impl Into<String>) {
        self.push(false, message.into(), None);
    }

    pub fn dismiss(&self, id: u64) {
        self.items.update(|v| v.retain(|t| t.id != id));
    }
}

impl Default for Toasts {
    fn default() -> Self {
        Self::new()
    }
}
