# Gantt

The **Gantt** screen (`g j`) draws the whole plan on a calendar: **objectives**, the **projects** under them and the **tasks** in each project, as bars on one row each. It is a place to **see what will finish when**, **see which task waits for which**, and **move work by dragging it**.

## The rows

- Every **objective** is a row with its projects under it. Click the arrow (or press `→` / `←`) to open or fold a row. Objectives start open. A project starts **open when it needs a look** (it is late, has no room left before its target, or has blocked work) and folded otherwise, so the first thing you see is where the trouble is. **All tasks** opens every project and **Projects only** folds them again; the one that matches what you see is highlighted (if you opened a few projects by hand, neither is). Neither touches an objective you folded.
- A project that serves **several objectives** is drawn once, under the one with the soonest target, and says *+1* on its row. Point at it to read where else it serves. Projects that serve no objective sit under *No objective*.
- A task with **sub-tasks** is followed by them, one step in. Sub-tasks are only a way of organising: a parent's bar does not stretch over them (see [Tasks](help:tasks)).
- **Finished** tasks and projects are hidden until you tick *Show finished*. Meetings are not on the plan, so they are not here.
- A project's **counts** sit in a column at the right of the names: open tasks, then blocked (⊘) and done (✓), then its room (*14d room*, *2d over*). Point at them for the same in words. Point at a name to read it whole.
- **Drag the edge** between the names and the chart to make the names wider or narrower; **double-click** it to fit the longest name. Minimap remembers the width on this computer. When the names are very narrow, the counts move into the names' tooltips.
- A row with **nothing in it says so**: an objective with no projects says *No projects yet* and offers **Add a project**, and a project with no open tasks says *No open tasks* and offers **Add a task**. Both open [quick-add](help:quick-add) with the objective or project already filled in; type the name and press `Enter`.
- Under the tasks of an open project there is an **add row**: drag across it on the chart, from the first day to the last, and [quick-add](help:quick-add) opens with the project, the start, the due date and the estimate filled in; type the name and press `Enter`. Nothing is made until you do. A new task can't start before today. **+ Add a task** on that row opens quick-add with just the project.
- A task's **assignee** shows as their avatar at the right end of its row in the names column (up to three, overlapping, when it has several people). A task with nobody shows none.
- In **My view** (`v`) only what touches you is drawn.

## Narrow it down

- **Show** in the toolbar narrows the chart: **one person** (their tasks, under the projects and objectives they belong to, with the counts recounted for them), **one objective**, **one project** (the *Every project* box lists only the projects of the objective you picked, or all of them when none is picked; changing the objective lets go of a project that is not under it), and **Needs a look** (only the projects that are late, out of room or blocked). They work together, and with My view.
- **Minimap remembers the toolbar** on this computer: Chart or Table, the scale, *Show finished*, *By person*, the arrows and the *Show* choices are still as you left them when you come back, even after you close the app. (A person or objective that has since been archived or deleted is dropped.) Whenever something differs from the defaults, a **Reset** button appears at the right of the toolbar, saying how many choices changed; one click puts them all back (Chart, *Days*, *Focus*, nobody filtered). The Find box, which rows are folded and *Comfortable* are not part of it.
- **Find** (or `Ctrl/Cmd+F`) opens a box: type part of a name and every objective, project and task that matches lights up; folded rows open to show them, and the first one opens in the detail pane. `Enter` goes to the next match, `Shift+Enter` to the one before, `Esc` closes the box. (`/` is still the search of the whole app.)
- **By person** (in *Rows*) shows one row per person with their open tasks under it (a task with several people is under the first of them by name; tasks with nobody are under *Not assigned*). The weeks a person has more work than time are hatched red on their row and counted (*over 2 wk*), from the same numbers as [Capacity](help:capacity). Rows by person only read: to move work, switch it off.

## Finding your way on the calendar

