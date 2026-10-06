# What if this slips?

**What if** answers "if this takes N working days longer than planned, what else moves, and what gets late?" It changes nothing until you choose to apply it.

## Starting a scenario

- Open a task or project and press **What if this slips?** in its detail pane; it starts a 5-day scenario for that item and opens the screen.
- Or open **What if** (`g f`) directly and add items yourself with the search box.

A scenario can hold **several slips** at once (for example two things both running late). For each, set the **number of working days**; remove one with ✕. A project slip delays all its open tasks.

## What it shows

A summary first (how many tasks move, how many are newly late, the worst delay), then four lists:

| List | What each row says |
|---|---|
| **Tasks** | For each task the slip reaches: the delay passed on, how much of it **slack absorbed**, the delay that is left, old and new finish dates, and whether it is **newly late** against its due date |
| **Projects** | New projected finish against the target, and whether it is newly late |
| **Objectives** | The objectives those projects contribute to, and whether their targets are now at risk |
| **People** | Whose assigned work moves, and by how much |

A task that absorbs the slip entirely (it had slack) still appears, with delay 0: that is where the ripple stops.

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
