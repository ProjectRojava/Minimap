//! Undo and redo (spec 25): the session's stack of steps and the logic that records and
//! applies them. The pure part (what undoes what) is `minimap_core::undo`; applying it is
//! `minimap_store::undo`.
//!
//! One command is one step. After any command that wrote activity, [`record_since`] turns what
//! it wrote into a step. Undo applies the step's inverse and keeps the *inverse of that* for redo
//! (taken from the rows the undo itself wrote, so redo is just undo of an undo). A new write
//! forgets whatever could have been redone. The stacks live in memory: closing the app ends the
//! session, and undo after a restart has nothing to undo (decision in the spec).

use std::sync::Mutex;

use minimap_core::undo::{headline, headline_text, plan, Inverse, Plan};
use minimap_store::{activity, nodes, Connection, StoreError};
use minimap_types::{AppError, NodeRef, UndoOutcome, UNDO_STEPS};

use crate::error::store_error;

#[derive(Debug, Clone, PartialEq)]
pub enum StepKind {
    /// Apply these to take it back.
    Undoable(Vec<Inverse>),
    /// Remember it happened; say why it can't be taken back when it comes up.
    Irreversible(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// What the user did, e.g. "archived task Fix login". The same through undo and redo.
    pub label: String,
    pub kind: StepKind,
}

#[derive(Debug, Default)]
pub struct UndoStack {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

fn push_capped(stack: &mut Vec<Step>, step: Step) {
    stack.push(step);
    if stack.len() > UNDO_STEPS {
        stack.remove(0);
    }
}

impl UndoStack {
    /// A write by the user: remember it and forget what could have been redone.
    pub fn record(&mut self, step: Step) {
        self.redo.clear();
        push_capped(&mut self.undo, step);
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[cfg(test)]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

/// A name short enough for a sentence.
fn short(label: &str) -> String {
    let label = label.trim();
    if label.chars().count() > 40 {
        let cut: String = label.chars().take(39).collect();
        format!("{cut}…")
    } else {
        label.to_owned()
    }
}

/// The step for what was written after `marker`, if there is one worth remembering.
pub fn capture(conn: &Connection, marker: i64) -> Option<Step> {
    let rows = activity::since(conn, marker).ok()?;
    if rows.is_empty() {
        return None;
    }
    let name = |node: NodeRef| {
        nodes::summary(conn, node).map_or_else(|_| "(removed)".to_owned(), |s| short(&s.label))
    };
    let label = headline(&rows).map_or_else(|| "a change".to_owned(), |h| headline_text(&h, name));
    let kind = match plan(&rows) {
        Plan::Ignore => return None,
        Plan::Irreversible(why) => StepKind::Irreversible(why),
        Plan::Steps(inverse) => StepKind::Undoable(inverse),
    };
    Some(Step { label, kind })
}

/// Records what a command wrote after `marker` (the activity marker taken before it ran).
pub fn record_since(conn: &Connection, stack: &Mutex<UndoStack>, marker: i64) {
    if activity::latest_rowid(conn).is_ok_and(|now| now <= marker) {
        return;
    }
    if let Some(step) = capture(conn, marker) {
        if let Ok(mut stack) = stack.lock() {
            stack.record(step);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Undo,
    Redo,
}

fn outcome(done: bool, message: impl Into<String>) -> UndoOutcome {
    UndoOutcome {
        done,
        message: message.into(),
    }
}

/// True for the errors that mean "things are no longer as this step expects".
fn is_stale(e: &StoreError) -> bool {
    matches!(
        e,
        StoreError::Invalid(_)
            | StoreError::NotFound { .. }
            | StoreError::EdgeNotFound(_)
            | StoreError::AlreadyArchived { .. }
            | StoreError::NotArchivedYet { .. }
            | StoreError::DuplicateEdge
            | StoreError::Constraint(_)
    )
}

/// Undoes (or redoes) the newest step. Expected trouble (nothing to do, a step that can't be
/// taken back, things changed since) is an `UndoOutcome` with `done: false` and the step is
/// dropped; only real database errors are errors (and keep the step).
pub fn step(
    conn: &mut Connection,
    stack: &mut UndoStack,
    direction: Direction,
) -> Result<UndoOutcome, AppError> {
    let (from, to, word, verb) = match direction {
        Direction::Undo => (&mut stack.undo, &mut stack.redo, "undo", "Undone"),
        Direction::Redo => (&mut stack.redo, &mut stack.undo, "redo", "Redone"),
    };
    let Some(step) = from.pop() else {
        return Ok(outcome(false, format!("Nothing to {word}")));
    };
    let inverse = match &step.kind {
        StepKind::Irreversible(why) => {
            return Ok(outcome(
                false,
                format!("Can't {word} {}: {why}", step.label),
            ));
        }
        StepKind::Undoable(inverse) => inverse,
    };
    match minimap_store::undo::apply(conn, inverse) {
        Ok(written) => {
            if let Plan::Steps(counter) = plan(&written) {
                push_capped(
                    to,
                    Step {
                        label: step.label.clone(),
                        kind: StepKind::Undoable(counter),
                    },
                );
            }
            Ok(outcome(true, format!("{verb}: {}", step.label)))
        }
        Err(e) if is_stale(&e) => Ok(outcome(
            false,
            format!("Couldn't {word} {}: {e}", step.label),
        )),
        Err(e) => {
            from.push(step);
            Err(store_error(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateTask, NodeType, Patch, TaskStatus, UpdateNote, UpdateTask,
    };
    use time::macros::date;

    fn demo() -> Connection {
        let mut conn = minimap_store::open_in_memory().unwrap();
        minimap_store::demo::seed(&mut conn, date!(2027 - 03 - 03)).unwrap();
        conn
    }

    /// Runs `f` as a command would be run: marker before, record after.
    fn command(conn: &mut Connection, stack: &Mutex<UndoStack>, f: impl FnOnce(&mut Connection)) {
        let marker = activity::latest_rowid(conn).unwrap();
        f(conn);
        record_since(conn, stack, marker);
    }

    fn task(conn: &Connection, title: &str) -> minimap_types::Task {
        minimap_store::tasks::list(conn, false)
            .unwrap()
            .into_iter()
            .find(|t| t.title == title)
            .unwrap()
    }

    fn new_task(conn: &mut Connection, title: &str) {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
    }

    fn undo(conn: &mut Connection, stack: &Mutex<UndoStack>) -> UndoOutcome {
        step(conn, &mut stack.lock().unwrap(), Direction::Undo).unwrap()
    }

    fn redo(conn: &mut Connection, stack: &Mutex<UndoStack>) -> UndoOutcome {
        step(conn, &mut stack.lock().unwrap(), Direction::Redo).unwrap()
    }

    #[test]
    fn undo_and_redo_say_what_they_did() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        let t = task(&conn, "Book the leadership offsite");
        command(&mut conn, &stack, |c| {
            minimap_store::nodes::archive(c, NodeRef::new(NodeType::Task, t.id)).unwrap();
        });
        let undone = undo(&mut conn, &stack);
        assert_eq!(
            undone,
            outcome(true, "Undone: archived task Book the leadership offsite")
        );
        assert!(task(&conn, "Book the leadership offsite")
            .archived_at
            .is_none());
        let redone = redo(&mut conn, &stack);
        assert_eq!(
            redone,
            outcome(true, "Redone: archived task Book the leadership offsite")
        );
        assert!(minimap_store::tasks::get(&conn, t.id)
            .unwrap()
            .archived_at
            .is_some());
        // And back again: the step moves between the stacks.
        assert!(undo(&mut conn, &stack).done);
        assert!(minimap_store::tasks::get(&conn, t.id)
            .unwrap()
            .archived_at
            .is_none());
    }

    #[test]
    fn steps_come_back_newest_first_and_an_empty_stack_says_so() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        assert_eq!(undo(&mut conn, &stack), outcome(false, "Nothing to undo"));
        assert_eq!(redo(&mut conn, &stack), outcome(false, "Nothing to redo"));
        command(&mut conn, &stack, |c| new_task(c, "First"));
        command(&mut conn, &stack, |c| new_task(c, "Second"));
        assert_eq!(
            undo(&mut conn, &stack).message,
            "Undone: created task Second"
        );
        assert_eq!(
            undo(&mut conn, &stack).message,
            "Undone: created task First"
        );
        assert_eq!(undo(&mut conn, &stack).message, "Nothing to undo");
        assert_eq!(
            redo(&mut conn, &stack).message,
            "Redone: created task First"
        );
        assert_eq!(
            redo(&mut conn, &stack).message,
            "Redone: created task Second"
        );
    }

    #[test]
    fn a_new_write_forgets_what_could_have_been_redone() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        command(&mut conn, &stack, |c| new_task(c, "First"));
        undo(&mut conn, &stack);
        assert_eq!(stack.lock().unwrap().redo_len(), 1);
        command(&mut conn, &stack, |c| new_task(c, "Second"));
        assert_eq!(stack.lock().unwrap().redo_len(), 0);
        assert_eq!(redo(&mut conn, &stack).message, "Nothing to redo");
    }

    #[test]
    fn only_the_last_twenty_steps_are_kept() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        for i in 0..25 {
            command(&mut conn, &stack, |c| new_task(c, &format!("Task {i}")));
        }
        assert_eq!(stack.lock().unwrap().undo_len(), UNDO_STEPS);
        for i in (5..25).rev() {
            assert_eq!(
                undo(&mut conn, &stack).message,
                format!("Undone: created task Task {i}")
            );
        }
        assert_eq!(undo(&mut conn, &stack).message, "Nothing to undo");
        // The oldest five are still there.
        assert!(task(&conn, "Task 4").archived_at.is_none());
    }

    #[test]
    fn reads_and_ignorable_writes_are_not_steps() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        command(&mut conn, &stack, |c| {
            let _ = minimap_store::tasks::list(c, false).unwrap();
        });
        // A note's text is the editor's business.
        let note = minimap_store::notes::list(&conn, false).unwrap().remove(0);
        command(&mut conn, &stack, |c| {
            minimap_store::notes::update(
                c,
                note.id,
                UpdateNote {
                    body: Some("changed".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        });
        assert_eq!(stack.lock().unwrap().undo_len(), 0);
    }

    #[test]
    fn what_cannot_be_undone_is_a_step_that_says_so_and_does_not_hide_older_ones() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        command(&mut conn, &stack, |c| new_task(c, "Kept"));
        let t = task(&conn, "Book the leadership offsite");
        command(&mut conn, &stack, |c| {
            minimap_store::nodes::archive(c, NodeRef::new(NodeType::Task, t.id)).unwrap();
        });
        command(&mut conn, &stack, |c| {
            minimap_store::nodes::delete(c, NodeRef::new(NodeType::Task, t.id)).unwrap();
        });
        let first = undo(&mut conn, &stack);
        assert!(!first.done);
        assert!(
            first.message.contains("Can't undo deleted task"),
            "{}",
            first.message
        );
        // The next Ctrl+Z goes on to the step before it, which is the archive of the (now
        // deleted) task: refused with a reason, and then the one before works.
        let second = undo(&mut conn, &stack);
        assert!(!second.done, "{}", second.message);
        assert!(
            second.message.starts_with("Couldn't undo archived task"),
            "{}",
            second.message
        );
        let third = undo(&mut conn, &stack);
        assert_eq!(third, outcome(true, "Undone: created task Kept"));
    }

    #[test]
    fn a_change_since_drops_that_step_with_a_reason_and_the_next_one_still_works() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        command(&mut conn, &stack, |c| new_task(c, "Older"));
        let t = task(&conn, "Savings review with finance");
        command(&mut conn, &stack, |c| {
            minimap_store::tasks::update(
                c,
                t.id,
                UpdateTask {
                    status: Some(TaskStatus::InProgress),
                    due_date: Patch::Clear,
                    ..Default::default()
                },
            )
            .unwrap();
        });
        // Changed behind the stack's back (another computer's merge, say).
        minimap_store::tasks::update(
            &mut conn,
            t.id,
            UpdateTask {
                status: Some(TaskStatus::Blocked),
                ..Default::default()
            },
        )
        .unwrap();
        let refused = undo(&mut conn, &stack);
        assert!(!refused.done);
        assert!(
            refused.message.contains("status was changed since"),
            "{}",
            refused.message
        );
        assert_eq!(
            minimap_store::tasks::get(&conn, t.id).unwrap().status,
            TaskStatus::Blocked
        );
        assert_eq!(
            undo(&mut conn, &stack).message,
            "Undone: created task Older"
        );
    }

    #[test]
    fn long_names_are_shortened_in_the_message() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        let long = "A task with a very long name that goes on and on and on";
        command(&mut conn, &stack, |c| new_task(c, long));
        let m = undo(&mut conn, &stack).message;
        assert_eq!(
            m,
            "Undone: created task A task with a very long name that goes …"
        );
    }

    #[test]
    fn a_whole_command_is_one_step() {
        let mut conn = demo();
        let stack = Mutex::new(UndoStack::default());
        let p = minimap_store::projects::list(&conn, false)
            .unwrap()
            .into_iter()
            .find(|p| p.title == "Platform Cost Reduction")
            .unwrap();
        command(&mut conn, &stack, |c| {
            minimap_store::projects::archive(c, p.id, minimap_types::TaskDisposition::Archive)
                .unwrap();
        });
        assert_eq!(stack.lock().unwrap().undo_len(), 1);
        let m = undo(&mut conn, &stack).message;
        assert_eq!(
            m,
            "Undone: archived project Platform Cost Reduction and 14 more changes"
        );
        assert!(minimap_store::projects::get(&conn, p.id)
            .unwrap()
            .archived_at
            .is_none());
    }
}