- **Today** is the blue line, and today's column is marked in the dates at the top. The chart opens with today a little in from the left; **Today** (or `t`) brings it back.
- The dates at the top have the **months** on one line and the days (or the first day of each week) under them. Point anywhere on the chart and its row lights up on both sides, and the day under the pointer is named at the top (*Thu 22 Oct*).
- The chart counts **working days** only, so weekends are not drawn (see [Schedule](help:schedule)).
- **Scale**: **Days** is the wide one, with names written next to the bars; **Weeks** fits a long plan on screen; **Fit** squeezes the whole plan into the width of the window. Only **Days** is wide enough to grab the edge of a bar.
- `Ctrl` with the mouse wheel **zooms** between *Weeks* and *Days* in a few steps, keeping the day under the pointer where it is.
- To move around, scroll, use `Shift` with the mouse wheel to scroll sideways, or **drag any empty part of the chart**.
- The thin **strip** above the dates is the whole plan in miniature: each project's work, the targets, today, and a box for what the chart shows. Click or drag on it to move the chart there.
- The **legend** along the bottom explains every bar, mark and arrow in a line of small drawings (tasks, projects and objectives, and the chart itself). **?** at the end of the toolbar, or the ✕ on the legend, hides it, and **?** brings it back; Minimap remembers the choice on this computer. The legend is shown on the chart, not on the table.

## What a bar says

A task's bar runs from the day it can start to the day it will finish, worked out as in [Schedule](help:schedule). On the **Days** scale its name is written just after it. Its **look** says where the task stands, in words as well as colour:

| Look | Means |
|---|---|
| Hollow, grey outline | To do |
| Solid accent colour | In progress |
| Amber stripes (and a *blocked* chip on the row) | Blocked |
| Faded grey | Done: it stays on the days it was done |
| Dashed outline | No estimate: counted as one day |
| Accent underline and a bold name | **Critical**: any slip here moves the project's finish |
| Red stretch at the end of a bar (and *late 2d* on the row) | The part that runs past what the project's target allows |
| Thin dashed line after a bar | **Slack**: how far it can slip without moving the finish |
| A short vertical tick | The task's **due date**; red when the task is overdue |

A **project** is drawn as a grey bar for its open work, as tall as a task's, **filled from the left for the share of its tasks that are done**, with a red flag at its **target date**. The thin strip along the foot of the row, from the end of the work to the flag, is the **room** the work has: **solid green** when wide, **hatched amber** when narrow, **dotted amber** when it is closing or gone (the row says *14d room*, *no room* or *2d over*). Work that runs past the target is red, as tall as the bar. A short dark mark at the left is the project's **start date**. An **objective** looks the same, with the flag at its target; an **ongoing** objective has no end, so a faint line runs on from its work for a few weeks and fades out.

On a bar, **colour always says where the work stands**: a task's status, or a project's room. An objective's own colour is the dot by its name.

## Which task waits for which

When a person's tasks are in [order](help:order), their bars follow it: a task starts when the step above it in that person's order is finished. No arrow is drawn for that; open the task to see what holds it.

An **arrow** runs from the end of a task that must finish to the start of the task it blocks (a *blocks* link; a lag shows in the arrow's tooltip). Dashed arrows are *depends on* links between projects, or links to a task inside a folded project (the arrow then goes to the project's summary bar). The **Arrows** buttons choose how many are drawn:

- **Focus** (the default): every arrow, quietly. **Point at a task or open it** and everything it waits for, and everything waiting for it, lights up in the accent colour while the other arrows fade.
- **All**: every arrow at full strength.
- **Critical**: only the chain of tasks that decides each project's finish.
- **Off**: no arrows.

## Move work by dragging

Press on a bar and drag. The bar snaps to working days and a label says where it will land (*Wed 3 Mar → Fri 5 Mar · 3 days*); the bars it moves along with it move on screen too. Let go to save, or press `Esc` to cancel.

| Drag | What it saves |
|---|---|
| The **middle** of a task bar | Its **start date** moves, and its due date by the same number of working days. The estimate stays. |
| The **left edge** | The **start date**; the end stays, so the estimate becomes the days from the new start to the end. |
| The **right edge** | The **due date**; the start stays, so the estimate becomes the days from the start to the new end. |

What to expect:

