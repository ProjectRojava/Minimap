//! App-wide reactive state, provided as contexts from `App`.

use std::time::Duration;

use leptos::{prelude::*, task::spawn_local};
use leptos_router::hooks::use_location;
use minimap_types::{AppError, Clock, LinkRelation, NodeRef, Task, TaskStatus, Uuid, ZoomStep};

use crate::{
    nav::move_cursor,
    zoom::{focus_of, in_target, join_route, out_target, project_route, route_for, WORLD_ROUTE},
};

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

/// Text the palette opens with next time: a quick-add line filled in from another screen
/// (the Gantt's *Add a project* / *Add a task*, spec 58).
#[derive(Clone, Copy)]
pub struct PaletteText(pub RwSignal<Option<String>>);

impl PaletteText {
    /// Opens the palette with `text` in its box, the cursor at the end.
    pub fn open_with(self, palette: PaletteOpen, text: String) {
        self.0.set(Some(text));
        palette.0.set(true);
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
    /// A cursor position to put back on the next rows that arrive (a zoom in returning to a
    /// screen), with the time it was asked for in ms.
    restore: StoredValue<Option<(usize, f64)>>,
}

/// How long a cursor waits for its screen's rows before it is forgotten, in ms.
const RESTORE_WAIT_MS: f64 = 3000.0;

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
            restore: StoredValue::new(None),
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
        // A cursor waiting for its screen's rows lands on the first rows that arrive.
        let waiting = if len == 0 {
            None
        } else {
            self.restore
                .try_update_value(Option::take)
                .flatten()
                .filter(|(_, asked)| js_sys::Date::now() - asked < RESTORE_WAIT_MS)
                .map(|(i, _)| i)
        };
        self.cursor.update(|c| {
            *c = match waiting {
                Some(i) => Some(i.min(len - 1)),
                None => c.and_then(|i| if len == 0 { None } else { Some(i.min(len - 1)) }),
            };
        });
    }

    /// Puts the cursor back on row `i` (a zoom in returning to where it was). On the screen that
    /// is already showing it moves now; for a screen that is about to open it waits for the
    /// screen's rows.
    pub fn restore_cursor(&self, i: usize, screen_changes: bool) {
        if screen_changes {
            self.restore.set_value(Some((i, js_sys::Date::now())));
        } else {
            let len = self.items.with_untracked(Vec::len);
            if len > 0 {
                self.cursor.set(Some(i.min(len - 1)));
            }
        }
    }

    /// Whether a screen's rows include this node (it is on the screen).
    pub fn contains(&self, node: NodeRef) -> bool {
        self.items.with_untracked(|i| i.contains(&node))
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
    /// Offers *Undo* (the last change, as `Ctrl/Cmd+Z`).
    pub undo: bool,
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

    fn push(&self, is_error: bool, message: String, code: Option<String>, undo: bool) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.items.update(|v| {
            v.push(Toast {
                id,
                is_error,
                message,
                code,
                undo,
            })
        });
        let this = *self;
        set_timeout(move || this.dismiss(id), Duration::from_secs(TOAST_SECONDS));
    }

    pub fn error(&self, e: &AppError) {
        self.push(true, e.message.clone(), Some(e.code.clone()), false);
    }

    pub fn info(&self, message: impl Into<String>) {
        self.push(false, message.into(), None, false);
    }

    /// A message about a change just made, with an *Undo* button.
    pub fn info_with_undo(&self, message: impl Into<String>) {
        self.push(false, message.into(), None, true);
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

/// The two dialogs that can follow finishing a task: *Finish decision* (spec 42), the Decision
/// task that was being finished, and *Resolve waiting-ons* (spec 56), the task that was just
/// finished while something was still being waited for. Each holds the task while it is open.
#[derive(Clone, Copy)]
pub struct FinishDecision {
    pub decision: RwSignal<Option<Uuid>>,
    pub waiting: RwSignal<Option<Uuid>>,
}

impl FinishDecision {
    pub fn new() -> Self {
        Self {
            decision: RwSignal::new(None),
            waiting: RwSignal::new(None),
        }
    }

    pub fn open(&self, task: Uuid) {
        self.decision.set(Some(task));
    }

    pub fn close(&self) {
        self.decision.set(None);
    }

    pub fn open_waiting(&self, task: Uuid) {
        self.waiting.set(Some(task));
    }

    pub fn close_waiting(&self) {
        self.waiting.set(None);
    }
}

impl Default for FinishDecision {
    fn default() -> Self {
        Self::new()
    }
}

/// The error code of a Decision task that can't be finished yet.
pub const DECISION_INCOMPLETE: &str = "decision_incomplete";

/// How long after a task's completion time a change still counts as "just finished it".
const JUST_FINISHED_SECONDS: i64 = 15;

/// Whether this change is the one that finished the task: done, and completed a moment ago (an
/// edit to a task finished long ago keeps its old completion time).
pub fn just_finished(
    status: TaskStatus,
    completed_at: Option<time::OffsetDateTime>,
    now: time::OffsetDateTime,
) -> bool {
    status == TaskStatus::Done
        && completed_at.is_some_and(|at| {
            let age = (now - at).whole_seconds();
            (-JUST_FINISHED_SECONDS..=JUST_FINISHED_SECONDS).contains(&age)
        })
}

fn now_utc() -> Option<time::OffsetDateTime> {
    let nanos = (js_sys::Date::now() * 1_000_000.0) as i128;
    time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()
}

/// [`finish`] for a change to a task. If it was refused because the task is a Decision with no
/// decision and reason written yet, the *Finish decision* dialog opens instead of an error toast
/// (views still reload, so a dragged card goes back to its column). If the change finished the
/// task and something is still being waited for on it, the *Resolve waiting-ons* dialog follows.
pub fn finish_task(
    result: Result<Task, AppError>,
    task: Uuid,
    toasts: Toasts,
    version: DataVersion,
    gate: FinishDecision,
) -> Option<Task> {
    if let Err(e) = &result {
        if e.code == DECISION_INCOMPLETE {
            version.bump();
            gate.open(task);
            return None;
        }
    }
    let done = finish(result, toasts, version)?;
    if now_utc().is_some_and(|now| just_finished(done.status, done.completed_at, now)) {
        spawn_local(async move {
            let open = crate::api::get_task_waiting(task)
                .await
                .map(|rows| rows.iter().any(|r| r.waiting.resolved_on.is_none()))
                .unwrap_or(false);
            if open {
                gate.open_waiting(task);
            }
        });
    }
    Some(done)
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

// ------------------------------------------------------------------------------------- zoom

/// A place to come back to: where the screen was before a zoom out (spec 52).
#[derive(Clone, Debug, PartialEq)]
pub struct ZoomFrame {
    pub route: String,
    pub selection: Option<NodeRef>,
    pub cursor: Option<usize>,
}

/// A move waiting for `ZoomHost` to carry it out: go to `route`, open `selection` in the pane
/// (none closes it), and remember `push` so a zoom in comes back.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoomMove {
    pub route: String,
    pub selection: Option<NodeRef>,
    pub cursor: Option<usize>,
    pub push: Option<ZoomFrame>,
}

/// Zoom (spec 52): moving between the whole portfolio, an objective, a project and an item, and
/// back to exactly where you were. `Alt+Up` / `Alt+Down`, the breadcrumb and the palette call
/// [`out`](Self::out), [`inn`](Self::inn) and [`go_to`](Self::go_to).
#[derive(Clone, Copy)]
pub struct ZoomCtx {
    /// Where to come back to, newest last. A normal navigation empties it (`ZoomHost`).
    pub stack: RwSignal<Vec<ZoomFrame>>,
    /// The move waiting to be carried out.
    pub request: RwSignal<Option<ZoomMove>>,
    /// The project the Tasks screen is filtered to, while that screen is open.
    pub page_project: RwSignal<Option<Uuid>>,
    /// A project filter the Tasks screen should take (`Some(None)` = none), set when a zoom
    /// lands on it.
    pub pending_project: RwSignal<Option<Option<Uuid>>>,
    /// The route the last zoom went to, so the arrival is not mistaken for a navigation.
    pub expected: StoredValue<Option<String>>,
    selection: Selection,
    list: ListNav,
    toasts: Toasts,
    pathname: Memo<String>,
    search: Memo<String>,
}

impl ZoomCtx {
    /// Provides the context. Call inside the `<Router>`.
    pub fn provide() -> Self {
        let location = use_location();
        let ctx = Self {
            stack: RwSignal::new(Vec::new()),
            request: RwSignal::new(None),
            page_project: RwSignal::new(None),
            pending_project: RwSignal::new(None),
            expected: StoredValue::new(None),
            selection: expect_context::<Selection>(),
            list: expect_context::<ListNav>(),
            toasts: expect_context::<Toasts>(),
            pathname: location.pathname,
            search: location.search,
        };
        provide_context(ctx);
        ctx
    }

    /// The screen you are on, with its query.
    pub fn route(&self) -> String {
        join_route(&self.pathname.get_untracked(), &self.search.get_untracked())
    }

    /// The route that brings the screen back as it is: a Tasks board filtered by its dropdown
    /// has no query, so the project is written into it.
    fn return_route(&self) -> String {
        if self.pathname.get_untracked() == "/tasks" {
            if let Some(project) = self.page_project.get_untracked() {
                return project_route(project);
            }
        }
        self.route()
    }

    /// What zooming starts from: the item open in the pane, else the filtered project.
    pub fn focus(&self) -> Option<NodeRef> {
        focus_of(
            self.selection.0.get_untracked(),
            self.page_project.get_untracked(),
        )
    }

    /// Zooms out one level from what you are looking at.
    pub fn out(self) {
        let Some(node) = self.focus() else {
            // Already the widest view of a screen: out is the Overview.
            if self.pathname.get_untracked() != WORLD_ROUTE {
                self.go(&ZoomStep::world(), true);
            }
            return;
        };
        spawn_local(async move {
            match crate::api::get_zoom_path(node).await {
                Ok(path) => {
                    if let Some(step) = out_target(&path) {
                        self.go(step, true);
                    }
                }
                Err(e) => self.toasts.error(&e),
            }
        });
    }

    /// Zooms back in to where the last zoom out came from; with nothing to go back to, into the
    /// item under the list cursor.
    pub fn inn(self) {
        if let Some(frame) = self.stack.try_update(|s| s.pop()).flatten() {
            self.request.set(Some(ZoomMove {
                route: frame.route,
                selection: frame.selection,
                cursor: frame.cursor,
                push: None,
            }));
            return;
        }
        let Some(row) = self.list.current() else {
            return;
        };
        let focus = self.focus();
        spawn_local(async move {
            match crate::api::get_zoom_path(row).await {
                Ok(path) => {
                    if let Some(step) = in_target(focus, &path) {
                        self.go(step, false);
                    }
                }
                Err(e) => self.toasts.error(&e),
            }
        });
    }

    /// Zooms to a step of a path (a breadcrumb was clicked); a zoom in comes back to here.
    pub fn go_to(self, step: &ZoomStep) {
        self.go(step, true);
    }

    fn go(self, step: &ZoomStep, push: bool) {
        let here = self.route();
        let visible = step
            .node
            .as_ref()
            .is_some_and(|n| self.list.contains(n.node));
        let (route, selection) = route_for(step, &here, visible);
        if route == here && selection == self.selection.0.get_untracked() {
            return;
        }
        let push = push.then(|| ZoomFrame {
            route: self.return_route(),
            selection: self.selection.0.get_untracked(),
            cursor: self.list.cursor.get_untracked(),
        });
        self.request.set(Some(ZoomMove {
            route,
            selection,
            cursor: None,
            push,
        }));
    }
}

#[cfg(test)]
mod finish_tests {
    use super::*;
    use time::{macros::datetime, Duration};

    #[test]
    fn only_the_change_that_finished_the_task_counts() {
        let now = datetime!(2027-03-15 10:00:00 UTC);
        let done = TaskStatus::Done;
        assert!(just_finished(done, Some(now), now));
        assert!(just_finished(done, Some(now - Duration::seconds(10)), now));
        // An edit to a task finished earlier keeps its old completion time.
        assert!(!just_finished(done, Some(now - Duration::minutes(5)), now));
        assert!(!just_finished(done, Some(now - Duration::days(3)), now));
        // Not done, or no completion time: nothing was finished.
        assert!(!just_finished(TaskStatus::InProgress, Some(now), now));
        assert!(!just_finished(done, None, now));
    }
}
