# 06 — Tasks and inbox

Status: Implemented — awaiting manual check · Milestone: M1 · Priority: Must
Depends on: 05

## Goal
The atoms of work. Fast to create and edit; loose tasks land in an inbox for triage.

## Scope
**In**
- Fields: title, description, project_id (nullable), status (`todo`, `in_progress`, `blocked`, `done`, `cancelled`), estimate_days, start_date, due_date, completed_at, priority.
- Task list with filters: status, project, assignee, due range, text. Sort by due date, priority.
- **Inline editing** in list rows: status, due date, priority, assignee via keyboard (no modal).
- **A task needs a project.** One can't be created without it (store `tasks::NEEDS_PROJECT`, checked in `create_in_tx`, so every way of making a task is covered: the forms, quick add, pasted lists, checklist lines in notes). The project can be changed but not taken away (`update` refuses `Patch::Clear`; undo and redo are not held up by it). Exceptions: **meetings** (they have a start time), and a task made *from* another (the next of a repeating task, a linked one), which takes that one's project, so work from before the rule can still carry on. Archiving a project with "Move tasks to inbox" and removing demo data still detach tasks, and a merge repairs a task whose project was deleted elsewhere. The forms ask for a project (the filtered one, else a required picker); quick add says "A task needs a project: add #handle"; a checklist line is filed in the project it mentions, else the first one the note mentions, else refused.
- **A task needs a type** (follow-up to spec 32). One can't be created without it: store `tasks::NEEDS_TYPE`, checked in `create_in_tx` and `create_linked`, so the forms (board card, list box, pasted lists, the link dialog, checklist lines in notes) and quick-add are covered; the quick-add command also holds a `task` line without `type:` back in the preview. The type can still be changed or cleared afterwards. Exceptions: **meetings** (made with the built-in Meeting type) and a task made *from* another by the store itself (the next of a repeating task keeps its type, or none for one from before). The UI boxes start on the filtered type, else the type last used (localStorage `minimap.new-task-type`).
- **Start date = when work started.** Moving a task from To do to In progress (board, row, pane or `s`) sets its start date to that day, replacing any planned one (the planned dates stay in the activity). A start date given in the same change is kept. **A task in progress always has a start date**: a move *into* In progress (from any status) that would leave the task without one is refused by `store::tasks::update` with `StartDateNeeded` (command code `start_date_needed`), and the UI answers by opening the *Start date* dialog (`components/start_date.rs`, opened by `state::finish_task` like *Finish decision*), which asks for the day (today to start with) and sends the status and the date together; Cancel leaves the task where it was. Only the move from To do replaces a date the task already has (with today); Blocked → In progress and reopening a done task keep theirs. Clearing the start date of a task in progress is refused the same way (the date can be changed, and cleared once it is out of In progress). A task *created* in progress with no date starts today. A task that was in progress with no date before this rule keeps working: it can be edited otherwise until someone sets a date. Meetings are exempt, and undo/redo put things back as they were. "Today" is the app's (UTC) day, like the rest of the store.
- **The task pane has five sections, not eight.** *Fields* (without the status, priority and due date, which are controls in the pinned header; a meeting keeps its priority), *Related tasks* (part of, sub-tasks, blocks, blocked by, related and other links: `task_links.rs`), *Files & links* (reference links and attachments: `files_links.rs`), *Notes and findings* and *Activity*. The last three before Activity stay hidden while empty (`pane_reveal.rs`), and an **Add** row under the fields starts the first of each: Sub-task, Part of…, Linked task, Reference link, File, Note, and **What if…** (which used to be a section of its own). Asking for a hidden section shows it; a meeting's *Follow-ups* section and a Decision's record are unchanged. Other item types keep their own sections.
- **Finished tasks fade, then archive themselves.** On the board (`task_board.rs`, view only), a done card is full for 3 days after `completed_at`, small and dimmed to 7 days, folded into an *Earlier* group to 14 days, then behind *Show older*. `store::done_sweep::archive_old_done` (run at start and hourly by a timer thread in `main.rs`) archives done tasks `done_archive_days` (30) after they were finished, meetings after `done_archive_meeting_days` (2), 0 = never; both synced Settings (Thresholds → Finished tasks). It keeps a sub-task of an open parent, a parent with open sub-tasks, a blocker of open work and a task with a live ping. It is the ordinary archive, restored from the **Archive** screen (`/archive`, `g v`; `store::archive::list_all`, `restore_archived`), which lists every archived item of every type. The weekly review still counts tasks finished that week and archived since (`archive::done_rows_between`). The sweep is a background write like the repeating notes, so it is not an undo step.
- **Inbox**: tasks with no project (those from before the rule, or detached as above), plus items quick-add couldn't fully resolve. Triage actions: move to project, assign, set due, archive.
- **Paste multiple lines** into "new task" → one task per line, preview first.
- `completed_at` set automatically when status becomes `done`, cleared if reopened.
- Commands: `create_task`, `update_task`, `archive_task`, `get_task`, `list_tasks`, `create_tasks_bulk`.