- **A task can't start before what blocks it is done**, before its project starts, or before today. The bar stops there, a dashed line marks the limit and the label says why (*can't start before Provision EU network is done (Wed 31 Mar)*).
- **What follows moves later, and its due dates follow.** If the drag makes a task finish later, every task it blocks that moves gets its **due date pushed later by the days it moved** (a task with slack to spare absorbs part of it). Their start dates are never touched, so they stay free to move with the plan, and **nothing is ever pulled earlier**. The tasks pushed show a red *+2d* until you drag something else, and *Weekly review* counts them as slipped, because they did. The message after the drop says how many.
- **Finished work can't be dragged**; reopen it first.
- The whole drop is **one undo step**: **Undo** in the message after the drop, or `Ctrl/Cmd+Z`, puts back the task's dates and every due date it pushed.
- A short press without moving is a click and opens the item in the detail pane.

### Project and objective bars: more room

Dragging a project or an objective does not move its tasks. It gives them **room to grow into**:

- Drag a project's **flag** (or an objective's) to change the **target date**. Later gives the work more room (the shading grows and slack grows with it); earlier takes room away, and when it runs out the overrun turns red. An **ongoing** objective has no target to drag.
- Drag a project's **start mark** to change its **start date**. **No task in a project starts before the project does**, so a later start can hold work back; the left end of the bar is only there when the project has a start date.
- Drag the **middle** of a project's summary bar to move its start and target together (it needs both dates).

Each of these is one undo step. The message after the drop says the new dates and has **Undo**.

## Change dates without dragging

Everything a drag does can be done by clicking or from the keyboard:

- **Click a bar** (or press `Enter` on a row) and a small **date box** opens beside it. For a task: **Move**, **Start** and **End**, each with **−1d** / **+1d** and, for the start and end, a date to type or pick. For a project: **Start**, **Target** and **Both**; for an objective: **Target**. The same rules hold as for dragging (*can't start before …* is said, and nothing is saved), what follows gets its due dates pushed the same way, and each change is one undo step. `Esc` closes the box and takes you back to the chart.
- **`Alt+←` / `Alt+→`** move the open item a working day: a task as a whole, a project's or objective's target. **`Alt+Shift+←` / `Alt+Shift+→`** move a task's end or a project's start. An ongoing objective has no target, and the message says so.
- Every bar, flag and start mark can be pressed anywhere in a square of at least 24 pixels around it, even where what is drawn is thinner.

## Easier to read and to use

- **Comfortable** (in *Rows*) makes rows and bars taller, for a touchpad, a tablet or tired eyes. Minimap remembers it on this computer.
- **Table** (in *View*) shows the same rows as a table: name, kind, status, start, finish, due, room or lateness, critical. Folding and the keyboard work as on the chart, and `Enter` opens the date box under the row. Click a column's name to list **every task** sorted by it; click again to reverse, a third time to go back to the tree.
- With a **screen reader**, the chart is a tree: each row says what its bar shows (*EU pricing, task, blocked, critical, Wed 3 Mar to Fri 5 Mar, due Fri 5 Mar, 2 working days late*), its level, and whether it is open. Folding and opening are announced, and so is the message after every change, or why it couldn't be made.
- The open row has a **focus ring** on both sides while the chart has the keyboard, and its bar is outlined.
- When your system asks for **less motion**, scrolling to today jumps instead of gliding and the arrows don't fade.

## From the keyboard

Click the chart (or `Tab` into it), then:

| Key | Does |
|---|---|
| `↑` `↓` (or `k` `j`) | Move to the previous or next row and open it in the detail pane |
| `Home` / `End` | The first or last row |
| `PageUp` / `PageDown` | A screenful of rows up or down |
| `→` / `←` | Open or fold the row |
| `*` | Open every project |
| `t` | Bring today into view |
| `Enter` | Open the date box of the row |
| `Alt+←` / `Alt+→` | Move the open task one working day earlier or later; on a project or objective, its target |
| `Alt+Shift+←` / `Alt+Shift+→` | Move only a task's end (the due date), or a project's start |
| `Esc` | Close the date box or the find box, or cancel a drag |
| `Ctrl/Cmd+F` | Find a row by name |

## Good to know

- Dragging writes ordinary dates and estimates, so everything else follows: [Schedule](help:schedule), health, [capacity](help:capacity), the [Overview](help:overview) and [What if](help:what-if). To try a delay **without** saving it, use What if.
- A task's bar is where the schedule puts it, not where its dates say: a task with no start date starts as early as its blockers allow. Dragging its middle **pins** it with a start date; dragging only its right edge does not.
- For a loop in the blocks links the chart can't be drawn; see [Schedule](help:schedule).
