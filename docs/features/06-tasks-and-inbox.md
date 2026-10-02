# 06 — Tasks and inbox

Status: Draft · Milestone: M1 · Priority: Must
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
- Estimate input accepts `3d`, `4h` (8h = 1d, configurable later).

## Acceptance criteria
- [ ] Create, edit and complete a task using only the keyboard.
- [ ] Pasting 5 lines creates 5 tasks after preview.
- [ ] Inbox shows only unprojected tasks; triaging removes them from it.

## Open questions
- Should subtasks exist, or is `blocks` enough?
- Hours per day for `h` estimates: fixed 8 or a setting?
