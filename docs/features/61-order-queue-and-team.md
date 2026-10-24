# 61 — Order as a queue, and My team

Status: Done — awaiting manual check · Milestone: — · Priority: Should
Depends on: 49, 60

## Goal
Spec 60's Order screen works, but it is hard to read: each step is a full-width box with one narrow card in it, every date reads the same, and nothing says why the order seems to do nothing (the tasks are already started, or have no estimate). Make it a **queue**: one row per task, numbered by step, with a small bar on a shared ruler so time is visible. And give the person who plans for a team a **My team** view: everyone in Minimap side by side, where dragging a task to someone else's column gives it to them at that place in their order. The design exploration is in the Order redesign brainstorm (options A and D).

## Scope
**In**
- **One person / My team** toggle in the Order header, remembered on this computer.
- **One person (the queue)**:
  - The person picker is a strip of tabs: everyone who isn't archived, you first, then by name, each with their number of open tasks, or this week's load in amber when it is over 100%. (A *Someone else* dropdown after the tabs only shows when someone is left out of them.) `[` and `]` switch to the previous or next tab.
  - A summary line: *6 tasks in 5 steps · Last step ends Wed 4 Nov · 1 finishes after its due date · 2 have no estimate, counted as 1 day*.
  - A note when tasks in the order are already started: they keep their own start; the order only holds up what comes after them.
  - Rows, top to bottom. Columns: a grip, the step number (on the first row of a step), the title with type, project, *started* and the share of time when below 100%, the estimate, a bar on a ruler of 15 working days (today, the due date as a tick; started bars solid, unestimated bars hatched, late bars red; hidden on narrow windows), and the dates with the due date (*on time*, or how many working days late, in red).
  - A step with several tasks gets a label row (*Step 3 · 2 at the same time · 150% of Uditt's time*, amber over 100%) and a bracket that groups its rows.
  - The tray (*Not in the order*) is a panel on the right with a filter box, each task with a **+** that puts it in a new last step.
  - **Dropping**: over the middle of a row the task joins that step (the step is outlined and says *together*); near the top or bottom edge of a row a line marks a new step there; on the tray it leaves the order.
  - **Moves show at once**: the rows change before the schedule answers; the dates fade until it does; on an error the rows go back and the error shows.
  - **The estimate is edited in place**: click it (an amber dash when there is none) or press `e`, type `3d` or `4h` as in quick-add, `Enter` saves.
  - **Keys** on a focused row: `j`/`k` (or `↓`/`↑`) the next or previous row; `Alt+↑`/`Alt+↓` move (as spec 60); `t` together with the step above; `Shift+T` a step of its own; `1`–`9` into step *n* (past the last: a new last step); `x` out of the order; `e` estimate; `Enter` open on the right. In the tray: `a` (or `Enter` on **+**) adds it. They are handled by the row, so the app-wide `Alt+↑`/`Alt+↓` (zoom) does not also fire.
- **My team**:
  - A column for everyone in Minimap who isn't archived (you first, then by name; reporting lines don't matter): avatar, name, *5 steps · ends Wed 4 Nov* (or *No order yet · 3 open tasks*), and this week's load in amber when it is over 100%.
  - Each column lists the steps (number, title, finish date, red when after the due date, *started*); tasks of one step are grouped with a left rule. The footer says *+ 4 not in the order*; clicking it or the person's name opens their queue.
  - Dragging a row within a column reorders it (as the queue). Dragging it to **another person's column gives it to them** at that place (a new step or a step of theirs), or into *not in the order* when dropped on their footer. It is one change and one undo step, and a message says what happened. The task gets that one person as its assignee (another assignee it had is removed).
  - Nobody else yet: your column, and a line saying to add people on the People screen.
- Commands: `get_team_orders()` (everyone, you first, each with their order and this week's load as Capacity counts it), `give_task(task_id, person_id, steps)` (reassign and make `steps` the new person's order, one transaction).
- `WorkOrder` gains `ruler`: the working days the bars are drawn on (core: 15 working days from the earlier of today and the first start, not more than 5 working days back).

**Out**
- Pointer-based drag with a custom drag image (the webview's own drag is kept), animation of the rows, conflicts shown on the row they affect (they stay a note above the queue), a team order across people.

## Acceptance criteria
- [x] `core::order::ruler` gives 15 working days, starts no later than today and no earlier than 5 working days back (core tests).
- [x] `give_task` reassigns and orders in one transaction; a refused order leaves the assignee as it was; one undo step (store and command tests).
- [x] `get_team_orders` lists you first, then everyone else by name (archived people left out), each with their order and load (command test, demo snapshot).
- [x] The queue's pure functions: summary text, drop zone from the pointer, together / own step / step *n*, the optimistic regrouping (ui tests).
- [x] Help, design, knowledge graph, ADR-0024 and the changelog follow.
- [ ] Manual check: drag, keys, estimate edit and giving a task to someone else in `cargo tauri dev`.
