# Links between items

Links are what make Minimap more than a list. They are how it knows what blocks what, who has what, and which work serves which objective.

## Adding a link

Open any item and find the **Links** section of its detail pane (a task's **Links** section sits right under its fields and also has buttons to link a new or an existing task: see [Tasks and the inbox](help:tasks)). Choose **Add a link…**, pick the **relation** (only the ones that make sense for this kind of item are offered, in both directions), then search for the other item and pick it. Some links have details you can edit in place. Remove a link with its ✕.

(Addresses of web pages and Drive files are a different thing: a task's **Reference links**, see [Tasks and the inbox](help:tasks).)

Some relations are edited where they live instead: a task's assignee on the task, a project's objectives and dependencies on the project, a person's manager and teams on the person.

## The relations

| Relation | Goes from → to | Details | Used for |
|---|---|---|---|
| **blocks** | task → task | lag in working days | The [schedule](help:schedule), critical path, [what-if](help:what-if). Works across projects |
| **depends on** | project → project | a note | Project order and knock-on slips |
| **contributes to** | project or task → objective | weight 0 to 1 | [Objective](help:objectives) health |
| **assigned to** | task → person | allocation 1 to 100% | [Capacity](help:capacity) |
| **member of** | person → team | lead or member | [Teams](help:people-teams) |
| **reports to** | person → person | none | The reporting lines (one manager each) |
| **relates to** | any → any | a note | A loose connection |
| **mentions** | note → any | none | Made by `@` in [note](help:notes) text; don't add by hand |
| **affects** | decision → project, task or objective | none | [Decisions](help:decisions) |
| **about** | waiting-on → task or project | none | [Waiting on](help:waiting-on) |
| **supersedes** | decision → decision (newer → older) | none | Marks the older decision superseded |
| **part of** | task → task (sub-task → parent) | none | [Sub-tasks](help:tasks): organising a task into pieces. A task is part of one task, one level deep. Moves no date and holds nothing up |

Direction matters: *Design blocks Build* means Build waits for Design. The task pane shows the same link from both ends, worded to suit each (*Blocks* on Design, *Blocked by* on Build).

**Blocking and part of are different things.** *Blocks* is about the order of the work: one task can't start until another is done, whether or not they have anything else to do with each other. *Part of* is about structure: a task is a piece of a bigger task. A task can be a piece of one task and blocked by another, and two tasks can be linked both ways. Only *blocks* links move dates, the critical path or the dependency graph.

## Loops are refused

A link that would make a loop in **blocks**, **depends on**, **reports to** or **supersedes**, or put a team inside itself, is refused with the path that would loop:

> Can't add this link: this would create a loop — Design → Build → Test → Design

Break the loop by removing or reversing one of the links, then add the one you wanted.

## Details worth knowing

- **Lag** on a *blocks* link adds waiting time: "Design blocks Build, lag 2" means Build can start two working days after Design finishes.
- **Weight** on *contributes to* says how much of the objective the item is (0 to 1). It defaults to 1.
- **relates to** is symmetric: A–B and B–A are the same link, so it is offered only once.
- **Archiving** an item archives the links touching it; restoring it brings back the links that went with it (unless the item at the other end is archived).
- Changing a link's details (weight, lag, role) is not undoable; adding and removing links is. See [Undo and redo](help:undo).