**Out**
- Scheduling (13), recurring tasks (27).

## Rules
- Default assignee: self, unless set.
- Estimate input accepts `3d`, `4h` (hours convert at the **hours-per-day setting**, default 8).

## Acceptance criteria
- [x] Create, edit and complete a task using only the keyboard. *(shortcuts below; unverified by hand)*
- [x] Pasting 5 lines creates 5 tasks after preview. *(core `parse_lines`, command + store tests for the all-or-nothing create; the preview UI is unverified by hand)*
- [x] Inbox shows only unprojected tasks; triaging removes them from it. *(core filter test; the inbox is the tasks list with `no_project`)*

## Decisions
- **No subtasks** (spec 29 added them and ADR-0015 removed them again; this decision stands). Break work into ordinary tasks and sequence them with `blocks`: every piece keeps its own owner, estimate and status, so the schedule, critical path and impact analysis (13, 14) stay correct. A Markdown checklist in descriptions may come later (with "convert to task", like notes in 09); if grouping becomes a real need, a lightweight "section" inside a project is the way, not parent/child arithmetic.
- **Hours per day is a setting** (default 8; 0 < h <= 24). It only affects estimates entered afterwards; existing estimates are never rewritten. A new key/value `settings` table, `get_settings` / `update_settings`, and a minimal Settings screen (just this field) landed with this spec; spec 23 builds the rest.
- Estimates are always stored and shown in days. `3d`, `1.5d`, `4h` and a bare number (= days) are accepted; empty clears.
- **Default assignee = me** (the self person; nobody if first-run setup hasn't happened). The assignee is the `assigned_to` edge; one assignee per task.
- The inbox is the open tasks that belong to no project. "Items quick-add couldn't fully resolve" join it with spec 12.
- Done and cancelled tasks are hidden unless "Show done" is on or a status filter is chosen.
- List order: due date (undated last), then priority (1 = highest), then title.

## Keyboard (task lists)
`n` new task · `j`/`k` move · `Enter` open details · `x` toggle done · `s` next status (to do → in progress → done → to do) · `1`–`5` priority · `d` focus the due date · `a` focus the assignee · `Esc` close the pane or leave a field. `n` also opens the "new" form on People, Teams, Objectives and Projects. In the new-task box: Enter adds, Shift+Enter inserts a line; pasting several lines shows a preview first.

## Implementation notes
- **Core** `minimap-core::tasks`: `filter`, `sort`, `parse_estimate`, `parse_lines` (proptests: parsing is total, hours round-trip through the setting, no blank titles).
- **Store**: migration `0005_settings.sql`; `settings::{get, update}`; `tasks::{create, create_many, set_assignee}` (assignment and its activity happen in the create's transaction); `views::{task_rows, task_detail}`; `edges::add_in_tx`. `CreateTask` gained `assignee` (`Me` / `Nobody` / `Person`).
- **Commands**: `list_tasks`, `get_task`, `get_task_detail`, `create_task`, `update_task`, `set_task_estimate`, `set_assignee`, `archive_task`, `parse_task_lines`, `create_tasks_bulk`, `get_settings`, `update_settings`. Putting a task in an archived project is refused.
- **UI**: Tasks and Inbox screens (one `TaskList` component: filters for text, status, project, assignee and due range; inline status / priority / project / assignee / due controls in every row, no modal), task panel (all fields, estimate text, archive), Settings screen. `ListNav` gained `on_new` and `on_row_key` so screens opt into the `n` and row shortcuts.
- The project panel's task list stays read-only; tasks are created on the Tasks screen (or filtered to a project, where new tasks join that project).
- **Not done**: editing `blocks` (spec 07's link editor); a one-key archive in the list (archive is in the task panel, with a confirmation, until undo exists, spec 25); sorting by clicking column headers.

## Board (added after the first version)
The Tasks screen opens as a board: columns To do, In progress, Blocked, Done (+ Cancelled on request), cards dragged between columns change the status through `update_task` (optimistic, saved behind the drop). List remains one click away (List/Board toggle, shared filters) and is what the Inbox uses. Order in a column is the list order (no manual ranking: there is no position field); Done is newest first, 15 shown. Code: `ui/src/components/task_board.rs`. No backend change.

## Not yet verified by hand
- `n`, type a title, Enter: the task appears; keep typing for more. Paste a 5-line list: preview, then 5 tasks
- row controls (status, priority, project, assignee, due) save without opening the pane; clicking elsewhere on the row opens it
- on a row: `x`, `s`, `1`–`5`, `d` (type a date, Enter), `a`; `j`/`k` keep the cursor after a change
- Tasks board: drag a card to another column and it moves at once and stays after a refresh; `+` in a column adds a task there; drop on Done on a repeating task creates the next one; List/Board keeps the filters
- filters combine; "Show done" reveals completed tasks; the inbox empties as tasks get a project
- estimate `4h` becomes `0.5d` at 8 h/day; change the setting (Settings) and `4h` converts differently; invalid text shows a toast
- Settings: hours per day saves on blur; bad input shows a toast
