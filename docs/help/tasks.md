# Tasks and the inbox

A **task** is one piece of work. The **Tasks** screen shows all of them as a **board** of status columns, or as a list (switch with *List / Board* at the top right). The **Inbox** is the list showing only tasks that belong to no project, a good place for things you have captured and not yet filed.

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

## The board

The board has a column for each status: **To do**, **In progress**, **Blocked** and **Done**, plus **Cancelled** when you tick *Show cancelled*. Each card shows the title, its project, the priority (P1 and P2 are highlighted), the due date (red when overdue, amber when due today), the estimate and the assignee's initials. A circular arrow means the task repeats. A task wears the colour of the **objective its project contributes to**: a coloured edge and a chip with the objective's name on the board, an edge and a dot in the list. Tasks without a project, or whose project serves no objective, stay plain. See [Objectives](help:objectives).

- **Drag a card onto another column** to change its status. The column lights up where it will land, the card moves at once, and the change is saved behind it (you can undo it with `Ctrl/Cmd+Z`).
- **Click a card** to open it in the detail pane.
- **Add a task straight into a column** with the `+` in its heading (or `n` for the To do column). Type a title and press `Enter`; the box stays open for the next one. If you filtered by a project, new tasks join that project.
- **Order inside a column** follows the list: due date first (undated last), then priority. Dragging changes the status, not the position. Done shows the most recently finished first, and the 15 newest; press *Show older* for the rest.
- Dropping a repeating task on **Done** creates its next one, exactly as finishing it anywhere else does.

Keyboard: `j` / `k` move through the cards column by column, and `x`, `s` and `1`–`5` act on the highlighted card (see below).

## Finding tasks

The filters above the board or list combine: text (matches title, description, project and assignee), project, assignee and a due-date range. The list adds a status filter and *Show done*: finished and cancelled tasks are hidden there unless you ask for them or pick that status. Switching between board and list keeps your filters.

## Working in the list

Every row has its own controls (status, priority, project, assignee, due date), so most changes need no trip to the detail pane. Keyboard shortcuts for the highlighted row (on the board, only `j`, `k`, `Enter`, `x`, `s` and `1`–`5` apply):

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
