# What if this slips?

**What if** answers "if this takes N working days longer than planned, what else moves, and what gets late?" It changes nothing until you choose to apply it.

## Starting a scenario

- Open a task or project and press **What if this slips?** in its detail pane; it starts a 5-day scenario for that item and opens the screen.
- Or open **What if** (`g f`) directly and add items yourself with the search box.

A scenario can hold **several slips** at once (for example two things both running late). For each, set the **number of working days**; remove one with ✕. A project slip delays all its open tasks.

## What it shows

A summary first (how many tasks move, how many are newly late, the worst delay), then two views, chosen with the **Route** and **Details** buttons under it (the choice is kept in the address, so a link to `?view=details` opens the tables). **Route** is the one you land on.

### Route: the recalculated route to each finish

The critical path is the **route** to a project's finish. When a slip changes it, each affected project gets a card, like a satnav saying *recalculating*:

- **The headline**: *EU Region · recalculated: arrives 2027-04-14 instead of 2027-04-07 (+5 working days), 3 working days past target*. A project the slip reached but did not move says *no change to the route; finish unchanged* and is just that line.
- **A small timeline** with the old route **dashed and faded** and the new route in **blue**, with today and the target marked. Tasks the slip moved show both their old (dashed outline) and new (solid) position: **blue** when the task is on the new route, **grey** when it moved but is off the route because it has slack. Click a bar or a name to open the task. With more than 12 tasks only the tasks on a route are drawn.
- **A key** under the timeline says what each mark is: a dashed box is where the task was before the slip, a blue bar is on the route now, a grey bar moved but is not on the route (it has slack), and the two thin lines are today and the target date.
- **Step by step**: the route in plain sentences, each with an icon (and a key to the icons under the list, showing only the ones used). *Select a Data provider slips 5 days: the delay starts here* is where it enters (○); *moves 5 days later* is a task that passes the delay on (→), and *and 2 more tasks move 5 days later* folds a run of those into one line; *absorbs 2 of the 5 days (it has slack) and moves 3 days later* is slack soaking up part of it (a bracket), and *absorbs all 5 days: the delay stops here* soaking up all of it; *is now on the route* (+) and *is no longer on the route* (−) are tasks the new route starts or stops running through; and the last step (a flag) is always where the project now **finishes**, in red when the slip makes it newly late. A very long route keeps its first four and last three steps and says *… N more steps*.

### Details: the tables

Four lists:

| List | What each row says |
|---|---|
| **Tasks** | For each task the slip reaches: the delay passed on, how much of it **slack absorbed**, the delay that is left, old and new finish dates, and whether it is **newly late** against its due date |
| **Projects** | New projected finish against the target, and whether it is newly late |
| **Objectives** | The objectives those projects contribute to, and whether their targets are now at risk |
| **People** | Whose assigned work moves, and by how much |

A task that absorbs the slip entirely (it had slack) still appears, with delay 0: that is where the ripple stops. **Apply to plan…** works from either view.

## How it is worked out

Minimap re-runs the [schedule](help:schedule) with the slipped task held back until *its planned start + N working days*, and compares with the plan as it stands. So slack, lags, start dates, targets and cross-project links all behave exactly as they do on the Schedule. A slip never moves anything earlier, and a project that depends on another also waits for it.

## Apply to plan

**Apply to plan…** turns the scenario into real changes, after showing you exactly what they will be:

- the tasks the slips **hold back** get a **start date** pinned, and their **due dates** move by the same number of working days;
- tasks that are merely pushed along by *blocks* links are left alone: the schedule moves them by itself;
- **targets are never changed**; that decision is yours.

You confirm, and it is **all or nothing**. It is one step you can take back with `Ctrl/Cmd+Z`.

## Tips

- Try a slip on a *critical* task to see the whole chain; try one on a task with slack to see it get absorbed.
- Put several slips in one scenario to test a bad week, not just one bad task.
- The *What if it slips?* button on an [Approaching](help:approaching) strip starts a scenario with the late or blocked task already in it.
