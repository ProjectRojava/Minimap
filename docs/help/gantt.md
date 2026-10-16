# Gantt

The **Gantt** screen (`g j`) draws the whole plan on a calendar: **objectives**, the **projects** under them and the **tasks** in each project, as bars on one row each. It is a place to **see what will finish when**, **see which task waits for which**, and **move work by dragging it**.

## The rows

- Every **objective** is a row with its projects under it. Click the arrow (or press `→` / `←`) to open or fold a row. Objectives start open and projects start folded; **All tasks** opens every project and **Projects only** folds them again; the one that matches what you see is highlighted (if you opened a few projects by hand, neither is). Neither touches an objective you folded.
- A project that serves **several objectives** is drawn once, under the one with the soonest target, and says *+1* on its row. Point at it to read where else it serves. Projects that serve no objective sit under *No objective*.
- A task with **sub-tasks** is followed by them, one step in. Sub-tasks are only a way of organising: a parent's bar does not stretch over them (see [Tasks](help:tasks)).
- **Finished** tasks and projects are hidden until you tick *Show finished*. Meetings are not on the plan, so they are not here.
- **Today** is the blue line. The chart counts **working days** only, so weekends are not drawn (see [Schedule](help:schedule)). **Days** is the wide scale; **Weeks** fits a long plan on screen but is too narrow to grab the edge of a bar.
- In **My view** (`v`) only what touches you is drawn.

## What a bar says

A task's bar runs from the day it can start to the day it will finish, worked out as in [Schedule](help:schedule). Its **look** says where the task stands, in words as well as colour:

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

A **project** is drawn as a thin summary bar for its open work with a red flag at its **target date**. The shaded stretch between the end of the work and the flag is the **room** the work has (green when wide, amber when narrow or none left; the row says *14d room*, *no room* or *2d over*). Work that runs past the target is shaded red. A short dark mark at the left is the project's **start date**. An **objective** looks the same, with the flag at its target; an **ongoing** objective has no end, so its bar fades out to the right.

## Which task waits for which

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
- The whole drop is **one undo step**: `Ctrl/Cmd+Z` puts back the task's dates and every due date it pushed.
- A short press without moving is a click and opens the item in the detail pane.

### Project and objective bars: more room

Dragging a project or an objective does not move its tasks. It gives them **room to grow into**:

- Drag a project's **flag** (or an objective's) to change the **target date**. Later gives the work more room (the shading grows and slack grows with it); earlier takes room away, and when it runs out the overrun turns red. An **ongoing** objective has no target to drag.
- Drag a project's **start mark** to change its **start date**. **No task in a project starts before the project does**, so a later start can hold work back; the left end of the bar is only there when the project has a start date.
- Drag the **middle** of a project's summary bar to move its start and target together (it needs both dates).

Each of these is one undo step. The message after the drop says the new dates.

## From the keyboard

Click the chart (or `Tab` into it), then:

| Key | Does |
|---|---|
| `↑` `↓` (or `k` `j`) | Move to the previous or next row and open it in the detail pane |
| `→` / `←` | Open or fold the row |
| `*` | Open every project |
| `Alt+←` / `Alt+→` | Move the open task one working day earlier or later |
| `Alt+Shift+←` / `Alt+Shift+→` | Move only its end (the due date) |

## Good to know

- Dragging writes ordinary dates and estimates, so everything else follows: [Schedule](help:schedule), health, [capacity](help:capacity), the [Overview](help:overview) and [What if](help:what-if). To try a delay **without** saving it, use What if.
- A task's bar is where the schedule puts it, not where its dates say: a task with no start date starts as early as its blockers allow. Dragging its middle **pins** it with a start date; dragging only its right edge does not.
- For a loop in the blocks links the chart can't be drawn; see [Schedule](help:schedule).
