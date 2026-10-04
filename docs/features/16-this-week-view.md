# 16 — This week view

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Must
Depends on: 06, 08

## Goal
The daily landing page: what needs attention now.

## Scope
**In**
- Sections: overdue, due this week, blocked, my tasks in progress, waiting-ons due or stale, 1:1s this week (if notes are dated).
- Inline actions: complete, reschedule (`+1d`, `next week`), resolve waiting-on.
- Command: `get_this_week(week_start)`.

## Acceptance criteria
- [x] Each section matches the underlying data for demo data. *(a property test checks every section is exactly a filter of the tasks: nothing closed, dates inside their bounds, nothing missing, the day strip adds up; plus hand-built cases per section. Demo data is spec 24.)*
- [x] Actions update the view without reload. *(every action goes through the data-version refresh, like the other screens; unverified by hand)*

## Decisions
- **This week is the default screen on launch** (your answer): it is the route `/` and the first sidebar entry (`g w`). **Overview moved to `/overview`** (`g o`). The palette's "Go to This week" and "Go to Overview" follow the nav table.
- **"This week" = Monday to Sunday** (your answer). `get_this_week(week_start)` takes any date and snaps to that week's Monday (today's week when omitted), so a Sunday belongs to the week that ends on it. The screen has previous / next week buttons and a "This week" button; the command returns the neighbouring Mondays so the UI does no date arithmetic.
- **Sections** (a task can appear in more than one, e.g. an overdue task that is also blocked, because each section answers a different question):
  - **Overdue**: open tasks due before today, whatever week is shown (it is about now), oldest first, with "N days overdue".
  - **Due this week**: open tasks due from today through Sunday (the whole week when you look at a future week), by date then priority. Earlier days of the current week are overdue, not repeated here.
  - **Blocked**: open tasks with status Blocked, by priority, each with the open tasks that block it ("blocked by …").
  - **My tasks in progress**: In-progress tasks assigned to me. With no "me" person it shows everyone's and says so.
  - **Waiting on: stale or due**: open, not snoozed, and stale (age setting or past its expected date) or expected by Sunday; oldest first. Snoozed ones stay hidden until they resurface.
  - **1:1s this week**: notes of kind 1:1 dated Monday to Sunday (earliest first), which also covers a planned 1:1 note dated later in the week.
  - A **week strip** (seven day tiles, today highlighted) shows what is due, expected and scheduled each day.
- **Inline actions**: complete (the round check, or `x` on the row under the cursor), reschedule (**Tomorrow**, **Next week** = next Monday, or type a date: `fri`, `next-wed`, `+3d`, `2027-03-31`, Enter), resolve or snooze 3 days for waiting-ons. Rescheduling uses the quick-add date rules, which gained **`next-week`** (also typed `next week`), the Monday of the next calendar week. `reschedule_task(id, when)` is a new command; a refused date changes nothing.
- Rows use the shared list rows, so `j`/`k`/`Enter` walk the whole screen and the detail pane opens as usual.

## Implementation notes
- Core `this_week::build(input)` (pure; reuses the waiting-on and note list rules), `monday_of`. Types in `minimap-types::week` (`ThisWeek`, `WeekTask`, `WeekDay`). Store view `open_blockers`. Commands `get_this_week(week_start)`, `reschedule_task(id, when)`.
- UI: `pages/this_week.rs` (header with week navigation, day strip, sections, row actions).

## Not yet verified by hand
- the app opens on This week; the sidebar shows This week first and Overview second; `g w` / `g o` jump to them
- an overdue task shows "N days overdue" in the Overdue section; a task due Friday is under Due this week; a blocked task lists its blockers
- the round check completes a task and it leaves the list at once; `x` does the same on the row under the cursor
- Tomorrow / Next week / a typed `+3d` change the due date and the row moves to the right section; a nonsense date shows an error and changes nothing
- a stale waiting-on can be resolved or snoozed in place; a 1:1 note dated this week is listed
- ‹ / › move by weeks, "This week" returns; a future week lists what is due then
