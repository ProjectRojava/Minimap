# Schedule and critical path

Minimap works out **when each task can really start and finish**, from your estimates, dates and *blocks* links, and which tasks **decide the finish date** (the critical path). You don't enter forecasts; you enter the work and the links.

## Where to see it

Open a project: its detail pane has a **Schedule** section with

- a **forecast line**: projected finish, target date, and how many working days late or early;
- the **critical path**: the tasks that decide the finish, in order;
- a **timeline** (switch between *Days* and *Weeks*; *Show finished* adds completed tasks) with today and the target marked, slack drawn as thin lines, and late or unestimated work styled differently. Hover for the details of a bar; click a bar to open the task.

The [Dependencies](help:dependencies) screen draws the same thing as a graph, and the [Overview](help:overview) uses the forecasts for health.

## How it is worked out

1. **Working days only.** Monday to Friday by default (change it in [Settings](help:settings)). A weekend start moves to the next working day. Time is counted from today.
2. **Forward pass.** Each open task starts as soon as everything that **blocks** it has finished (plus any *lag*), but not before its **start date**, and runs for its **estimate**. Links across projects count: a task in another project can hold this one up whichever project you are looking at.
3. **Finished work is fixed** at the date it was actually completed; **cancelled** and archived tasks are ignored.
4. **Backward pass.** Working back from the project's **target date** (or, if it has none, from its own projected finish) gives each task the *latest* it could start without making the project late.
5. **Slack** = latest start minus earliest start: how many working days a task can slip without moving the finish. **Critical** tasks are the ones with the least slack in their project.

## Reading the results

- **Zero slack and critical**: any slip here slips the project. With no target date, slack is never negative.
- **Negative slack** means the target can't be met: the late tasks show *late by N working days* and the project shows how far past its target it will finish.
- **Unestimated** tasks (no estimate) are scheduled as one day and flagged. The more of a plan is unestimated, the less you can trust the forecast. The project's health says so.
- A task with a **start date in the future** won't be scheduled before it, even if nothing blocks it.
- **Meetings** are left out: they happen at a time and take no days of work, so they are not unestimated and are never on the critical path.

## A loop in the blocks links

If *blocks* links form a loop the work can't be ordered; Minimap says so and the forecasts are unavailable until you remove one link. (Normally it refuses to create a loop in the first place: see [Links between items](help:links).)

## Getting a useful forecast

- Put **estimates** on tasks (`3d`, `4h`), even rough ones.
- Add **blocks** links for real dependencies, including between projects.
- Give each project a **target date** so slack and lateness mean something.
- Keep statuses honest: finishing a task fixes it at the date you finished it.

Try the effect of a delay with [What if](help:what-if).
