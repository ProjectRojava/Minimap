# 31 — Card menu and linking tasks

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 07

## Goal
Archive a task, and make or join a linked task, straight from a Kanban card, and make linking tasks obvious in a task's own pane.

## Scope
**In** (built)
- **A ⋯ menu on every Kanban card** (`components/card_menu.rs`; it appears when you point at the card, and stays while open): *Open*, *Link a new task…*, *Link an existing task…*, *Archive*. Archive is the soft archive (links go with it, `Ctrl/Cmd+Z` brings it back, a message says so); a permanent delete is still only possible from an archived item.
- **The link dialog** (`components/link_dialog.rs`, one per app, opened from the menu or the panel): tabs *New task* / *Existing task*; the relation: **Comes first** (the other task blocks this one), **Comes after** (this one blocks the other) or **Related**; a title for a new task (Enter creates it) or a picker for an existing one. A new task is made and linked in one step (`create_linked_task` command, `store::tasks::create_linked`: same project and priority, assigned to the user, one undo step) and opens in the detail pane; an existing one is linked with `add_edge`, so loops are refused with the path as usual.
- **One Links section for a task**, right under its fields (`components/task_links.rs`): waits for / blocks / related tasks with their status pills and ✕ to remove, then the other links (the generic list, `LinksEditor` in compact mode), and three buttons side by side: *New linked task…*, *Link an existing task…* and *Link to something else…*. The task has no second Links list further down; other kinds of item keep theirs.
- Subtasks are removed (ADR-0015); this is the replacement for "make a step of this task".

**Out**
- A menu on list rows, permanent delete from the menu, other relations (assigned, contributes to) in the dialog, linking several tasks at once, keyboard shortcut for the menu.

## Acceptance criteria
- [x] A new task can be made as blocking, following or related to a task, in one undo step (store `tests/linked_tasks.rs`, `undo.rs`; command test `a_linked_task_needs_a_title_and_a_live_source`).
- [x] The link between the two is the right way round for each choice (ui `link_dialog` test `the_task_that_comes_first_is_the_one_that_blocks`).
- [ ] Click-through (see below).

## Not yet verified by hand
- Point at a card on the Tasks board: a ⋯ appears at its top right; clicking it opens the menu without opening the pane
- *Link a new task…* → choose *Comes first*, type a title, Enter: the new task opens in the pane and the original's *Links* shows it under *Waits for*; Ctrl+Z removes the new task and the link
- *Link an existing task…* → pick one → *Link*; a choice that would make a loop is refused with the path
- *Archive* hides the card and says how to undo; Ctrl+Z brings it back
