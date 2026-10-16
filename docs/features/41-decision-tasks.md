# 41 — Decision tasks and the decision log

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 10, 32, 25, 24

## Goal
A task of type *Decision* is the work of deciding; the *Decisions* log is the lasting record. Making a task a Decision creates its record in the log, visible and written from the ticket. The task can be finished and disappear from the board and This week; the decision stays.

## Scope
**In**
- **A record per decision task.** Creating a task of type Decision (any path: pane, board, quick-add, a linked task, a repeating task's next copy) or changing a task's type to Decision creates a Decision in the same transaction (one undo step): `proposed`, titled like the task, the task's description as its context. It is linked by an `affects` link to the task (marked `attrs.origin = "task"`, hidden from the link editor, accepted by `core::edge_rules::validate_attrs` on `affects` only) and to the task's project (plain `affects`).
- **Never two.** `store::decisions::for_task` finds the active decision with the marked link; if there is one, nothing is made. Choosing Decision again on a task that has its record keeps the one record.
- **In the ticket.** The task pane has a *Decision record* section (`components/task_decision.rs`): status and decided-on date, and the context, decision and rationale boxes (the decision pane's own parts, `StatusPart` and `WritePart`). A task with the type but no record shows *Create decision record* (`ensure_task_decision`, for tasks typed before this existed or whose record was archived; refused for other tasks).
- **Decisions stay.** Finishing, cancelling or archiving the task leaves the record in the log with its text and status; archiving the task archives the link, so the record then stands alone. **Changing the type away removes the record for good** (archived, then deleted with its links, leaving a tombstone for the other devices); the task pane asks first, naming the record and whether anything was written in it, and goes back to the old type on Cancel. Making the task a Decision again starts a new, empty record. Undo and redo put the type back without touching records, so after undoing, the pane offers *Create decision record*. The same question is asked when a meeting is turned into another type (its time goes). A finished task whose record is still `proposed` shows a nudge in the section. (Spec 42 later made finishing require the written decision and decide the record.)
- **Undo/redo** follow the activity log: undoing the change archives the record and its links, redo brings the same record back (`tasks::restore_in_tx` does not make a second one).
- Demo data: "Choose the EU cloud region" (done) and the two open Decision tasks have records; the first is the decided "Frankfurt as the primary EU region".

**Out**
- Title or status kept in step with the task afterwards (they are independent), a record on the board card, a "from task" hint on the Decisions screen beyond its normal *affects* text, no migration or backfill (existing tasks get a record by *Create decision record*).

## Acceptance criteria
- [x] A new Decision task, or a task whose type becomes Decision, has one proposed record linked to the task and its project (`store/tests/task_decisions.rs`).
- [x] Other types make none; the type changing away and back never makes a second one.
- [x] The record survives the task being finished or archived, with its text and status.
- [x] The button makes a record when there is none (archived), returns the existing one otherwise, and is refused for other tasks.
- [x] A decision that merely affects the task is not its record.
- [x] Undo and redo of the change leave exactly one record (`store/tests/undo.rs`).
- [x] The demo data has the records; counts and Developer text are updated.
- [ ] Click-through (see below).

## Not yet verified by hand
- Add a task, set Type to Decision: a *Decision record* section appears in the pane; the Decisions screen has a new proposed row that affects the task and its project
- Write the decision, set Decided, mark the task done: it leaves the board, the record stays with its text
- Type back to something else and to Decision again: still one record
- Ctrl/Cmd+Z after setting the type removes the record; Ctrl/Cmd+Shift+Z brings the same one back
- `task Pick the store type:decision` in the palette creates the task and its record
