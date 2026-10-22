# 60 — Order of work: each person's tasks in the order they will be done

Status: Done — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 13, 17, 57

## Goal
The schedule lets every task start as soon as its blockers and dates allow, so one person's ten tasks all start today and the Gantt draws them side by side. People do one thing after another. Let the user say, per person, **what comes first**: tasks stacked vertically are done one after the other; tasks side by side in the same row are done at the same time. The schedule follows that order, so the Gantt, the forecast, health, impact and capacity all agree.

## Scope
**In**
- A new screen **Order** (sidebar, under People; `g q`). Pick a person (you by default). Their open, assigned tasks are shown as **steps**, top to bottom: step 1 is done first. Cards in the same step sit side by side and run at the same time. A step starts when the whole step above it has finished.
- **Not in the order yet**: the person's other open tasks, in a tray under the steps (due date first, then priority, then title). They keep today's behaviour: as early as their blockers and dates allow.
- Moving cards: drag a card onto a step (it joins it), onto the gap between two steps or the space above the first / below the last (a new step there), or onto the tray (out of the order). On a card: ↑ (to the step above, or a new first step), ↓ (to the step below, or a new last step), ✕ (out of the order). In the tray: *Add* puts a task in a new last step. Keyboard: `Alt+↑` / `Alt+↓` on a focused card. Every change is one undo step.
- Each card says its project, estimate, and the dates the schedule now gives it (*Mon 3 Mar → Fri 7 Mar*), and warns when that finish is after the due date. Each step says its dates, and *2 tasks at once, 150%* when the allocations side by side add up to more than 100%. A task already **in progress** is marked *started*: it keeps its own start, and the order only holds up the steps below it.
- **The start dates are worked out, not written.** The order is a rule of the schedule (CLAUDE.md 5.3): a task in step *n* starts no earlier than the end of the latest task in the person's step *n − 1*. A start date you typed still wins when it is later. The task's own `start_date` is never changed by the order.
- **Stored on the task**: `tasks.order_step` (migration 0020), the task's step in its assignee's order; empty = not in an order. A reorder renumbers that person's tasks 1..n in one transaction (one undo step; the field is an ordinary patch field, so undo, sync, export and merge need nothing new). Changing a task's assignee, finishing it or cancelling it takes it out of the order.
- **Conflicts**: an order that would contradict `blocks` (a task placed above one that must be done before it) is refused with the names (*"Design" must be done before "Build", so "Build" can't come first*). A blocks link added later that contradicts an order wins: the schedule drops that part of the order, and the Order screen says which.
- Left out of an order's effect: meetings (they are not planned work), tasks on the backburner (set aside), finished and cancelled tasks, tasks with no assignee.
- Commands `get_order(person_id?)` (no id = you) and `set_order(person_id, steps)`.

**Out**
- Tracks (two parallel sequences inside one person's order), work-in-progress limits, automatic levelling (*Suggest order*), writing the dates into `start_date` (*Lock in dates*), meetings and part-time capacity stretching durations. Possible later.
- An order for a team or across people: each person has their own.

## Acceptance criteria
- [x] A task in step 2 starts when the latest task of step 1 finishes; tasks in the same step start together (core `schedule` tests + proptest).
- [x] A typed start date later than the order's still wins; an in-progress task is not moved by the order; meetings, parked, finished and unassigned tasks are ignored (core tests).
- [x] An order contradicting `blocks` is refused with names; a later contradicting `blocks` link drops that part of the order instead of breaking the schedule (core + store tests).
- [x] `set_order` renumbers in one transaction, one undo step; reassigning, finishing or cancelling clears the step (store tests).
- [x] The Order screen shows steps, the tray, dates and warnings; drag, ↑/↓/✕, *Add* and `Alt+↑/↓` work (ui pure-function tests; manual check pending).
- [x] The Gantt, forecast and health follow (they read the same schedule; demo snapshot).
- [x] Demo data gives one person an order; the story still holds.
