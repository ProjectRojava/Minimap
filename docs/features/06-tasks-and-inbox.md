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
- **Inbox**: tasks with no project, plus items quick-add couldn't fully resolve. Triage actions: move to project, assign, set due, archive.
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
- **No subtasks** (superseded by spec 29 / ADR-0013, which added a `subtask_of` link). Break work into ordinary tasks and sequence them with `blocks`: every piece keeps its own owner, estimate and status, so the schedule, critical path and impact analysis (13, 14) stay correct. A Markdown checklist in descriptions may come later (with "convert to task", like notes in 09); if grouping becomes a real need, a lightweight "section" inside a project is the way, not parent/child arithmetic.
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
