# Order of work

Without an order, Minimap starts every task as soon as its blockers and dates allow. If one person has ten tasks, all ten start today, and the [Gantt](help:gantt) draws them side by side, as if that person could do them all at once. The **Order** screen (`g q`, under People in the sidebar) is where you say what each person does first.

## Steps

Pick a person in the tabs under the header: everyone in Minimap, you first, each with their number of open tasks (or this week's load in amber, when it is over 100%). `[` and `]` go to the previous or next tab. The screen remembers who you looked at last.

Their open tasks are a **queue**, one row per task, top to bottom:

- **Step 1 is done first.** The number on the left is the step. Each step starts when the whole step above it is finished.
- **Tasks in the same step are done at the same time.** They share a number and a bracket, under a line such as *Step 3 · 2 at the same time · 150% of Uditt's time*. The share is in amber when it is more than 100%.
- Each row shows the task's type, project, estimate and the dates it now gets (*Mon 3 Mar → Fri 7 Mar*), with its due date under them. When the finish is after the due date, that line is red and says by how many working days.
- On a wide window, each row also has a **bar** on a ruler of 15 working days. The blue line is today and a dashed tick marks the due date. A started task's bar is solid. A bar with no estimate is dashed amber. A late bar is red.
- A task already **in progress** is marked *started*. It keeps the start it has; the order only holds up the steps below it. A note above the queue says so when the order has started tasks.

The line under the tabs sums it up: *6 tasks in 5 steps · Last step ends Wed 4 Nov · 1 finishes after its due date · 2 have no estimate, counted as 1 day*. A task with no estimate counts as one day, which is the most common reason dates look wrong. **Open the Gantt** goes to the [Gantt](help:gantt).

The person's other open tasks are in **Not in the order** on the right, earliest due date first, with a filter box when there are more than four. They are planned as before: as early as their blockers and dates allow.

## Putting tasks in order

- **Drag a row** onto the middle of another row to do it at the same time as that step: the step is outlined and says *together*. Drag it near the top or bottom edge of a step and a blue line shows where it becomes a step of its own. Drop it on *Not in the order* to take it out.
- **+** on a task on the right, or `a`, puts it in a new last step.
- **↑**, **↓** and **✕** appear when you point at a row. ↑ and ↓ (or `Alt+↑` / `Alt+↓`): a task that shares its step gets a step of its own just above or below; a task alone in its step moves above the step before it, or below the step after it. ✕ (or `x`) takes it out of the order.
- **Click the estimate** (an amber dash when there is none), or press `e`, and type it as in quick-add: `3d`, `0.5d`, `4h`. `Enter` saves it and the dates below move; `Esc` leaves it as it was.
- Clicking a row (or pressing `Enter` on it) opens the task on the right.

The rows move the moment you make a change. The dates fade until Minimap has worked out the new ones. If Minimap refuses the change (see *Blocks links come first* below), the rows go back and a message says why.

Keys on a row: `j` / `k` (or `↓` / `↑`) the next or previous row, `Alt+↑` / `Alt+↓` move it, `t` at the same time as the step above, `Shift+T` a step of its own, `1` to `9` into that step (a number past the last step makes a new last step), `x` out of the order, `e` the estimate, `Enter` open it.

Every change is saved at once and is **one step for [Undo](help:undo)** (`Ctrl/Cmd+Z`).

## My team

**My team** (next to *One person* in the header) shows everyone in Minimap side by side, one column each: you first, then everyone else by name. Archived people are left out. Each column has the person's name, how many steps they have and when the last one ends (or *No order yet*), and this week's load, in amber when it is over 100%. The rows are their steps: the step number, the task, the day it finishes (red when that is after its due date) and *started*. Tasks done at the same time share a rule on the left.

- **Drag a row within a column** to reorder that person's work, as in the queue.
- **Drag a row to someone else's column** to give the task to them at that place: onto a row to do it with that step, near a step's edge for a step of its own, or onto *+ N not in the order* at the bottom to give it to them without a place. The task then has that one person as its assignee (anyone else assigned to it is taken off). A message says what happened, and one Undo takes it all back.
- Click a person's name, or *not in the order*, to open their queue.

With nobody else in Minimap yet, My team shows only you, and says how to add people on the People screen.

## What it changes

The order is part of the [schedule](help:schedule): a task starts no earlier than the end of the step above it. So everything that reads the schedule follows at once: the [Gantt](help:gantt) bars, the projected finish and [health](help:overview) of projects, [What if](help:what-if), the critical path and [Capacity](help:capacity) (the person's load spreads over the weeks instead of piling up in the first).

- **The order writes the start dates.** A task below another gets its **Start date** set to the next working day after the task above finishes (a step with several tasks waits for the last one). Open the task and you will see it. When something above slips (a longer estimate, a blocker, a task not finished by its day), the dates below move down with it, as part of the same change, so one Undo takes both back.
- **The due date is filled in when it is empty.** A task in the order with an estimate and no **Due** gets the last working day of its work (start date plus estimate, so 5 days from Monday is Friday). A due date you have set is a promise and is never changed, so when work above slips, the card says it finishes after its due date.
- **Your own start dates still count.** A start date later than the order gives is kept, and the order never moves a date earlier: when a task above finishes early, the dates below stay where they are until you change them. Tasks in step 1, tasks already in progress and tasks outside the order keep the date they have.
- **Blocks links come first.** You can't put a task above one it has to wait for: Minimap says which task has to be done first. If a *blocks* link added later goes against the order, the schedule follows the link, and the Order screen says which part of the order it can't follow.
- A task leaves the order when it is **finished or cancelled**, or when you give it to **someone else** in its *Assignee* box. On *My team*, dragging it to someone's column gives it to them and puts it in their order in the same change.
- Meetings, tasks on the [backburner](help:tasks) and tasks with nobody assigned are not part of any order.
- A task with **two people** assigned has one place, and it counts in both people's orders.

Expect some projects to turn amber when you first put someone's work in order: the forecast now counts that person's time, which is usually the honest answer.
