//! App-wide reactive state, provided as contexts from `App`.

use std::time::Duration;

use leptos::prelude::*;
use minimap_types::{AppError, Clock, LinkRelation, NodeRef, Uuid};

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

/// Whether the command palette is open (`Ctrl/Cmd+K`).
#[derive(Clone, Copy)]
pub struct PaletteOpen(pub RwSignal<bool>);

impl PaletteOpen {
    pub fn toggle(&self) {
        self.0.update(|o| *o = !*o);
    }
}

/// The "link a task" dialog (from a card's menu or a task's panel): which task it starts from
/// and whether it opens on *New task* or *Existing task*.
#[derive(Clone, Copy)]
pub struct LinkDialog(pub RwSignal<Option<LinkRequest>>);

#[derive(Clone, Debug, PartialEq)]
pub struct LinkRequest {
    pub task: Uuid,
    pub label: String,
    /// Open on the *Existing task* tab (otherwise *New task*).
    pub existing: bool,
    /// The relation that starts chosen.
    pub relation: LinkRelation,
}

impl LinkDialog {
    pub fn open(&self, task: Uuid, label: impl Into<String>, existing: bool) {
        self.open_as(task, label, existing, LinkRelation::Blocks);
    }

    /// Like [`open`](Self::open), with the relation already chosen (the "Part of" buttons).
    pub fn open_as(
        &self,
        task: Uuid,
        label: impl Into<String>,
        existing: bool,
        relation: LinkRelation,
    ) {
        self.0.set(Some(LinkRequest {
            task,
            label: label.into(),
            existing,
            relation,
        }));
    }

    pub fn close(&self) {
        self.0.set(None);
    }
}

/// The clock on the user's wall, kept fresh by the meeting clock every half minute (spec 38), so
/// "in 35 min" labels move on their own. `None` until the first tick.
#[derive(Clone, Copy)]
pub struct NowClock(pub RwSignal<Option<Clock>>);

/// The "new meeting" dialog (spec 38): a plain new meeting, or a follow-up to the meeting named.
#[derive(Clone, Copy)]
pub struct MeetingDialog(pub RwSignal<Option<MeetingRequest>>);

#[derive(Clone, Debug, PartialEq)]
pub struct MeetingRequest {
    /// The meeting a follow-up is for (its id and title), or none for a new meeting.
    pub follow_up_of: Option<(Uuid, String)>,
}

impl MeetingDialog {
    pub fn new_meeting(&self) {
        self.0.set(Some(MeetingRequest { follow_up_of: None }));
    }

    pub fn follow_up(&self, source: Uuid, title: impl Into<String>) {
        self.0.set(Some(MeetingRequest {
            follow_up_of: Some((source, title.into())),
        }));
    }

    pub fn close(&self) {
        self.0.set(None);
    }
}

/// The ⋯ menu of a Kanban card that is open, if any: one at a time, drawn once at the top of the
/// app so no card can paint over it.
#[derive(Clone, Copy)]
pub struct CardMenuState(pub RwSignal<Option<MenuRequest>>);

#[derive(Clone, Debug, PartialEq)]
pub struct MenuRequest {
    pub task: Uuid,
    pub label: String,
    /// Top-left corner of the menu, in viewport pixels.
    pub left: f64,
    pub top: f64,
}

/// The "what if" scenario being explored: slips on tasks or projects. "What if this slips?"
/// buttons fill it and open the screen; the screen edits it.
#[derive(Clone, Copy)]
pub struct Scenario(pub RwSignal<Vec<minimap_types::Slip>>);

impl Scenario {
    /// Starts a scenario with one slip.
    pub fn start(&self, node: NodeRef, days: u32) {
        self.0.set(vec![minimap_types::Slip { node, days }]);
    }
}

