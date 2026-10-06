# 13 — Schedule and critical path

Status: Implemented — awaiting manual check · Milestone: M2 · Priority: Must
Depends on: 07

## Goal
Compute when work will actually finish and which tasks drive the end date.

## Scope
**In**
- CPM in `minimap-core` per `CLAUDE.md` §5.3: working days Mon–Fri; duration = estimate_days or 1 (flag "unestimated"); forward and backward pass; slack; zero-slack = critical; done tasks fixed at actual dates; `lag_days` honoured; cross-project `blocks` supported.
- Scope: one project, or the whole portfolio.
- Commands: `get_schedule(scope)`, `get_critical_path(scope)`.
- Project detail: timeline (simple Gantt-like bars, SVG from Rust) with critical tasks highlighted, plus a critical-path list.
- Projected project finish vs target date shown on the project.

**Out**
- Holidays, part-time calendars (later).

## Rules
- Tasks without start date start as soon as predecessors allow, from today.
- Backward pass from project target date, or latest finish if none (slack can then be negative → shown as "late by N days").

## Acceptance criteria
- [x] Hand-built graphs in unit tests produce expected ES/EF/LS/LF and critical path. *(diamond with exact numbers and dates, weekends, start dates, lag, done tasks, cancelled tasks, cross-project, targets early and generous)*
- [x] Proptest: slack ≥ 0 when backward pass uses latest finish. *(also: starts are tight so nothing can move earlier, every link and lag honoured, a later target adds exactly the same slack, calendar round-trips)*
- [x] With demo data the critical path matches an `insta` snapshot. *(spec 24: `the_schedule_and_critical_path_of_the_demo_data` in `src-tauri/src/commands/demo.rs` snapshots every project's forecast and the critical path of each project on a fixed Wednesday; the hand-built tests still assert exact values)*

## Decisions
- **Negative slack is allowed** (your answer). Slack is `LS - ES` against the project's deadline: its target date, or, with none, its own projected finish (so slack is never negative without a target, which is the proptest). With an unrealistic target the late tasks show "late by N working days", and the project shows "target …: late by N working days".
- **Timeline offers both**: a Days / Weeks toggle (a column per working day, or five narrow columns per week), plus "Show finished" (done tasks are faint and hidden by default).
- **What "critical" means**: a task is critical when it has the **least slack in its project**. With no target that is exactly zero slack (the spec's rule). With a target it is still the chain that decides the finish, even when the target is generous (positive slack) or unrealistic (negative), instead of the critical path vanishing.
- **Forward pass is global, backward pass is scoped**: earliest dates are always computed over every open task, so a `blocks` link from another project delays a task in every scope. The scope (`Project(id)` or `Portfolio`) only decides which tasks are reported and judged. In project scope successors in other projects are ignored when working out slack; in the portfolio each project is judged against its own deadline (so every project has a critical path).
- **Working-day model**: time is working days from today's working day (a weekend today means Monday). A weekend start date counts as the next Monday; a weekend target counts as the Friday before. Fractional estimates work (`0.5d` finishes the day it starts). No holidays or part-time calendars.
- **No progress tracking**: an in-progress task is scheduled from today with its whole estimate; a task whose start date has passed starts today. Done tasks are fixed at their completion date (finished today means successors start tomorrow) and are never critical.
- **Unestimated** tasks count as 1 day and are flagged (dashed outline, a count on the project). An estimate of 0 is a milestone, not unestimated.
- **Cancelled and archived tasks** are left out, and links through them constrain nothing. A loop in `blocks` (which the UI prevents) is reported as a `cycle` error instead of hanging.
- **Timeline is drawn from Rust data by the UI** (`timeline.rs` layout + Leptos SVG), so bars are clickable and need no HTML injection; the schedule carries the axis dates so the UI does no date arithmetic.

## Implementation notes
- Core `schedule::compute(tasks, edges, projects, today, scope) -> Schedule` (petgraph toposort, forward/backward pass), `critical_path`. Types in `minimap-types::schedule` (`ScheduleScope`, `Schedule`, `ScheduledTask`, `ProjectForecast`). Commands `get_schedule(scope)`, `get_critical_path(scope)` (the UI reads the critical tasks out of `get_schedule`).
- UI: `Schedule` section in the project panel (`components/schedule_panel.rs`): forecast line, critical-path list (click opens the task), Days/Weeks, timeline (today line, target line, slack lines, late bars in the danger colour), hover text per task. Layout and wording are pure and tested in `timeline.rs`. Styles: `.gantt` in `input.css` (theme tokens only).
- Not in this spec: a portfolio timeline screen (the command supports it; the overview and graph view, 15 and 18, will use it).

## Not yet verified by hand
- a project with a few tasks, estimates and `blocks` links: Schedule shows a projected finish, a critical path and bars; clicking a bar or a list row opens the task
- Days / Weeks toggle redraws (weeks are narrower); "Show finished" adds faint bars for done tasks
- set the project target earlier than the projected finish: the line turns red with "late by N working days" and late bars go red; set it far later: "on track (N working days to spare)", the critical path is still highlighted
- today and target lines are drawn; weekends are not on the axis
- tasks without an estimate have dashed outlines and are counted on the line; a task with a start date in the future starts then
- add a `blocks` link from a task in another project: this project's task starts after it
- scrolling the timeline sideways keeps the task names on the left
