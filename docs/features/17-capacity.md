# 17 — Capacity

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Should
Depends on: 13

## Goal
Spot overloaded people before they become the bottleneck.

## Scope
**In**
- Core: per person per week, Σ(allocation_pct × scheduled hours of active assigned tasks in that week) ÷ weekly_capacity_hours. Flag > 100%.
- Heatmap: people × weeks, colour by load; click a cell to see contributing tasks.
- Command: `get_capacity(from, to)`.
- **Fallback for MVP**: if allocation data is missing, use active-task count per person with a configurable threshold.

## Acceptance criteria
- [x] Demo data's overloaded person shows > 100% in the right weeks. *(hand-built plans in the core tests: a person with two parallel tasks is 140% in the right week and 20% the next, a spanning task splits across weeks, etc. Demo data itself is spec 24.)*
- [x] Unit tests with hand-built schedules. *(plus a property test that the weekly figures add up to the work assigned)*

## Decisions
- **Ship both** (your answer): the heatmap and the open-task-count flag land together, and both feed the Overview.
- **Load per person per week**: for each Monday-to-Sunday week, the working days of each open assigned task that fall in the week (from the schedule, spec 13), times the assignment's `allocation_pct` (default 100), summed, divided by capacity. Capacity is `weekly_capacity_hours / hours per working day` days (40h at 8h = 5 days; 20h = 2.5). Over 100% is overloaded; exactly 100% is full, not over. Because each task is sliced by week, the weekly figures add up to `duration x allocation` over the whole task (property test). Done, cancelled and archived tasks and archived people don't count; past weeks are empty because the schedule starts today.
- **The task-count flag** is separate from the load and does not depend on estimates: a person with **more open tasks than a limit** (Settings → Capacity, default 10, 1-500) is flagged "many tasks". It also covers the case where estimates are missing and the load is only a guess: tasks without an estimate count as one day, are marked "no estimate" in the cell, and are counted per person.
- **Heatmap**: people x weeks, a cell per week coloured by band (under 50% light, 50-85% good, 85-100% full, over 100% over, over 125% heavy) with the percentage always printed. Overloaded people come first, then by name. Click a cell for the tasks behind it (days that week, allocation, the task's dates, project) and click a task or person to open it. Window: 4 / 8 / 12 weeks, earlier / later, "This week".
- **`get_capacity(from, to, weeks)`**: `from` and `to` may be any dates in the first and last week; without `to`, `weeks` weeks (default 8, at most 52) are shown; a backwards range is one week.
- **The Overview's "Overloaded this week" now comes from this calculation** (this week and next, plus the task limit), replacing the earlier "next five working days" estimate, so the two screens always agree. The Overview row says the percentage and the week, or that the person is over the open-task limit.

## Implementation notes
- Core `capacity::compute(input) -> Capacity` and `overlap_days`; types in `minimap-types::capacity`; `Settings.capacity_task_limit`; command `get_capacity`. UI `pages/capacity.rs` (route `/capacity`, sidebar People group, `g c`); pure band, label and wording helpers tested.

## Not yet verified by hand
- Capacity opens from the sidebar (or `g c`); the heatmap lists everyone, overloaded people first, the current week's column header in the accent colour
- give someone two parallel 4-day tasks: this week turns amber/red with a percentage over 100; click the cell: both tasks are listed with their days; clicking one opens it
- an 8-day task starting Monday shows 100% then 60%; a 50% allocation (Links on the task) halves the load; a 20h person is loaded twice as heavily
- assign someone more than 10 open tasks: "many tasks" appears next to them; lowering the limit in Settings flags more people
- tasks without estimates say "no estimate" and the person row mentions them
- the Overview's "Overloaded this week" lists the same people with the same percentages
