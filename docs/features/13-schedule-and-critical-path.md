# 13 — Schedule and critical path

Status: Draft · Milestone: M2 · Priority: Must
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
- [ ] Hand-built graphs in unit tests produce expected ES/EF/LS/LF and critical path.
- [ ] Proptest: slack ≥ 0 when backward pass uses latest finish.
- [ ] With demo data the critical path matches an `insta` snapshot.

## Open questions
- Allow negative slack when target date is unrealistic (spec says slack ≥ 0; target-date mode breaks that)?
- Timeline granularity: days or weeks?
