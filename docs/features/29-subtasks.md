# 29 — Subtasks

> **Withdrawn (ADR-0015):** subtasks were removed after use. Kept as history; nothing below is in the app any more.

Status: Withdrawn · Milestone: — · Priority: Should
Depends on: 06, 07

## Goal
Group tasks under a bigger one, add a step to a task without leaving it, and see how far the whole thing is.

## Scope
**In** (built)
- **A `subtask_of` link** (ADR-0013): child -> parent, Task -> Task, no attributes, no loops (the error names the path), one parent per task.
- **From the task panel** (*Subtasks* section, `components/subtasks.rs`): a box that makes a new subtask on Enter (in the parent's project, with its priority, assigned to you); a picker to link a task that already exists; on a subtask, *Subtask of X* with ✕ to make it a regular task, or a picker to make a task a subtask of another. Each subtask has a tick to mark it done, its status pill, its due date (red when overdue), and ✕ to take it out. A bar and "2 of 5 done" show progress (cancelled ones don't count).
- **On the board and list**: a subtask card shows "↳ parent title"; a parent shows a "2/5" pill (green when all are done).
- Commands `set_parent(task_id, parent_id?)` and `create_subtask(parent_id, title)`; `TaskRow.parent` / `TaskRow.subtasks`, `TaskDetail.parent` / `TaskDetail.subtasks` read models.
- Undo covers creating, linking, moving and freeing (one step each); the full export carries the links (`edges.json`) and the Markdown project files say "subtask of X"; the demo data has three subtasks under the EU go-live checklist; sync repairs a loop made by two devices like any other.

- **Order and next step**: subtasks are listed in the order the work goes (`blocks` links between them, oldest first where there is none); *Do after…* adds that link, *after X ✕* removes it; the first open, not-waiting subtask is marked *next* and held-up ones *waiting* (`core::subtasks::{sequence, next_step}`, `Subtask.{after, waiting, next}`).
- **Status suggestions**: `TaskDetail.status_hint` (`core::subtasks::status_hint`): all done -> done, all open ones blocked -> blocked, work started on a to-do group -> in progress; shown with a button, never applied by itself.
- **Dependency graph**: a group is a dashed frame around its subtasks (`GraphGroup`, `layout_clustered`), not a box; links on a group are drawn to its subtasks. Subtasks are always shown, even with no `blocks` links (being a subtask counts as a link), and the rest of a group comes along as context when one member is in view.
- **Groups and `blocks` (ADR-0013 addendum)**: a task with subtasks is a group, not scheduled itself (its estimate is ignored; dates, slack and criticality come from the tasks under it); a `blocks` link on a group applies to every task under it; capacity, project health and impact analysis count the tasks under it, not the group; a `blocks` link between a group and its own part is refused (and so is a parent change that would leave one), as is a loop that only exists through a group; the timeline draws a group as a thin bracket.

**Out**
- Finishing a parent automatically when its subtasks are done (it is only suggested), the reverse (finishing a parent finishing its subtasks), an indented tree on the board or list, dragging a card onto another to nest it, quick-add (`sub:`), subtasks copied by a repeating parent.

## Acceptance criteria
- [x] A subtask can be created from its parent, or an existing task linked, and freed (store `tests/subtasks.rs`).
- [x] A loop is refused with the path and changes nothing; a task can't be its own parent (command test `a_subtask_loop_is_refused_*`).
- [x] Lists show the parent and the done/total (cancelled excluded) (store `lists_show_the_parent_*`, ui `subtask_chip`, `progress`).
- [x] Everything is undone and redone (store `undo.rs` `creating_and_linking_subtasks_*`).
- [x] Schedule: a group spans its leaves and its own estimate is ignored; a block on a group holds every leaf; a group blocks what waits for it until its last leaf; capacity, health and impact count leaves (core `schedule` / `impact` / `capacity` / `overview` / `subtasks` tests).
- [x] A block between relatives, or a loop through a group, is refused with a message, and changes nothing (command tests in `commands/tasks.rs`).
- [x] Order, next step and waiting come out right (store `subtasks_come_in_order_*`, `a_block_on_the_group_*`; core `sequence`, `next_step`); status suggestions (`a_group_gets_a_status_suggestion_*`); the graph frames a group without covering other boxes (core `dependency_graph`, `layout`).
- [ ] Click-through (see below).

## Decisions
- See ADR-0013 (edge, not column; a parent doesn't affect the schedule; one parent).
- **Unlinking makes a regular task, never deletes.** ✕ on a subtask only removes the link.

## Implementation notes
- Types: `EdgeType::SubtaskOf`, `Subtask`, `SubtaskProgress`; core `edge_rules` (matrix, acyclic); store `tasks::{set_parent, create_subtask}`, `views::{task_rows, task_detail}`; `merge::ACYCLIC`; commands in `commands/tasks.rs` (capability + manifest entries); UI `subtasks.rs`, `labels::subtask_chip`, `task_board.rs`, `task_list.rs`; the generic Links section hides the relation (`detail_pane::kind_edited_elsewhere`).

## Not yet verified by hand
- A group with three subtasks: use *Do after…* to make Build → Test → Ship: they are numbered, the first open one says *next*, the others *waiting*; tick Build and *next* moves to Test
- Tick every subtask: a line suggests *Set done* for the group; click it
- The Dependencies screen shows the group as a dashed frame around its subtasks, titled with the group, and an arrow from the group's blocker to each subtask
- Give a task a 5-day estimate and two 1-day subtasks: the Schedule shows the group as a thin bracket over the two, and the project's finish does not include the 5 days
- *Approval blocks Launch* (Launch has subtasks): every subtask starts after Approval; linking one of Launch's subtasks to block Approval is refused with a loop message; linking Launch to block its own subtask is refused
- Open a task, type a title in *Add a subtask…* and press Enter: it appears with a bar "0 of 1 done"; tick it: "1 of 1 done" and the parent's board card shows a green 1/1
- *Link an existing task…* adds another task; open it: it says "Subtask of …"; ✕ frees it
- Making A a subtask of B and then B of A is refused with a toast naming the loop
- Ctrl+Z after each step takes it back
- The Tasks board shows "↳ parent" on subtask cards
