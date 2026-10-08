# 31 — Card menu and linking tasks

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 07

## Goal
Archive a task, and make or join a linked task, straight from a Kanban card, and make linking tasks obvious in a task's own pane.

## Scope
**In** (built)
- **A ⋯ menu on every Kanban card** (`components/card_menu.rs`; it appears when you point at the card, and stays while open): *Open*, *Link a new task…*, *Link an existing task…*, *Archive*. Archive is the soft archive (links go with it, `Ctrl/Cmd+Z` brings it back, a message says so); a permanent delete is still only possible from an archived item.
- **The link dialog** (`components/link_dialog.rs`, one per app, opened from the menu or the panel): tabs *New task* / *Existing task*; the relation, as what the *other* task is: **Parent** (this one blocks it, so it waits until this is done), **Child** (it blocks this one) or **Related**, with a sentence under the choices naming the task that waits (the words were changed from *Comes first / Comes after* because they did not say which task they described; the link underneath is unchanged); a title for a new task (Enter creates it) or a picker for an existing one. A new task is made and linked in one step (`create_linked_task` command, `store::tasks::create_linked`: same project and priority, assigned to the user, one undo step) and opens in the detail pane; existing ones are picked from a searchable list with a tick box each (tasks already linked are left out), one or several, and linked with `add_edges` (one transaction and one undo step, every link checked against the rules and the ones before it, so a loop refuses the whole batch with the path as usual).
- **One Links section for a task**, right under its fields (`components/task_links.rs`): parents / children / related tasks with their status pills and ✕ to remove, then the other links (the generic list, `LinksEditor` in compact mode), and three buttons side by side: *New linked task…*, *Link an existing task…* and *Link to something else…*. The task has no second Links list further down; other kinds of item keep theirs.
- Subtasks are removed (ADR-0015); this is the replacement for "make a step of this task".

**Out**
- A menu on list rows, permanent delete from the menu, other relations (assigned, contributes to) in the dialog, linking several tasks at once, keyboard shortcut for the menu.

## Acceptance criteria
- [x] A new task can be made as blocking, following or related to a task, in one undo step (store `tests/linked_tasks.rs`, `undo.rs`; command test `a_linked_task_needs_a_title_and_a_live_source`).
- [x] The link between the two is the right way round for each choice (ui `link_dialog` test `the_task_that_comes_first_is_the_one_that_blocks`).
- [ ] Click-through (see below).

## Not yet verified by hand
- Point at a card on the Tasks board: a ⋯ appears at its top right; clicking it opens the menu without opening the pane
- *Link a new task…* → choose *Child*, type a title, Enter: the new task opens in the pane and the original's *Links* shows it under *Children*; Ctrl+Z removes the new task and the link
- *Link an existing task…* → pick one → *Link*; a choice that would make a loop is refused with the path
- *Archive* hides the card and says how to undo; Ctrl+Z brings it back