/// Rows of the list screen currently shown, so `j`/`k`/`Enter` work on any list.
/// A list screen calls `set_items` with its rows and renders them with `NodeRow`.
#[derive(Clone, Copy)]
pub struct ListNav {
    items: RwSignal<Vec<NodeRef>>,
    pub cursor: RwSignal<Option<usize>>,
    /// Bumped by the `n` shortcut; list screens open their "new" form in response.
    pub new_request: RwSignal<u64>,
    /// The last row shortcut (`x`, `1`-`5`, ...) pressed with a row under the cursor.
    pub row_key: RwSignal<Option<RowKey>>,
}

/// A row shortcut. `seq` grows with every press so repeating a key is still a change.
#[derive(Clone, PartialEq)]
pub struct RowKey {
    pub key: String,
    pub seq: u64,
}

impl ListNav {
    pub fn new() -> Self {
        Self {
            items: RwSignal::new(Vec::new()),
            cursor: RwSignal::new(None),
            new_request: RwSignal::new(0),
            row_key: RwSignal::new(None),
        }
    }

    pub fn request_new(&self) {
        self.new_request.update(|n| *n += 1);
    }

    pub fn send_row_key(&self, key: &str) {
        self.row_key.update(|k| {
            let seq = k.as_ref().map_or(0, |k| k.seq) + 1;
            *k = Some(RowKey {
                key: key.to_owned(),
                seq,
            });
        });
    }

    /// Runs `f` each time the `n` shortcut is pressed while this screen is mounted.
    pub fn on_new(&self, f: impl Fn() + 'static) {
        let request = self.new_request;
        Effect::new(move |previous: Option<u64>| {
            let now = request.get();
            if previous.is_some() {
                f();
            }
            now
        });
    }

    /// Runs `f(key, node)` for each row shortcut pressed on the row under the cursor while this
    /// screen is mounted (presses from before it mounted are ignored).
    pub fn on_row_key(&self, f: impl Fn(String, NodeRef) + 'static) {
        let this = *self;
        let seen = this.row_key.get_untracked().map_or(0, |k| k.seq);
        Effect::new(move |_| {
            if let Some(k) = this.row_key.get() {
                if k.seq > seen {
                    if let Some(node) = this.current() {
                        f(k.key, node);
                    }
                }
            }
        });
    }

    /// Replaces the rows, keeping the cursor on the same position (clamped) so a
    /// refresh after an edit doesn't lose your place.
    pub fn set_items(&self, items: Vec<NodeRef>) {
        let len = items.len();
        self.items.set(items);
        self.cursor.update(|c| {
            *c = c.and_then(|i| if len == 0 { None } else { Some(i.min(len - 1)) });
        });
    }

    /// Leaving a screen: forget its rows and the cursor.
    pub fn clear(&self) {
        self.items.set(Vec::new());
        self.cursor.set(None);
        self.row_key.set(None);
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

/// Bumped after every write so lists and the detail pane reload.
#[derive(Clone, Copy)]
pub struct DataVersion(RwSignal<u64>);

impl DataVersion {
    pub fn new() -> Self {
        Self(RwSignal::new(0))
    }

    /// Read inside a resource's source closure to reload it on every write.
    pub fn track(&self) -> u64 {
        self.0.get()
    }

    pub fn bump(&self) {
        self.0.update(|v| *v += 1);
    }
}

impl Default for DataVersion {
    fn default() -> Self {
        Self::new()
    }
}

/// Finishes a write: errors become a toast, and either way views reload (so a rejected
/// edit snaps controls back to the stored value).
pub fn finish<T>(result: Result<T, AppError>, toasts: Toasts, version: DataVersion) -> Option<T> {
    version.bump();
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            toasts.error(&e);
            None
        }
    }
}

/// Undo (or redo) the last change, as the keyboard shortcut and the palette do: the answer is a
/// toast ("Undone: archived task X", or why nothing was), and views reload when something changed.
pub fn undo_or_redo(redo: bool, toasts: Toasts, version: DataVersion) {
    leptos::task::spawn_local(async move {
        let result = if redo {
            crate::api::redo_last().await
        } else {
            crate::api::undo_last().await
        };
        match result {
            Ok(outcome) => {
                toasts.info(outcome.message);
                if outcome.done {
                    version.bump();
                }
            }
            Err(e) => toasts.error(&e),
        }
    });
}
