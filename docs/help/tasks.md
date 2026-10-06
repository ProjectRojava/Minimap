# Tasks and the inbox

A **task** is one piece of work. The **Tasks** screen lists all of them; the **Inbox** is the same list showing only tasks that belong to no project, a good place for things you have captured and not yet filed.

## Adding tasks

- **Type a title** in the box at the top and press `Enter`. Press `n` (outside a text box) to jump to it.
- **Paste a list.** Paste several lines at once and Minimap shows a preview of one task per line (list markers like `-`, `*`, `1.` and `[ ]` are stripped); confirm to add them all or none.
- **Quick-add** with `Ctrl/Cmd+K` takes a whole line: `task Fix login timeout @priya #api-launch !2 due:fri est:3d`. See [Quick-add and the palette](help:quick-add).

New tasks are assigned to you unless you say otherwise.

## Fields

| Field | Notes |
|---|---|
| Title, description | Plain text |
| Status | to do, in progress, blocked, done, cancelled |
| Priority | 1 (highest) to 5; default 3 |
| Project | Optional. No project means the inbox |
| Assignee | One person, or nobody |
| Estimate | `3d`, `1.5d`, `4h` or a bare number (days). Hours use your hours-per-working-day setting. Leave it empty if you don't know |
| Start date, due date | Optional. A start date means "not before" for the schedule |
| Repeats | Optional rule such as `mon` or `2w`: see [Recurring items](help:recurring) |

A task with **no estimate** is scheduled as one day and flagged *unestimated*, so projects can tell you how much of their plan is a guess.

## Finding tasks

The filters above the list combine: text (matches title, description, project and assignee), status, project, assignee and a due-date range, plus *Show done*. Finished and cancelled tasks are hidden unless you ask for them or pick that status.

## Working in the list

Every row has its own controls (status, priority, project, assignee, due date), so most changes need no trip to the detail pane. Keyboard shortcuts for the highlighted row:

| Key | Does |
|---|---|
| `j` / `k` | Move down / up |
| `Enter` | Open the detail pane |
| `x` | Mark done (or reopen) |
| `s` | Move to the next status |
| `1`–`5` | Set the priority |
| `d` | Focus the due date |
| `a` | Focus the assignee |

## Linking tasks

Open a task to link it: what it **blocks**, who it is **assigned** to (with an allocation percentage), which objective it **contributes to**. See [Links between items](help:links). There are no subtasks: break work into tasks and order them with *blocks* links.

## Finishing, archiving and undoing

*Done* records when it was completed. *Archive* (in the detail pane) hides the task with its links. Both can be undone with `Ctrl/Cmd+Z`; see [Undo and redo](help:undo).
