# 32 — Task types

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 12, 23, 26

## Goal
Say what kind of work a task is (design, decision, bug ...), see it at a glance, filter by it, and see how a decision task did against the date it was planned for.

## Scope
**In** (built)
- **One optional type per task**: `tasks.task_type`, the `id` of an entry in the user's own list (migration 0013). No type is allowed and is what existing tasks have.
- **The list is the user's** (`Settings.task_types`, `UpdateSettings.task_types`; ADR-0016). It starts as Design, Build, Decision, Review, Research, Bug, Admin and can be extended, renamed, recoloured (eight hues, the objective palette) and archived in Settings → General → *Task types* (`components/task_type_settings.rs`). A type is never deleted: tasks that have an archived one keep it (dimmed), it is not offered for new tasks and cannot be set again (the store refuses it). A rename keeps the id, so tasks follow.
- **Where it shows**: a coloured label (`TypeChip`) on the board card, after the title in the list row and among the pills at the top of the pane; a *Type* dropdown next to Status and Priority in the pane; a *Type* filter (board and list; new tasks added while it is set get that type); a *Type* box in the link dialog's *New task* tab; `type:` in quick-add (by name or id; an unknown or archived type lists the available ones); a repeating task's next copy keeps its type. The Markdown export marks tasks with their type name, and the JSON export has `task_types.json`.
- **Planned against actual** (no new field): the due date is the date planned, `completed_at` the date done. A finished task says *Done 2027-03-05, 2 days late / on time / 1 day early* (pane, and a badge on the board card). `plan_history` reads the activity log for the first due date and the number of times it moved: *Originally planned for 2027-02-26, moved twice*, and for a finished task how it did against that first date. Calendar days. Types do not change this: it works for every finished task with a due date; the wording is aimed at decisions.
- Types do not change scheduling, capacity, health or any other rule.

**Out**
- Several types per task, per-project lists, reordering, types that carry templates or actions (a possible second step), a "Record the decision" action linking to the Decisions screen, grouping the board by type, a type column in the list, types in the weekly review or report.

## Acceptance criteria
- [x] A task can have a type, change it, clear it; an unknown or archived type is refused and nothing is written (`store/tests/task_types.rs`).
- [x] The list can grow, be renamed and recoloured and keeps every id; removing a type is refused; a bad list changes nothing else in the same update (`task_types.rs`, `core::task_types` tests).
- [x] A stored list that no longer reads falls back to the defaults (`task_types.rs`).
- [x] Undo restores a type (`store/tests/undo.rs`), a repeating task hands its type on, a linked task can be created with one.
- [x] `type:` in quick-add resolves names and ids, reports unknown and archived types, and is refused for other kinds (`core::quick_add` tests).
- [x] Due-date history and finish timing are read correctly (`types::task_type` tests, `ui::task_type` tests).
- [x] The export carries the list; the demo data has typed tasks and a decision that moved and finished late (`store/tests/demo.rs`).
- [ ] Click-through (see below).

## Not yet verified by hand
- Settings → General → Task types: add "Legal review", rename Build, recolour a type, archive one; the Type dropdown on a task offers the active ones
- Open a task: Type dropdown next to Status/Priority; the label shows on its board card and list row; the Type filter narrows both
- Demo data: "Choose the EU cloud region" (Decision) shows *Done …, on time* and *Originally planned for … moved once. Done 7 days late against that date*
- `task Pick the store type:decision due:fri` in the palette previews "Type: Decision"
