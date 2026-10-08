# Tasks and the inbox

A **task** is one piece of work. The **Tasks** screen shows all of them as a **board** of status columns, or as a list (switch with *List / Board* at the top right). The **Inbox** is the list showing only tasks that belong to no project, a good place for things you have captured and not yet filed.

## Adding tasks

- **Type a title** in the box at the top and press `Enter`. Press `n` (outside a text box) to jump to it.
- **Paste a list.** Paste several lines at once and Minimap shows a preview of one task per line (list markers like `-`, `*`, `1.` and `[ ]` are stripped); confirm to add them all or none.
- **Quick-add** with `Ctrl/Cmd+K` takes a whole line: `task Fix login timeout @priya #api-launch !2 due:fri est:3d`. See [Quick-add and the palette](help:quick-add).

New tasks are assigned to you unless you say otherwise.

## Fields

At the top of a task's detail pane a row of coloured pills shows where it stands at a glance: its status, priority (P1 and P2 in amber), its due date (red "overdue" when it has passed and the task is still open, amber "due today"), its type, and the objectives its project serves. The Status and Priority dropdowns are coloured the same way and change colour as you pick.

| Field | Notes |
|---|---|
| Title, description | Plain text |
| Status | to do, in progress, blocked, done, cancelled |
| Priority | 1 (highest) to 5; default 3 |
| Type | Optional kind of work: design, decision, bug and so on. See *Task types* below |
| Project | Optional. No project means the inbox |
| Assignee | One person, or nobody |
| Estimate | `3d`, `1.5d`, `4h` or a bare number (days). Hours use your hours-per-working-day setting. Leave it empty if you don't know |
| Start date, due date | Optional. A start date means "not before" for the schedule |
| Repeats | Optional rule such as `mon` or `2w`: see [Recurring items](help:recurring) |
| Reference links | Web pages, Google Drive files and other addresses: see below |

A task with **no estimate** is scheduled as one day and flagged *unestimated*, so projects can tell you how much of their plan is a guess.

## The board

The board has a column for each status: **To do**, **In progress**, **Blocked** and **Done**, plus **Cancelled** when you tick *Show cancelled*. Each card shows the title, its project, the priority, the due date, the estimate and the assignee's initials in a circle. Each person has a **colour of their own** (the same colour wherever they appear on the board; hover the circle for their name), so who has what is clear at a glance. **Priority** is a pill: P1 solid amber, P2 tinted amber, the rest quiet. The **due date** is a pill that gets louder as the day nears: *In 5d* (amber tint) within a week, *Tomorrow* and *Today* (solid amber), *3d overdue* (solid red); further out, or when the task is done or cancelled, it is the plain date. Hover it for the exact date. A small coloured label above the title is the task's **type** (see *Task types* below); a finished task shows how it did against its due date (*on time*, *2d late*, *1d early*). A circular arrow means the task repeats. Under the project, a **blue chip with a chain** says how many tasks it is linked with (*2 linked tasks*: parents, children or related), and a **green chip with a paperclip** says what is attached (*2 files, 1 web link*). Cards with neither show no chip. A task wears the colour of the **objective its project contributes to**: a coloured edge and a chip with the objective's name on the board, an edge and a dot in the list. Tasks without a project, or whose project serves no objective, stay plain. See [Objectives](help:objectives).

