# 16 — This week view

Status: Draft · Milestone: M3 · Priority: Must
Depends on: 06, 08

## Goal
The daily landing page: what needs attention now.

## Scope
**In**
- Sections: overdue, due this week, blocked, my tasks in progress, waiting-ons due or stale, 1:1s this week (if notes are dated).
- Inline actions: complete, reschedule (`+1d`, `next week`), resolve waiting-on.
- Command: `get_this_week(week_start)`.

## Acceptance criteria
- [ ] Each section matches the underlying data for demo data.
- [ ] Actions update the view without reload.

## Open questions
- Is this the default screen on launch, or Overview?
- "This week" = Mon–Sun or next 7 days?
