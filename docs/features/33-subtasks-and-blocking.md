# 33 — Sub-tasks, apart from blocking

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 07, 31

## Goal
Let a task be a *piece of* another task without that meaning the other waits for it, and stop calling the order of the work "parent" and "child".

## Scope
**In** (built)
- **`subtask_of` link** (task → task, sub-task → parent; ADR-0017). One parent per task, one level deep; refusals name the tasks (`core::subtasks`, `store::tasks::check_subtask`). No effect on the schedule, critical path, what-if, capacity, health or the dependency graph.
- **Wording**: *Blocks* / *Blocked by* for the order of the work in the pane and dialog. Parent/child words belong to *part of* only.
- **Task pane**: a *Part of* section (parent, sub-tasks with "N of M done", buttons *New sub-task…*, *Add existing…*, *Make it part of…*, offered only where the rule allows) above *Links* (Blocked by, Blocks, Related tasks, other links).
- **Link dialog**: five choices in two rows: *Blocks this*, *Blocked by this*, *Related* / *Parent*, *Sub-task*; a sentence under them names who waits, or says nothing waits. A parent is chosen alone. `LinkDialog::open_as` presets the relation.
- **Kanban**: a violet chip on a parent (*2/5 sub-tasks*) and on a sub-task (the parent's name); selecting a card lights its family and draws dotted lines (no arrow) to it; the ⋯ menu has *Add a sub-task…*; the blue chain chip counts only blocks and related links.
- `TaskRow.parent / subtask_count / subtasks_done` (cancelled and archived sub-tasks not counted).
- Demo data: five sub-tasks (the status page and runbook under "Go-live checklist", one done under "Cost dashboard for teams").
- `create_linked_task` takes `Parent` and `Subtask`; existing links are untouched (`blocks` stays `blocks`).

**Out**
- Deeper nesting; a parent's dates, status or estimate derived from its sub-tasks; blocking a parent until its sub-tasks are done; a quick-add `parent:` word; sub-tasks indented in the list, the weekly report or the Markdown export; a merge repair for a task that ends up with two parents after a sync of old data.

## Acceptance criteria
- [x] The rules: one parent, one level, no self, sub-tasks of one parent in a batch, a batch that only breaks the rule together refused as a whole (`core::subtasks` tests, `store/tests/subtasks.rs`, `commands/edges.rs` test).
- [x] Part-of and blocks are independent: both can join the same pair; the board's link count ignores part-of; the demo snapshots (schedule, overview, slip) did not change when sub-tasks were added.
- [x] Rows carry the parent and the progress; archiving the parent frees its sub-tasks; cancelled and archived sub-tasks are not counted (`store/tests/subtasks.rs`).
- [x] A new sub-task or parent is made and linked in one step; a refusal creates nothing (`store/tests/subtasks.rs`).
- [x] Dialog edges are the right way round and the words are told apart (`ui::link_dialog` tests); the pane groups links correctly (`ui::task_links` tests); the board draws part-of lines (`ui::task_board` tests).
- [ ] Click-through (see below).

## Not yet verified by hand
- Open a task → *Part of* → *New sub-task…*, give it a title: it opens, shows *Part of: <parent>*; the parent's pane shows *Sub-tasks · 0 of 1 done*; the board shows the violet chips and a dotted line when either card is open
- Make the sub-task Done: the parent's chip reads 1/1
- Try *New sub-task…* on that sub-task, or *Make it part of…* on the parent: refused with a sentence naming the tasks
- Link A *Blocks this* on B while B is part of A: allowed; the schedule is unchanged
- Ctrl+Z after making a sub-task takes the task and the link back in one step
