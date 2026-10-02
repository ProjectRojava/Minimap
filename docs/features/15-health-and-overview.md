# 15 — Health scoring and Overview

Status: Draft · Milestone: M3 · Priority: Must
Depends on: 13

## Goal
The "minimap": one screen that tells an executive what's at risk and why, without them asking.

## Scope
**In**
- Project health in core from: projected finish vs target, share of tasks blocked or overdue, share of unestimated work. Output: level (`green`, `amber`, `red`), score, and reasons ("3 tasks overdue; projected 6 days late").
- Objective health: weighted roll-up of contributing projects (`contributes_to.weight`).
- Overview screen: objectives with health; projects nested under them; top-5 risks; overloaded people (from 17, or task-count flag if 17 isn't built); stale waiting-ons.
- Command: `get_portfolio_overview()`.

## Acceptance criteria
- [ ] Demo data's at-risk project shows amber/red with correct reasons.
- [ ] Clicking any item opens its detail pane.
- [ ] Overview loads in < 200 ms with demo data.

## Open questions
- Thresholds for amber/red: fixed or configurable?
- Top-5 risks ranking: by days late × priority?
