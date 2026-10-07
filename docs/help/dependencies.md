# Dependency graph

The **Dependencies** screen (`g l`) draws what blocks what as a graph, left to right, with the critical path picked out.

## Two levels

A toggle at the top chooses:

- **Tasks**: one box per task, with arrows for *blocks* links (from the task that must finish to the one that waits).
- **Projects**: one box per project, with arrows for *depends on* links (from the project that is needed to the one that depends on it).

## Filters

Narrow the graph by **project**, **team** (including its sub-teams) or **objective**; filters combine. Items outside the filter that are directly linked to what is shown appear **dimmed** as context, so you can see what a filtered group depends on or holds up.

By default **finished** work and items with **no links** are hidden. Subtasks are the exception: being a subtask counts as a link, so a group's subtasks are always shown (inside their dashed frame) even when nothing blocks them, and when one subtask is in view the rest of its group comes with it, dimmed if your filter didn't select it. *Show finished* and *Include unlinked* bring them back. (A graph is limited to 400 boxes; if the filter is too wide, Minimap asks you to narrow it.)

## Reading it

| What you see | Meaning |
|---|---|
| Accent colour on boxes and arrows | On the **critical path** |
| A late marker | Projected finish is after its due date or target |
| A blocked marker | The task's status is *blocked* |
| Dimmed | Context outside your filter |
| A dashed frame with a title | A **group**: the boxes inside are the subtasks of the task named on the frame. The group itself has no box; click the frame to open it. A *blocks* link on the group is drawn to each of its subtasks (see [Tasks](help:tasks)) |

A *legend* at the bottom repeats these. Each box shows the item's title and a short subtitle.

## Moving around

- **Drag** the background to pan; the **mouse wheel** or the **+ / −** buttons zoom about the pointer; **Fit** shows everything.
- **Click** a box to open the item's detail pane, where you can add or remove links.

## If it says there is a loop

*Blocks* and *depends on* links can't form a loop; if old data somehow does, the screen shows a message naming the problem rather than a picture. See [Links between items](help:links).

Related: [Schedule and critical path](help:schedule), [What if](help:what-if).
