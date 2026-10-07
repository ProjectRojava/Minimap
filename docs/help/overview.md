# Overview and health

The **Overview** (`g o`) is the state of everything on one screen. It is computed from your tasks, dates and links; you don't maintain it.

## What is on it

- **Counts**: how many projects are red, amber, green and *not scored*.
- **Top risks**: the five worst things right now, most urgent first (and how many more there are).
- **Objectives**, each with its computed health and the projects under it (with their weights), and **projects with no objective** listed separately. A goal shows its target date; an *ongoing* objective shows *ongoing* and when its review is due (or how overdue).
- **Overloaded people**: over 100% of capacity in the next five working days, or with too many open tasks. See [Capacity](help:capacity).
- **Stale waiting-ons**: things you are waiting on for too long. See [Waiting on](help:waiting-on).
- A line recalling the **thresholds** in force.

Click any row to open the item and see why.

## Health marks

Health is a shape first and a colour second, so it reads without colour:

| Mark | Meaning |
|---|---|
| Filled dot (red) | **Red**: off track |
| Half dot (amber) | **Amber**: at risk |
| Empty dot (green) | **Green**: on track |
| A dash | **Not scored**: nothing to judge |

Each mark comes with **reasons** in words, for example *projected 6 working days late (target 2027-03-31); 3 tasks overdue, 2 blocked (5 of 12 open); 4 of 12 open tasks have no estimate.*

## How a project's health is decided

A project's health is its **worst signal** of three:

| Signal | Amber from | Red from |
|---|---|---|
| **Projected lateness** against its target date | 1 working day late | 5 working days late |
| **Overdue or blocked** share of open tasks | 15% | 35% |
| **Unestimated** share of open tasks | 50% | 80% |

These are the defaults; change them under *Settings → Thresholds*. A score from 0 to 100 sits behind the colour (above 60 green, 26–60 amber, 25 and below red). Projects that are **done, cancelled, paused**, or have **no open tasks** are *not scored*.

## How an objective's health is decided

A **weighted average** of the projects and tasks that contribute (by their weights), with **one red contributor capping the objective at amber**. The objective's own target date is judged like a project's. Compare it with the status *you* set on the objective: when they disagree, that is worth a conversation.

## What makes a top risk

Amber and red projects, plus open tasks of **priority 1 or 2** that are overdue or blocked. Risks are ranked by *how bad* × *how important* × *how big*: a project counts fully, a task about 60%, and a project borrows the priority of the highest-priority objective it feeds.

## When something is missing

If the *blocks* links form a loop, lateness and workload can't be judged and the Overview says so. Fix the loop (see [Links between items](help:links)). If many things are *not scored*, check that projects have open tasks and that tasks have estimates ([Schedule and critical path](help:schedule)).
