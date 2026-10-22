# 59 — Backburner: set a task aside without losing it

Status: Done — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 16, 17, 25, 37, 43

## Goal
Let the user put a task on the **backburner**: still there, with its status, dates and links, but out of the way of the screens that say what needs doing now. It is the opposite of focus: focus keeps a task in front of you; the backburner keeps it out until you want it back.

## Scope
**In** (built)
- A flag on the task, `backburner_since` (a date), not a status: the task keeps whatever status it had (a blocked task can be on the backburner too) and gets it back as it was.
- A **Backburner column** on the Tasks board, right of Done (before Cancelled). Drag a card in to set it aside; drag it onto a status column to bring it back with that status. `+` in its heading adds a task born there. Oldest first by default, so what has been set aside longest is the one you notice.
- `b` on the highlighted card or row (board and list) sets aside or brings back. A *Backburner* button in the task's pane. The message says how many open tasks still wait on it and has *Undo*.
- The Tasks **list** leaves these tasks out until *Show backburner* is ticked (remembered with the other filters); rows carry a *Backburner* chip.
- **What it leaves**: This week (attention, priorities, focus, the focus count), capacity and load, the Overview's health counts, risks and top tasks, *Your people* frames, the weekly review's lists, and the fog (a task on the backburner is never in play).
- **What it keeps**: the schedule, the critical path, impact, the dependency graph, the Gantt and the export. The schedule still counts a task on the backburner, so tasks that wait on it keep their place behind it. Cancel the task, or take it off its project, if the plan should stop counting it.
- Finishing or cancelling a task takes it off the backburner. A meeting cannot be put on it (it has a day). A finished or cancelled task is refused.
- A repeating task's next one starts off the backburner.

**Out**
- A revisit date that brings the task back onto This week, and a Weekly review step for old ones (the plain list is enough for now).
- A *waiting on a task on the backburner* mark on the tasks that wait on it (the message after setting aside says how many).
- A quick-add token, a card menu entry, a palette action.
- Putting a project or objective on the backburner (a project has *Paused*).

## Rules
- `Task.backburner_since: Option<Date>`; `Task::is_parked()`. `CreateTask.backburner_since`, `UpdateTask.backburner_since: Patch<Date>` (in `store::undo::nullable_fields`). Migration 0019.
- `set_task_backburner(id, on)` sets today or clears, refuses a closed task, does nothing if the task is already in that state, and returns `BackburnerResult { task, held_up }` where `held_up` = `core::tasks::held_up_by` (open, not parked tasks it blocks). It is a normal logged update: undoable, in the history.
- `store::tasks` clears the flag when status becomes done or cancelled (not on undo/redo) and refuses it on a meeting.
- `TaskFilter.hide_parked` (default false: every other caller still sees them).

## Acceptance criteria
- [x] Stored, cleared, logged and undone exactly; born parked; finishing or cancelling clears it; a meeting refuses it; the repeat's next one is not parked (`store` tests `backburner`).
- [x] The command sets and clears, does not move the day when asked twice, counts what waits on the task, refuses a finished task (`commands::tasks` test); `held_up_by` ignores closed, parked and archived (`core::tasks`).
- [x] A task on the backburner is not in play (`core::fog`), adds no load (`core::capacity`) and is nowhere on This week (`core::this_week`); the list filter hides it only when asked (`core::tasks`).
- [x] Board: columns in order, a parked task is in Backburner whatever its status and keeps it, a card being moved already sits in its new column in both directions (`ui::task_board` tests); text in `ui::components::backburner` tests.
- [x] Help describes it (`tasks`, `keyboard`, `this-week`, `capacity`, `overview`, `fog`).
- [ ] Click-through (below).

## Not yet verified by hand
- Drag a card onto Backburner: it moves at once, a message says it was moved (and how many tasks still wait on it) with *Undo*; `Ctrl/Cmd+Z` puts it back in its column
- Drag it from Backburner onto In progress: it leaves the backburner with that status
- Press `b` on a card and on a list row; open its pane: the button reads *Bring it back*
- This week, Capacity, the Overview: a parked overdue task is not counted
- The list hides it; tick *Show backburner*: it shows with a chip
- Mark a parked task done: it leaves the backburner; reopening does not put it back