- **Deadlines warm the card.** An open task with a due date starts to take on a red tint 14 days before it is due. The tint grows a little every day, is clearly red on the due day and a touch stronger once it is overdue, so what is closest to its deadline stands out as you scan the board. Done and cancelled tasks, and tasks with no due date, stay plain; the card you have open or selected shows the normal highlight instead. The list rows warm the same way.
- **Drag a card onto another column** to change its status. The column lights up where it will land, the card moves at once, and the change is saved behind it (you can undo it with `Ctrl/Cmd+Z`).
- **Click a card** to open it in the detail pane. The tasks it is linked to light up across all columns (a bright border), the other cards fade back, and arrows join them: a solid arrow points from a child to its parent (the task that waits for it), a dashed line joins related tasks. The top of the board counts the linked tasks. Click on any empty part of the board (blank space in a column, between the columns) and the board goes back to normal while the pane stays open; click the card again to light the links up again. Closing the pane, or opening a task with no links, also returns the board to normal. Linked tasks that the filters hide, or that are older finished tasks under *Show older*, are not drawn. See *Linking tasks* below.
- **The ⋯ menu** on a card (it appears when you point at the card) has: *Open*; *Link a new task…* and *Link an existing task…* (the dialog described under *Linking tasks*, already pointed at this card's task); and *Archive*, which hides the task and its links. Archiving shows a message saying `Ctrl/Cmd+Z` brings it back.
- **Add a task straight into a column** with the `+` in its heading (or `n` for the To do column). Type a title and press `Enter`; the box stays open for the next one. If you filtered by a project, new tasks join that project.
- **Order inside a column** is chosen with the *Sort* dropdown in the filter bar, and applies to every column at once. *Default* puts the earliest deadline first (tasks with no deadline last, a tie going to the higher priority) and shows Done with the most recently finished first. *Deadline, soonest* and *Deadline, latest* order every column, Done included, by due date; *Priority* puts P1 first, then the earliest deadline. Your choice is remembered on this computer. Dragging changes the status, not the position. Done shows the 15 newest in its order; press *Show older* for the rest.
- Dropping a repeating task on **Done** creates its next one, exactly as finishing it anywhere else does.

Keyboard: `j` / `k` move through the cards column by column, and `x`, `s` and `1`–`5` act on the highlighted card (see below).

## Finding tasks

The filters above the board or list combine: text (matches title, description, project and assignee), project, assignee, **type** and a due-date range. The list adds a status filter and *Show done*: finished and cancelled tasks are hidden there unless you ask for them or pick that status. Switching between board and list keeps your filters. A filter or sort that is set to something other than its default is **ringed in the accent colour** (and *Due* and *Show done / Show cancelled* turn the accent colour), so you can see at a glance why the list is shorter than you expect; *Reset (N changed)* appears at the right of the bar and puts everything, including the board's sort, back to its default.

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

## Task types

A **type** says what kind of work a task is: *Design*, *Build*, *Decision*, *Review*, *Research*, *Bug* or *Admin* to start with. A task has **one type or none**; tasks you already had stay without one until you choose.

- **Set it** with the *Type* dropdown next to Status and Priority in the task's pane, with `type:` in [quick-add](help:quick-add) (`task Pick the data store type:decision due:fri`), or in the *Type* box when you link a new task.
- **See it** as a small coloured label on the board card, after the title in the list, and among the pills at the top of the pane.
- **Filter by it** with the *Type* dropdown above the board or list. New tasks you add while a type is chosen get that type.
- **Make your own** in [Settings](help:settings) under General, *Task types*: add a type, rename it (every task that has it follows), pick one of eight colours, or archive it. A type is never deleted: tasks that have an archived type keep it (shown dimmed), and it is simply no longer offered for new tasks. Restore brings it back.

A type is a label. It does not change the schedule, capacity or health.

### Decisions that have a date

For a decision you have to make by a certain day, make a task of type *Decision* and give it a **due date**: that is the day you planned to decide. When you mark it done, the pane and the board card say how it went against that date: *Done 2027-03-05, 2 days late*, *on time* or *1 day early*. If you push the due date, the pane also keeps the first one: *Originally planned for 2027-02-26, moved twice*, and for a finished task how it did against that first date. (The record of what you decided and why belongs on the [Decisions](help:decisions) screen; link it to the task.) Days are calendar days.

## Reference links

A task's detail pane has a **Reference links** section for the documents and pages behind the work: a Google Doc or Drive folder, a ticket, a spec, a web page, an email address.

- **Add one**: paste the address into the first box, optionally type a name in the second, and press `Enter` or *Add link*. An address without `https://` (like `drive.google.com/...`) gets it added. Without a name, the list shows a short form of the address.
- **Open one**: click it. It opens in your computer's browser (or mail program for an email address), so Minimap never sees what is behind it and needs no access to your Drive.
- **Remove one**: the ✕ at its right. `Ctrl/Cmd+Z` brings it back.
- Only web addresses (`http`, `https`) and email addresses (`mailto:`) are accepted. Anything else, such as a path on your computer or a `file:` address, is refused with a message; to keep a file itself, use **Attach file** in the Attachments section of the pane instead.
- A task can hold up to 50 links. Adding the same address twice keeps one.
- When a repeating task is finished, its next one keeps the links.

Links travel with the task: they are saved with it, synced and backed up like the rest, and included in the [full export](help:export) and in the Markdown project files.

## Notes and findings

Below the reference links, **Notes and findings** is where you keep the record of a task: what you found out, what was decided with whom, what went wrong. Type in the box and press *Add note* (or `Ctrl/Cmd+Enter`). The first line becomes the note's title, and the note is saved as an ordinary [note](help:notes) that mentions the task, so it also appears on the Notes screen and in search, and is part of backups and exports. The list under the box shows the newest notes first; click one to open it in the pane and edit it (headings, lists and checkboxes work as in any note). Projects and objectives have the same section.

## Linking tasks

The **Links** section of a task's pane (right under its fields) is the one place for all of its links. It shows its **parents** (the tasks that wait until this one is done), its **children** (the tasks this one waits for) and which tasks are **related**, each with its status (click one to open it, ✕ to remove the link), then its other links (an objective it contributes to, decisions that affect it, and so on). Three buttons add links:

- **New linked task…** makes a new task already joined to this one. You can give it a type as you make it.
- **Link an existing task…** joins a task you already have.
- **Link to something else…** adds any other kind of link (to an objective, a decision, a note and so on): choose the relation, then search by name.

Both open a small dialog (also reachable from the board, below). Pick what the *other* task is to this one; a line under the choices says, by name, which task waits for which:

| Choice | Meaning |
|---|---|
| **Parent** | This task is part of the other one. The other task waits until this one is done |
| **Child** | The other task is part of this one. This task waits until it is done |
| **Related** | Connected, with no order |

Parent and child are two names for the same *blocks* link (a parent can't start until its children are done), so the schedule, critical path and dependency graph use them as they always have. A task can have several parents and several children.

For a new task, type its title and press `Enter` (or *Create and open*): it joins this task's project and priority, is assigned to you, and opens in the pane so you can fill it in. For existing tasks, search the list and **tick one or more** (for example all the children of a parent), then press *Link*; tasks already linked to this one are not listed. Several tasks are linked all together or not at all, and `Ctrl/Cmd+Z` takes them back in one step. A link that would make the work wait for itself is refused with the path, as always (see [Links between items](help:links)). Creating a linked task is one step for [Undo](help:undo).

Who a task is **assigned** to is set in its fields, above.

## Finishing, archiving and undoing

*Done* records when it was completed. *Archive* (in the detail pane) hides the task with its links. Both can be undone with `Ctrl/Cmd+Z`; see [Undo and redo](help:undo).
