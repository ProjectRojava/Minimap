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
- **Archive**: asks first and lists the open tasks assigned to them. Their assignment links are archived with them; the tasks themselves stay (unassigned).

## Teams

The **Teams** screen shows teams as a tree. A team has a name, a description and an optional **parent team**, so you can nest them (a team can't be placed inside itself or one of its own sub-teams). Its detail pane lists its **members**, with their roles, and its sub-teams. You can add or remove members from the team's pane or from the person's.

A team can only be archived once it has no active sub-teams.

## How people and teams are used

- **Assignment**: a task is assigned to one person, optionally with an *allocation percentage* (for someone giving half their time to it).
- **Capacity**: the load per person per week, against their weekly hours.
- **Filters**: the [Dependencies](help:dependencies) graph can be filtered to a team (including its sub-teams).
- **Mentions**: `@` in a [note](help:notes) links to a person.

See also [Links between items](help:links) for the *member of* and *reports to* links.
