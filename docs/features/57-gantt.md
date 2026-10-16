# 57 — Gantt: the plan on a calendar

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 05, 06, 13, 14, 17, 25, 30, 33, 44, 46, 50
ADR: [ADR-0022](../decisions/ADR-0022-the-gantt-writes-dates.md)

## Goal
One screen that draws the whole plan as bars on a calendar, **objectives → projects → tasks**, expandable at each level; shows **which task waits for which**; colours each bar by **where its task stands** (to do, in progress, blocked, done); and lets you **change dates by dragging**: the middle of a bar moves a task, its left or right edge changes its start or end, and the bars of projects and objectives give their tasks **room to grow into**.

## Scope
**In**
- **Screen `Gantt`** (sidebar *Plan*, `g j`, route `/gantt`, icon `gantt`). A tree of rows on the left (objectives, their projects, their tasks, a task's sub-tasks one step in) and the chart on the right with a working-day axis, today marked, the same Days / Weeks scale as the project panel's timeline. Objectives start open, projects folded; *All tasks* / *Projects only*, `→` `←` `*`. *Show finished* adds finished tasks and projects. My view (spec 50) narrows it to what touches you. A project that serves several objectives is drawn once, under the one with the soonest target, with a *+N* chip and a tooltip naming the others; projects with no objective sit under *No objective*.
- **Bars.** Task: from its earliest start to its finish (`schedule::compute`, spec 13). Status is the main channel: to do hollow grey, in progress accent, **blocked amber stripes** (not colour alone), done faded; critical = accent underline and bold name; late = a red stretch past the latest finish; slack = dashed line; due date = a tick (red when overdue); unestimated = dashed outline. Project: a thin summary bar for the open work, the **room** between its end and the target flag shaded by the safe zone's level (spec 44), the overrun red, the start date as a mark. Objective: the same with its target flag; an ongoing objective (ADR-0014) fades out to the right and has no flag to drag.
- **Arrows.** `blocks` links between drawn tasks as elbow arrows (end of the blocker to start of the blocked, lag in the tooltip), `depends_on` between projects dashed, and a link to a task inside a folded project goes to the project's summary bar (dashed; several links between the same two folded rows are one line). **Focus** (default): every arrow quietly, and pointing at or opening a task lights the whole chain through it (everything before and after, transitive) while the rest fades; **All**; **Critical** (the consecutive tasks of each project's chain, `chain_of`); **Off**.
- **Dragging a task bar** (core `gantt::plan_drag`, commands `preview_task_drag` / `apply_task_drag`): the **middle** moves `start_date` and shifts `due_date` by the same working days (the estimate stays); the **left edge** sets `start_date` and the estimate (the end stays); the **right edge** sets `due_date` and the estimate (the start stays). Bars snap to working days; a floating label says where it lands; the bars it moves along move on screen (the backend's preview); `Esc` cancels; a press under 4 px is a click that opens the item.
- **Rules of a drop.** A task can't start before **what blocks it is done**, before **its project starts**, or before **today** (the bar stops at the limit, a dashed line and the label say why; the backend refuses the same drops in words). Finished work, and anything off the plan, can't be dragged. **What follows is pushed in its due dates only**, by the working days it really moved (slack absorbs the rest), never earlier and never touching start dates; tasks without a due date are not given one; project targets are never changed by a task drag. The whole drop (the task and every pushed due date) is **one transaction and one undo step**. A toast says what moved and how many due dates were pushed; the pushed tasks wear a red *+2d* until the next drag. Weekly review counts them as slipped (it reads due dates moved later).
- **Dragging project and objective bars gives room, it does not move tasks.** The **flag** sets the project's or objective's `target_date` (later: more room and more slack; earlier: less, with the overrun red); the project's **start mark** sets `start_date`; the middle of a project's summary bar moves both (needs both dates). Each is one `update_project` / `update_objective`, one undo step.
- **Project start date becomes a rule** (the schedule change): **no open task starts before its project's `start_date`** (a past date changes nothing, like a past task start). It holds for every screen that reads the schedule (forecast, health, capacity, impact, What if). `ScheduledTask` gains `earliest_start` (the first day it could start whatever its own start date says: today, the project start or when its blockers are done) and `held_by` (the task that sets it), which the drag limit uses.
- **Keyboard.** `↑` `↓` / `j` `k` move between rows (and open them in the pane), `→` `←` open / fold, `*` open all, `Alt+←/→` move the open task one working day, `Alt+Shift+←/→` move only its end.
- **Help** page *Gantt*, keyboard and welcome pages, Schedule (the project-start rule), Projects, Objectives, Tasks, What if, Dependencies; changelog; ADR-0022 with amendments to ADR-0017 and ADR-0018.

**Out**
- Dragging a whole project bar to move every task in it (the middle drag moves the project's own dates, and tasks follow only as the start rule holds them back).
- Dragging the end of a project's summary bar to change its tasks.
- Pushing downstream **start** dates, or project targets, automatically; the push is due dates only (see the ADR).
- Waiting-on badges on bars (spec 56), pings and fog marks on bars, baselines (planned against now; needs stored baselines and a migration).
- Meetings (not planned work, ADR-0018) and tasks with no project (the Inbox).
- Creating a task or a link by dragging on the chart.

## Acceptance criteria
- [x] `core::gantt::build`: objectives in drawing order (dated soonest first, undated, ongoing last, then *No objective*); a project once, under its first objective, naming the others; sub-tasks right after their parent; links only between drawn rows; the axis covers every bar, pinned start, target, today and 25 days of room; My view keeps only what is in scope; an objective's overrun from its projects (`core/src/gantt.rs`).
- [x] `plan_drag`: move shifts start and due together; the edges set start/due and the estimate; a blocked, project-started or past-day drop is refused with its reason; finished and unplanned tasks are refused; the pushes follow by what each task really moved, are never earlier, never touch start dates and skip tasks without a due date; a proptest that no push is earlier and a move lands on the day asked for.
- [x] Schedule: no task starts before its project; `earliest_start` ignores the task's own start date and `held_by` names what holds it, open or finished (`core/src/schedule.rs`).
- [x] Commands: a drop writes the task and the pushed due dates in one transaction (one activity row each), a refused drop writes nothing, a drop where the bar already is writes nothing, the resize edges write the estimate; the screen lists *No objective* and My view without a "me" is empty (`commands/gantt.rs`).
- [x] Demo data: a snapshot of the Gantt of the seed; EU Region is the project 2 working days over its target, the story holds (`commands/demo.rs`, `snapshots/..the_gantt_of_the_demo_data.snap`).
- [x] UI layout (pure): rows and folding, finished work hidden, sub-task depth, bar classes, snapping, the candidate of each edge (held at the earliest start, never crossing), band marks and the room, band dates, arrow tones and elbows, the chain set, wording (`ui/src/gantt.rs`).
- [x] Help: every page true to the code (the keyboard and welcome tests, the link tests); `Gantt` page.
- [x] ADR check done: new ADR-0022; amendments to ADR-0017 and ADR-0018.
- [ ] Click-through (see below).

## Decisions (the open questions, answered by building as proposed)
- **A project under two objectives** is drawn once (like the radar, ADR-0021), under the first by drawing order, with a *+N* chip.
- **Arrows**: *Focus* is the default (everything quietly, the chain lit): the screen answers "which task relates to which" on opening, and still reads on a busy plan.
- **Name**: *Gantt* (the project panel's chart stays *Schedule*, so the two don't share a word).
- **Pushed due dates** are shown by a transient red chip, not stored: *Weekly review* already reads moved due dates from the activity log, so the record is there.
- **Keyboard shift** uses `Alt+←/→` (checked against the global shortcuts: only `Alt+↑/↓` zoom, spec 52).

## Not yet verified by hand
- Open `g j` on the demo data: three objectives, EU Region *2d over* in red, the ongoing objective fading out. *All tasks* opens every project; the arrows are quiet and point at a task to light its chain.
- Drag *EU pricing* two days right: the bar follows, a label says *Fri 5 Mar → Fri 5 Mar · 1 day*, the dashed ghost stays; let go: a toast, the bar lands, `Ctrl/Cmd+Z` puts it back.
- Drag a blocked task left past its blocker: it stops, the dashed limit and *can't start before … is done* show.
- Drag a task's right edge two days: its due date and estimate change in the pane, the tasks it blocks have later due dates (red *+2d*), and Weekly review lists them as slipped.
- Drag a project's flag later: the room grows; earlier than the work ends: the overrun is red. Drag its start mark later than a task's start: the task moves.
- `Alt+→` on the open task moves it a day; `Esc` cancels a drag; Weeks scale offers no edge handles.
