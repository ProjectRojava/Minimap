# Tasks and the inbox

A **task** is one piece of work. The **Tasks** screen shows all of them as a **board** of status columns, or as a list (switch with *List / Board* at the top right). The **Inbox** is the list showing only tasks that belong to no project, a good place for things you have captured and not yet filed.

## Adding tasks

- **Type a title** in the box at the top and press `Enter`. Press `n` (outside a text box) to jump to it.
- **Paste a list.** Paste several lines at once and Minimap shows a preview of one task per line (list markers like `-`, `*`, `1.` and `[ ]` are stripped); confirm to add them all or none.
- **Quick-add** with `Ctrl/Cmd+K` takes a whole line: `task Fix login timeout @priya #api-launch !2 due:fri est:3d`. See [Quick-add and the palette](help:quick-add).

New tasks are assigned to you unless you say otherwise.

## Fields

At the top of a task's detail pane a row of coloured pills shows where it stands at a glance: its status, priority (P1 and P2 in amber), its due date (red "overdue" when it has passed and the task is still open, amber "due today"), and the objectives its project serves. The Status and Priority dropdowns are coloured the same way and change colour as you pick.

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
| Reference links | Web pages, Google Drive files and other addresses: see below |

A task with **no estimate** is scheduled as one day and flagged *unestimated*, so projects can tell you how much of their plan is a guess.

## The board

The board has a column for each status: **To do**, **In progress**, **Blocked** and **Done**, plus **Cancelled** when you tick *Show cancelled*. Each card shows the title, its project, the priority, the due date, the estimate and the assignee's initials. **Priority** is a pill: P1 solid amber, P2 tinted amber, the rest quiet. The **due date** is a pill that gets louder as the day nears: *In 5d* (amber tint) within a week, *Tomorrow* and *Today* (solid amber), *3d overdue* (solid red); further out, or when the task is done or cancelled, it is the plain date. Hover it for the exact date. A circular arrow means the task repeats. A task wears the colour of the **objective its project contributes to**: a coloured edge and a chip with the objective's name on the board, an edge and a dot in the list. Tasks without a project, or whose project serves no objective, stay plain. See [Objectives](help:objectives).

- **Deadlines warm the card.** An open task with a due date starts to take on a red tint 14 days before it is due. The tint grows a little every day, is clearly red on the due day and a touch stronger once it is overdue, so what is closest to its deadline stands out as you scan the board. Done and cancelled tasks, and tasks with no due date, stay plain; the card you have open or selected shows the normal highlight instead. The list rows warm the same way.
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

## Reference links

A task's detail pane has a **Reference links** section for the documents and pages behind the work: a Google Doc or Drive folder, a ticket, a spec, a web page, an email address.

- **Add one**: paste the address into the first box, optionally type a name in the second, and press `Enter` or *Add link*. An address without `https://` (like `drive.google.com/...`) gets it added. Without a name, the list shows a short form of the address.
- **Open one**: click it. It opens in your computer's browser (or mail program for an email address), so Minimap never sees what is behind it and needs no access to your Drive.
- **Remove one**: the ✕ at its right. `Ctrl/Cmd+Z` brings it back.
- Only web addresses (`http`, `https`) and email addresses (`mailto:`) are accepted. Anything else, such as a path on your computer or a `file:` address, is refused with a message; to keep a file itself, use **Attach file** in the Attachments section of the pane instead.
- A task can hold up to 50 links. Adding the same address twice keeps one.
- When a repeating task is finished, its next one keeps the links.

Links travel with the task: they are saved with it, synced and backed up like the rest, and included in the [full export](help:export) and in the Markdown project files.

## Subtasks

A task can be broken into **subtasks**. A subtask is a full task of its own (status, assignee, dates, notes), joined to its parent by a link. Open the parent and use the **Subtasks** section:

- **Add a subtask**: type a title and press `Enter`. It joins the parent's project and priority and is assigned to you.
- **Link an existing task**: pick it from *Link an existing task…*. Open any task and use *Make this a subtask of…* to do it the other way round. A task has one parent; picking another moves it.
- **Tick a subtask** to mark it done (tick again to reopen), click its title to open it, and use ✕ to take it out: it stays a normal task.
- **Put them in order.** Under an open subtask, *Do after…* picks another subtask that has to come first (a *blocks* link between the two); *after X ✕* shows it and removes it. Subtasks are listed in the order the work goes, numbered once any order is set, and the first open one that isn't waiting for anything is marked **next**. Subtasks held up by unfinished work (an earlier subtask, or anything that blocks the group or the subtask) show *waiting*.
- **Status suggestions.** When all subtasks are done, or every open one is blocked, or work has started on a group still marked *to do*, a line above the list suggests the group's status with a button. Minimap never changes it by itself.
- A bar and "*2 of 5 done*" show progress. Cancelled subtasks are left out of the count. Finishing every subtask does **not** finish the parent: that is still your call.

On the board and in the list, a subtask shows "↳" and its parent's title, and a parent shows a small *2/5* pill that turns green when all are done. Undo (`Ctrl/Cmd+Z`) takes back each step.

### Subtasks and blocks work together

The two links say different things. A **subtask** link says *this belongs to that* (what a task is made of). A **blocks** link says *this must finish before that can start* (the order). A task with subtasks is a **group**:

- **Its dates come from its subtasks.** The group is not scheduled itself and its own estimate is not used; it runs from the start of its first subtask to the end of its last, and it counts as critical when one of them is. The [schedule](help:schedule), [capacity](help:capacity) and a project's health count the subtasks, not the group, so nothing is counted twice.
- **A blocks link on a group applies to everything in it.** If *Approval blocks Launch*, every subtask of Launch waits for Approval. If *Launch blocks Release*, Release waits for the last subtask of Launch. You can wire things at the level you think at.
- **Blocks between subtasks puts them in order** inside the group.
- **A group can't block (or wait for) one of its own subtasks**, since that says nothing useful; Minimap refuses the link, and refuses to make a task a subtask when that would leave such a link behind. A link that would make the work wait for itself, through a group, is refused with the path, like any loop.

[What if](help:what-if) treats a slip on a group as a slip on the work in it. Archiving the group frees its subtasks (they become ordinary tasks again), and when a repeating group is finished its next copy starts without subtasks. A group whose subtasks are all cancelled is scheduled as a normal task again.

## Notes and findings

Below the reference links, **Notes and findings** is where you keep the record of a task: what you found out, what was decided with whom, what went wrong. Type in the box and press *Add note* (or `Ctrl/Cmd+Enter`). The first line becomes the note's title, and the note is saved as an ordinary [note](help:notes) that mentions the task, so it also appears on the Notes screen and in search, and is part of backups and exports. The list under the box shows the newest notes first; click one to open it in the pane and edit it (headings, lists and checkboxes work as in any note). Projects and objectives have the same section.

## Linking tasks

Open a task to link it: what it **blocks**, who it is **assigned** to (with an allocation percentage), which objective it **contributes to**. See [Links between items](help:links). For breaking a task into steps, see *Subtasks* above; for ordering work, use *blocks* links.

## Finishing, archiving and undoing

*Done* records when it was completed. *Archive* (in the detail pane) hides the task with its links. Both can be undone with `Ctrl/Cmd+Z`; see [Undo and redo](help:undo).
