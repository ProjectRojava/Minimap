# Approaching: trouble elsewhere that reaches you

When you look at one project, trouble in **another** project is invisible until it lands: a blocked task over there can hold up a task here, and you only find out by opening the [dependency graph](help:dependencies) or running a [what-if](help:what-if). **Approaching** puts it where you already are, as a thin strip: *2 approaching from EU Region · first in 4 days*. Click it to see the chains.

When nothing outside is in trouble, nothing is shown. There is no "all clear" line to read.

## What counts as trouble

Something **outside** what you are looking at is in trouble when it is:

- an open task that is **overdue** (its due date has passed), **blocked** (its status), **late** (scheduled to finish after its due date) or has **no slack** (the schedule says the target of its project cannot be met with it as planned);
- a project that is **late** (its forecast finishes after its target) or whose [health](help:overview) is red.

Finished work, cancelled work and meetings are never trouble, and they cut a chain: finished work no longer holds anything up.

## How far it looks

From each open task you are looking at, Minimap walks back along the **blocks** links, through open tasks only, up to 4 links. It stops at the first task that is back inside what you are looking at (that task is a starting point of its own). Between projects it follows **depends on** links, up to 3. Each piece of trouble is shown once, with its shortest chain.

*Reaches Pen test in 3 working days* means the task it reaches is due to start in 3 working days (now, if it is under way or blocked). *Pen test has 1 day of slack* is how much room that task has before it, or the project's target, slips.

## How much it matters

- **Holding it up**: the task it reaches is already late, and this is the very task that sets its start. This is the one to deal with first.
- **Threatens it**: the task it reaches has 2 working days of slack or less, so a delay here becomes a delay there.
- **Worth watching**: there is room. These never count in the strip; they are in the list under *Also watching*.

## Where it shows

- At the top of a **project's** pane, for that project's tasks and the projects it depends on.
- On [This week](help:this-week), under the headline, for the tasks **assigned to you** and the projects you own. It is not shown when you look back at a past day.
- At the top of an **objective's** pane, for the projects that contribute to it.
- On the [Tasks](help:tasks) board and list, under the filters, when you have filtered to one project.

Click the strip to open the chains. Each one shows the path from the trouble to what it reaches, why it is in trouble, how soon it arrives, and two buttons: **Open** opens the source in the pane, and **What if it slips?** starts a [what-if](help:what-if) with the source already in the scenario (as many days as it is overdue, or 5).

## On the radar

The [radar](help:radar) draws what hits or threatens your own work as marks on its rim, in the wedge of the objective they will reach. Hover one for the sentence; click it to open the thing in trouble. Items that only deserve a look are left off.

## Good to know

- It is computed from your tasks, dates and links; nothing is stored, and it follows every change as soon as the screen reloads. Mark the blocking task done and the strip goes away.
- It looks at the same schedule as [Schedule and critical path](help:schedule), so it needs estimates and links to be useful: with no `blocks` or `depends on` links there is nothing to reach you.
