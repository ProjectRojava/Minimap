# 25 — Undo

Status: Implemented — awaiting manual check · Milestone: M4 · Priority: Should
Depends on: 01

## Goal
Make mistakes cheap: undo the last action(s).

## Scope
**In** (built)
- **Ctrl/Cmd+Z undoes the last change**, with a toast ("Undone: archived task Fix login"); **Ctrl/Cmd+Shift+Z (and Ctrl+Y) redoes** ("Redone: …"). Also in the command palette ("Undo last change", "Redo"). In a text field Ctrl/Cmd+Z keeps doing what it always does (undo the typing); the shortcut works everywhere else. Screens reload after an undo.
- **Built from the activity log**: after every command that writes, the rows it wrote are turned into the steps that take it back (`minimap_core::undo::plan`, pure); `minimap_store::undo::apply` runs them in one transaction through the ordinary repository functions, so the undo is logged like any other write (an archive writes `archived`, a restore `unarchived`, and so on).
- **One command = one step** (see decision below). Up to **20 steps**, in memory, for the session.
- **What is taken back**: create (the item is archived; nothing is ever hard-deleted by undo), update (old values put back, including clearing a date and a task's completion), archive and restore (with the links archived along), link added, link removed (rules and loops are checked again), a project archived with its tasks or with the tasks moved to the inbox, a changed manager, a superseded decision (the link and the status).
- **Commands**: `undo_last()` (spec) and `redo_last()`, both answering `UndoOutcome{done, message}`.

## Rules
- **Safe by checking first.** Every field restore first checks that the field still holds the value the step recorded; if something else changed it since (a later edit, a merge from another computer) the step is refused with a reason ("Couldn't undo changed due date of task X: \"X\"'s due date was changed since") and **nothing at all is written**; the step is dropped so the next Ctrl+Z goes on to the one before.
- **Not part of undo, and how that shows**:
  - *Ignored* (no step): edits to a note (its text is never in the log, and the editor has its own undo; this includes its title, date and kind), the `mentions` links a note's text produces, the first-run "me", settings, and reads.
  - *Irreversible* (a step that says so when it comes up, then is skipped, so Ctrl+Z never silently undoes something older than what you just did): a **hard delete**, an **attachment** added or removed, a **link's details** (weight, lag, role, ...) changed. "Can't undo deleted task: deleted items can't be brought back".
- **Redo is undo of the undo**: the rows an undo wrote become the step that redoes it, so redo needs no code of its own and survives any number of back-and-forths. A new change forgets what could have been redone.
- **The history is forgotten** when the data is replaced wholesale: restoring a backup, recovering a Drive checkpoint, adopting or merging another computer's data on connecting, and adding the demo data.

## Acceptance criteria
- [x] Undo works for create, update, archive, edge add and edge remove (store tests `tests/undo.rs` do a full cycle for each on the demo data: do it, undo it, redo it, undo again, comparing a fingerprint of every active row and link each time).
- [x] Undo after app restart does nothing: the stacks are in memory only (decision below).
- [ ] Click-through (see below).

## Decisions
- **Redo needed? Yes.** It comes almost for free (undo of the undo) and "I undid one too many" is the first thing people do after undo exists.
- **Group multi-row actions as one step, without a `batch_id`.** A command's boundary *is* the batch: the app notes the newest activity row before a command and takes everything written after it, so quick-add, bulk create, "archive a project with its tasks" and `apply_slips` are one step each with no schema change. (A `batch_id` column would add a migration, a merge change and a sync format change for no extra power.)
- **Session-scoped.** Undo after a restart does nothing: with the app closed, other computers may have changed the data, and the activity log can't say what is safe to take back. (The log itself is permanent history.)
- **Undo of a creation archives instead of deleting.** A hard delete would be irreversible (and write a tombstone that follows the item to every computer); an archived item keeps redo possible and the history whole. It does leave an archived row behind, like any archived item.
- **No new activity action for undo.** The ordinary actions are already the truthful record of what happened; "logged as an action" is met by that.
- **Irreversible things are steps, not silence**, so that Ctrl+Z never undoes an older change while you think it undid the latest one.

## Implementation notes
- Types: `UndoOutcome`, `UNDO_STEPS` (20).
- Core `undo`: `plan(rows) -> Plan{Ignore, Irreversible, Steps(Vec<Inverse>)}`, `Inverse{Archive, Unarchive, Restore{node, old, new}, RemoveEdge, AddEdge}`, `headline` / `headline_text` ("archived task X and 3 more changes"; the main thing is the command's last node row). Links touching a node created, archived or restored in the same step are left to that node's archive or restore.
- Store: `undo::apply(conn, &[Inverse]) -> Vec<Activity>` (one transaction; refuses on any mismatch), `activity::latest_rowid` / `since`; the repositories gained `update_in_tx` (objectives, projects, people, teams, decisions) and `nodes::unarchive_in_tx`. `undo::nullable_fields` lists each `Update*`'s `Patch` fields (a test checks it against the types); when a nullable field is added to an `Update*`, add it there.
- Commands: `undo_last`, `redo_last` (`commands/undo.rs`); `crate::undo` has `UndoStack`, `capture`, `record_since`, `step`. `AppState::run` records a step after every command that wrote (`run_vault` does not, so the undo commands, backups, security and sync are not steps); `AppState::forget_undo` clears the history.
- UI: `nav::undo_key` (pure, tested), the handler in `keyboard.rs`, `state::undo_or_redo`, palette actions `Undo` / `Redo`, `api::undo_last` / `redo_last`.

## Not yet verified by hand
- Archive a task, press Ctrl+Z (not in a text box): it comes back with its links and a toast names it; Ctrl+Shift+Z archives it again
- create a task from the palette or quick-add, undo: it disappears; redo: it returns
- change a due date, status and priority, undo each in turn
- archive a project with its tasks, undo: everything returns as one step
- set someone's manager, undo: the old manager is back
- delete an archived item for good, Ctrl+Z: "Can't undo deleted ..." and the next Ctrl+Z undoes the step before
- Ctrl+Z inside a text box still undoes the typing only
- after a restart Ctrl+Z says "Nothing to undo"
