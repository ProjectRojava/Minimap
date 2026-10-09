# Projects

A **project** is work with a purpose: an owner, a start and target date, a status, a priority and its tasks.

## The list and the board

The **Projects** screen has two layouts, switched at the top right:

- **List**: projects grouped under the objective they contribute to (a project under several objectives appears under each; ones under none are listed last), then by status and priority.
- **Board**: five columns (planned, active, paused, done, cancelled). **Drag a card to another column** to change its status.

Each project shows the **objectives** it contributes to as coloured chips (the list has an *Objective* column; a card on the board has them under its title), and a coloured edge in the colour of its first objective (on the Tasks board, a task's card has the objective's colour as its border). Under an objective's heading in the list, rows take that heading's colour. Colours are explained under [Objectives](help:objectives).

Filters narrow by status, owner or objective. *New project* opens a short form.

When the list is narrow (for example with the detail pane open) the *Owner* and *Target* columns are hidden so the project title stays readable; widen the window or close the pane to see them. The owner and target date are always in the detail pane.

## The handle

Every project has a **handle**, a short unique name such as `api-launch`, made from its title when you create it. You use it in quick-add (`#api-launch`) to put a task in the project. Handles must be unique among active projects; archiving a project frees its handle. You can change the handle in the detail pane.

## The detail pane

The pane opens with a row of coloured pills: the status, the priority (P1 and P2 in amber), the target date (red "overdue" once it has passed while the project is still open) and the objectives the project serves. The Status and Priority dropdowns are coloured the same way, as are the status words of the project's tasks and objectives further down.

| Section | What you do |
|---|---|
| Fields | Title, handle, description, dates, status, priority, owner |
| Objectives | Which objectives it contributes to, each with a **weight** from 0 to 1 (how much of the objective this project is) |
| Dependencies | Which projects it **depends on** and which need it. Loops are refused |
| Health | The computed health with reasons: see [Overview](help:overview) |
| Schedule | Forecast finish, the critical path and a timeline: see [Schedule and critical path](help:schedule) |
| Tasks | The project's tasks (edit them on the Tasks screen) |
| Notes and findings | Write a note about the project, and open the ones that mention it: see [Notes](help:notes) |
| What if | Start a "what if this slips?" scenario: see [What if](help:what-if) |

## Archiving a project

Archiving asks what to do with the project's open tasks: **archive them too**, or **move them to the inbox**. Either way it is one step you can undo with `Ctrl/Cmd+Z`.

## Tips

- Give projects a **target date** and tasks **estimates**; that is what lets Minimap say whether the project will finish on time.
- A project with no open tasks, or one that is paused, done or cancelled, is *not scored* for health.
