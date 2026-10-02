# 17 — Capacity

Status: Draft · Milestone: M3 · Priority: Should
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
- [ ] Demo data's overloaded person shows > 100% in the right weeks.
- [ ] Unit tests with hand-built schedules.

## Open questions
- Ship the simple task-count flag first and the heatmap later?
