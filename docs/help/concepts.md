# How Minimap thinks

A few ideas explain almost everything in the app.

## Items and links

Everything you record is an **item**. There are eight kinds:

| Item | What it is |
|---|---|
| **Objective** | An outcome you want, with a priority and either a target date or, for an *ongoing* one that never ends, a review rhythm |
| **Project** | Work with an owner, a start and target date, a status and tasks |
| **Task** | One piece of work: status, estimate, dates, assignee, priority |
| **Person** | Someone you work with, with a weekly capacity in hours (one person is *you*) |
| **Team** | A group of people; teams can be nested |
| **Note** | Markdown text: a meeting, a 1:1 or anything else |
| **Decision** | What was decided, the context and the rationale |
| **Waiting on** | Something you are waiting on from someone, with the date you asked |

Items are connected by **links**. A link has a type and a direction (see [Links between items](help:links)). The links are what let Minimap compute things: a *blocks* link between two tasks feeds the schedule, an *assigned to* link feeds capacity, a *contributes to* link feeds the health of an objective.

## Statuses and priorities

| Item | Statuses |
|---|---|
| Objective | on track, at risk, off track, done (your own assessment; computed health is shown beside it) |
| Project | planned, active, paused, done, cancelled |
| Task | to do, in progress, blocked, done, cancelled |
| Decision | proposed, decided, superseded |

**Priority is 1 to 5 and 1 is the highest.** The default is 3.

## Dates and working days

Dates are written `YYYY-MM-DD` (for example `2027-03-31`) everywhere. The schedule counts in **working days**, Monday to Friday unless you change it in [Settings](help:settings), and an estimate such as `4h` is turned into days using your **hours per working day** (8 unless you change it).

## What is computed and what you type

| You type | Minimap works out |
|---|---|
| Tasks, estimates, start and due dates | When each task can start and finish (the [schedule](help:schedule)) and which tasks decide the finish (the critical path) |
| Blocks and depends-on links | What is held up by what, and the effect of a slip ([What if](help:what-if)) |
| Assignments and weekly hours | Each person's load per week ([Capacity](help:capacity)) |
| Tasks, targets and links | The health of each project and objective, with reasons ([Overview](help:overview)) |
| What happened this week | The weekly review and its status report ([Weekly review](help:weekly-review)) |

## Archiving, deleting and undo

- **Archiving** hides an item (and its links) without deleting it. Use [Undo](help:undo) to bring it straight back; archived items stay in the database, can be found with the *Archived* switch in [Search](help:search), and are included in your [export](help:export).
- **Deleting for good** is only possible for something already archived, and asks you first. It can't be undone.
- **Undo** (`Ctrl/Cmd+Z`) takes back your last change. See [Undo and redo](help:undo).
- Every change is also written to the item's **activity** history, shown at the bottom of its detail pane.

## The detail pane

Click any row, or press `Enter` on it, and its **detail pane** opens on the right: every field, its links grouped by type (and editable), attached files and its activity. `Esc` closes it.

The pane is made of **sections**, each under its own header strip with a coloured dot, a count where it helps (*Links · 3*) and, at the right, the buttons that add to that section (*New sub-task…*, *New linked task…*, *Attach file…*). The item's own fields are always open; every other section can be **folded** by clicking its header, and Minimap remembers which ones you folded. *Activity* and *Archive* start folded, so the pane opens on what you use most. Inside the fields, thin lines separate the groups (title and description, then status and who, then dates).

## Where your data lives

In one database file in the app's data folder on this computer. *Settings → Data & backup* shows the exact place and can open the folder. See [Settings](help:settings), [Backup and restore](help:backup-restore) and [Google Drive](help:google-drive).
