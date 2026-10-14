# Fog: how fresh your information is

A project can say *On track* only because nobody has looked at it for three weeks. Health tells you what the data says; **fog tells you how far to trust it**. Work that is in play but that nobody has changed, written about or confirmed for a while **looks its age**, everywhere it appears, so you notice it before it surprises you.

Fog changes nothing you have recorded. It is a way of looking, like [focus](help:tasks): it never moves a date, a status, health, the schedule or capacity.

## What is in play

Only work whose state matters now can be fogged. Finished, far-off and paused things never are.

| Kind | In play when |
|---|---|
| Task | Open, not a meeting, and at least one of: in progress or blocked; due within 14 days (or already late); in [focus](help:this-week) today |
| Project | Its status is *Active* (planned, paused, done and cancelled are not) |
| Objective | Not done, and not *ongoing*: an [ongoing objective](help:objectives) has its own review rhythm instead |

A task due in six months that nobody has touched is **not** fogged: it is not in play yet. Waiting-ons have their own *stale* rule (see [Waiting on](help:waiting-on)), and people, teams, notes and decisions are never fogged.

## What counts as a change

How long ago something was **last seen** is the newest of:

- a change to the item itself: any field, its status, its dates, its focus;
- a **Still accurate** (see below);
- a change to its links (a blocker added, an assignee changed);
- a [note](help:notes) that mentions it (its date counts);
- for a project: its tasks that are in play moving or being written about; for an objective: the projects and tasks that contribute to it.

A sub-task changing does **not** count for its parent: a busy child does not vouch for the parent.

## Haze and fog

Age is counted in calendar days since it was last seen. Past the first number the item is hazy, past the second it is in fog. A task seven days old is still clear; eight days is haze.

| Kind | Haze after | Fog after |
|---|---|---|
| Tasks | 7 days | 14 days |
| Projects | 10 days | 21 days |
| Objectives | 14 days | 30 days |

You can change the six numbers under [Settings](app:/settings), *Thresholds*, *Fog*. Haze must come before fog, and both are 1 to 365 days.

## What you see

A hazy or fogged item is **faded** and ends in a small chip, `≋ 19d`, that says how long since it was last seen. Fog is never colour alone: the chip is always there, and it stays at full contrast. Point at it to read why: *Fog: no change for 19 days. Is it still on track?* If the last sighting was a confirmation, it says *last confirmed 15 days ago*.

It shows on tasks and projects in lists and on the board, on the [Overview](app:/overview) (objectives, projects and top risks), on [This week](app:/), and in the detail pane, where a pill (*Fog 19d* or *Haze 9d*) sits next to the status and *Still accurate* sits beside it.

## Still accurate

Look at the item. If it is fine as it is, press **Still accurate** (in the detail pane, on a row on This week, or in the weekly review) or press `c` on the highlighted row. Nothing about the item changes; the fog clears, and stays clear for as long as it takes to haze again (7 days for a task, 10 for a project, 14 for an objective, by default). A message says so.

It is one step for [undo](help:undo): `Ctrl/Cmd+Z` brings the fog back exactly as it was.

If you change the item, write a note about it or touch its links, that clears the fog too: confirming is for when there is nothing to change.

## Gone quiet, on This week

Under *Priorities this week*, a card called **Gone quiet** lists tasks that are in play and fogged but are not already somewhere else on the page (an in-progress task of a colleague's, due next month). They are everyone's tasks, not only yours: your blind spots are mostly other people's work. Each row has the chip, who has it, and *Still accurate*. It lists up to 8, fog first and the oldest first; a line says how many more there are, and the weekly review has all of them.

Tasks that are already on This week (overdue, due today, blocked, in focus or in the plan) simply show the chip and the faded look on their own row. A late task is never hidden by fog: it keeps its red pill and also shows the chip.

When you [look back at a past day](help:this-week), there is no fog: the history of what was touched cannot be rewound that far. The tiles and the headline at the top of This week do not count fog either: they are about what is late and what is planned.

## Visibility, on the Overview

The Overview shows a tile, **Visibility**: the share of what is in play that is clear, as a percent. Important work counts more than minor work, a project counts more than a task, and haze counts half. It shows a dash when nothing is in play. Under 50% the number is amber: it is a nudge, never an alarm.

## The review step

[Weekly review](help:weekly-review) has a step, **Fog**, just before the report. It lists everything that has gone quiet, fog first and then haze, oldest first. Each row has *Still accurate*, and tasks have the usual reschedule buttons. When you have cleared it, the step button says *All clear*. The status report does not mention fog: it is your bookkeeping, not something to show a board.

## On the radar

The [radar](help:radar) shows fog the same way: a blip whose item has gone quiet is faded, with a dashed outline, and its hover text says for how long. The radar's bottom line quotes your visibility.

## Good habits

- Let the weekly review be the scouting run: open the Fog step, and for each item either change it or say it is still accurate.
- Do not confirm what you have not looked at. A cleared item with stale information is worse than a hazy one.
- Fog is not a to-do list. Work that is far off, finished or paused is left alone on purpose.
