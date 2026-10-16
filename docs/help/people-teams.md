# People and teams

Minimap keeps **people** and **teams** as records, so that work can be assigned, workload can be measured and 1:1s have somewhere to live. They are not user accounts: nobody logs in.

## People

The **People** screen lists everyone, with *New person* at the top. A person has:

| Field | Notes |
|---|---|
| Name, role, email | Email is optional |
| **Weekly capacity** | Hours per week they can give. Default 40 (change the default in [Settings](help:settings)). [Capacity](help:capacity) divides their scheduled work by this |
| Notes | Anything you want to remember about them |

One person is **you** (marked *you*, created on first launch). You can't archive or delete yourself, and `@me` in [quick-add](help:quick-add) means you.

### The person's detail pane

- **Organization**: their **manager** (one person; a loop of managers is refused), their **direct reports**, and their **teams**, each with a role of *lead* or *member*.
- The number of **active tasks** assigned to them (the tasks themselves are listed under *Links → Assigned tasks*), the **waiting-ons** you have open on them, and the **notes** that mention them, with a *New 1:1* button that starts a 1:1 note already mentioning them. A 1:1 appears on [This week](help:this-week) for the week it is dated.
- **Archive**: asks first, in a dialog that lists the open tasks assigned to them. You can also point at a row on the People screen and press its **Archive** button (not shown for yourself); `Ctrl/Cmd+Z` brings the person back. Their assignment links are archived with them; the tasks themselves stay (unassigned).

## Your people

On [This week](help:this-week) (a card after the 1:1s) and at the top of the **People** screen, a small **frame** shows you and each of your **direct reports**: the people who have a *reports to* link to you (set a person's manager in their pane). They are listed **by name**, never by how busy they are. **Everyone below them is shown too**: if a direct report has people reporting to them, those people appear in a group under their manager (*Reporting to Priya*), indented, and so on down however many levels there are. You can turn that off under [Settings](help:settings), *Thresholds → Your people*.

A frame has three lines:

1. their initials in their colour, their name and role (yours says *You*);
2. a **load bar** and the number: *72% this week* is the same figure the [Capacity](help:capacity) heatmap shows for their cell this week. Past 100% the bar is full and the part over capacity is drawn in amber at its end. *No capacity set* when there is nothing to measure it against;
3. up to three small pills with facts, then *+N*:
   - *Over capacity*: this week's load is above 100%.
   - *12 open tasks*: more open tasks than the limit in [Settings](help:settings).
   - *1:1 due · 18 days*: it has been 18 days since the last 1:1 note that names them (or since you added them, if there has never been one), and that is more than the rhythm in Settings (14 days to start with). A 1:1 dated in the future has not happened yet.
   - *Waiting on them · 2 (1 stale)*: what **you** wait on them for. Amber when some of it is stale, a plain count (grey) otherwise. Snoozed waiting-ons are not counted.
   - *1 blocked*, *2 overdue*: their open tasks that are blocked or past their due date (the same colours as on task lists).

Point at a frame, or tab into it, for three buttons: **New 1:1** (starts a 1:1 note with them and opens it), **Tasks** (the Tasks screen filtered to their tasks) and **Capacity** (their row on the heatmap, this week chosen). Clicking the frame itself opens the person in the pane. On the People screen `j` and `k` reach the frames first.

Which 1:1 counts as *with* someone: a 1:1 that mentions one person is with them; one that mentions several is with the people whose name is in its title (*1:1 with Priya*), or with everyone it mentions when none is.

**Frames are about support, never performance.** They show load and neglect (yours, of them): there are no scores, no ranking, no health for people, and nothing is sorted by how busy someone is. If nobody reports to you the card shows just you and says what to do: open a person on People and set their manager to you (their pane's *Organization → Manager*).

## Teams

The **Teams** screen shows teams as a tree. A team has a name, a description and an optional **parent team**, so you can nest them (a team can't be placed inside itself or one of its own sub-teams). Its detail pane lists its **members**, with their roles, and its sub-teams. You can add or remove members from the team's pane or from the person's.

A team can only be archived once it has no active sub-teams.

## How people and teams are used

- **Assignment**: a task is assigned to one person, optionally with an *allocation percentage* (for someone giving half their time to it).
- **Capacity**: the load per person per week, against their weekly hours.
- **Filters**: the [Dependencies](help:dependencies) graph can be filtered to a team (including its sub-teams).
- **Mentions**: `@` in a [note](help:notes) links to a person.

See also [Links between items](help:links) for the *member of* and *reports to* links.
