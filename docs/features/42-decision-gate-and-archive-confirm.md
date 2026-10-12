# 42 — Finishing decisions, what is automatic, and confirming every archive

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 10, 25, 32, 41

## Goal
A Decision task is only done when the decision is written; what Minimap fills in is told apart from what the user set; and nothing is archived without an "Are you sure?" in its own dialog.

## Scope
**In**
- **Decided on fills itself.** Finishing a Decision task decides its record in the same transaction: a *proposed* record becomes `decided` today and gets today's date unless it already has one. A record that is already `decided` or `superseded` is left alone, and a date the user gave is kept (`store::decisions::decide_with_task_in_tx`). Reopening the task leaves the record as it is.
- **The gate.** Moving a Decision task to Done is refused (`StoreError::DecisionIncomplete`, command code `decision_incomplete`) unless its record has a non-blank **Decision** and **Rationale** (`decisions::check_can_finish`, called from `tasks::update_core`). A task with no record is refused too. Cancelling is not held up; undo/redo (`tasks::restore_in_tx`), sync merges and creating a task already done are not gated.
- **Finish decision dialog** (`components/finish_decision.rs`). Wherever a task is moved to Done (task pane, board drag and `x`/`s`, list `x`/`s` and its status box, This week's tick and `x`) a `decision_incomplete` result (`state::finish_task`) opens the dialog with the two boxes, pre-filled with what is written. *Decide and mark done* (`finish_decision_task`, `decisions::finish_task`) makes the record if needed, writes the two texts and finishes the task in one transaction (one undo step). A dragged card goes back to its column. Other paths (weekly review, bulk) show the error text.
- **Auto versus you.** Two flags on a decision, `auto_status` and `auto_decided_on` (migration 0016, `Decision` and `UpdateDecision`): set when Minimap fills the status (finishing the task) or the date (finishing the task, or stamped when a decision is decided without one) and cleared when the user sets that field. Undo restores them (they are in the activity diff). The decision pane shows an `AUTO` / `YOU` tag per field (`decision_panel::provenance`).
- **Archive asks, in a dialog, every time** (`components/confirm_archive.rs`, `ConfirmArchive` context, host mounted in the app). The task, note (pane and *Notes* list), objective, team, person, project, decision (pane and list row), waiting-on and the board card menu all ask `Archive "X"?` with what will happen. Cancel is focused and Esc cancels. Project and person list their tasks inside the dialog; the project offers *Archive tasks too* / *Move tasks to inbox*. The old inline "are you sure?" steps are gone.

**Out**
- A setting to turn the archive question off; marking the record `proposed` again when a task is reopened; a keyboard shortcut for archiving.

## Acceptance criteria
- [x] A Decision task can't be finished until Decision and Rationale are written (blank doesn't count); nothing changes when it is refused; cancelling and other types are not held up (`store/tests/task_decisions.rs`).
- [x] Finishing decides a proposed record with today's date and marks both automatic; a user date and a user-decided record are untouched (`task_decisions.rs`).
- [x] Setting the status or date by hand clears its flag; deciding without a date stamps and marks it (`task_decisions.rs`).
- [x] The finish dialog's command writes the texts, finishes the task and decides the record in one step, also for a task with no record; one undo step round-trips (`task_decisions.rs`, `store/tests/undo.rs`).
- [x] The error has its own code (`commands/tasks.rs` test); the pane's AUTO/YOU lines and the dialog's rules are tested in the UI crate.
- [x] Every archive in the UI goes through the shared dialog (`confirm_archive` tests; no `api::archive_*` call outside a confirmed action).
- [ ] Click-through (see below).

## Not yet verified by hand
- Give a task the type Decision, move it to Done on the board with nothing written: the dialog opens and the card goes back; write both boxes, *Decide and mark done*: the card is in Done and the record shows AUTO next to the status and the date
- Type a date on a record first, then finish the task: the date tag says YOU, the status AUTO
- Ctrl/Cmd+Z after the dialog undoes the task and the record together
- Archive from every pane, the Decisions row button and the card menu: a dialog asks each time, Esc and Cancel archive nothing; a project lists its tasks with the two choices
