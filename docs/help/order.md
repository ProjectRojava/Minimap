# Order of work

Without an order, Minimap starts every task as soon as its blockers and dates allow. If one person has ten tasks, all ten start today, and the [Gantt](help:gantt) draws them side by side, as if that person could do them all at once. The **Order** screen (`g q`, under People in the sidebar) is where you say what each person does first.

## Steps

Pick a person at the top (you, the first time). Their open tasks are shown as **steps**, top to bottom:

- **Step 1 is done first.** Each step starts when the whole step above it is finished.
- **Cards side by side in one step are done at the same time.** The step's header says how many, and the share of the person's time they add up to (*2 at once, 150%*), in amber when that is more than 100%.
- Each card says its project, its estimate and the dates it now gets (*Mon 3 Mar → Fri 7 Mar*). When the finish is after the due date, it says so in amber.
- A task already **in progress** is marked *started*. It keeps the start it has; the order only holds up the steps below it.

The person's other open tasks wait under the steps in **Not in the order yet**, earliest due date first. They are planned as before: as early as their blockers and dates allow.

## Putting tasks in order

- **Drag a card** onto a step to do it at the same time as that step, onto the space between two steps (or above the first, or below the last) to give it a step of its own there, or onto *Not in the order yet* to take it out.
- **Add** on a card in the tray puts it in a new last step.
- **↑** and **↓** on a card (or `Alt+↑` / `Alt+↓` when it has the keyboard focus): a card that shares its step gets a step of its own just above or below; a card alone in its step moves above the step before it, or below the step after it.
- **✕** takes a card out of the order. Clicking a card (or pressing `Enter` on it) opens the task on the right.

Every change is saved at once and is **one step for [Undo](help:undo)** (`Ctrl/Cmd+Z`).

## What it changes

The order is part of the [schedule](help:schedule): a task starts no earlier than the end of the step above it. So everything that reads the schedule follows at once: the [Gantt](help:gantt) bars, the projected finish and [health](help:overview) of projects, [What if](help:what-if), the critical path and [Capacity](help:capacity) (the person's load spreads over the weeks instead of piling up in the first).

- **The order writes the start dates.** A task below another gets its **Start date** set to the next working day after the task above finishes (a step with several tasks waits for the last one). Open the task and you will see it. When something above slips (a longer estimate, a blocker, a task not finished by its day), the dates below move down with it, as part of the same change, so one Undo takes both back.
- **The due date is filled in when it is empty.** A task in the order with an estimate and no **Due** gets the last working day of its work (start date plus estimate, so 5 days from Monday is Friday). A due date you have set is a promise and is never changed, so when work above slips, the card says it finishes after its due date.
- **Your own start dates still count.** A start date later than the order gives is kept, and the order never moves a date earlier: when a task above finishes early, the dates below stay where they are until you change them. Tasks in step 1, tasks already in progress and tasks outside the order keep the date they have.
- **Blocks links come first.** You can't put a task above one it has to wait for: Minimap says which task has to be done first. If a *blocks* link added later goes against the order, the schedule follows the link, and the Order screen says which part of the order it can't follow.
- A task leaves the order when it is **finished or cancelled**, or when you give it to **someone else** in its *Assignee* box.
- Meetings, tasks on the [backburner](help:tasks) and tasks with nobody assigned are not part of any order.
- A task with **two people** assigned has one place, and it counts in both people's orders.

Expect some projects to turn amber when you first put someone's work in order: the forecast now counts that person's time, which is usually the honest answer.
