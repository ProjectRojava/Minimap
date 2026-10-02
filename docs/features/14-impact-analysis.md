# 14 — Impact analysis ("What if this slips?")

Status: Draft · Milestone: M2 · Priority: Must
Depends on: 13

## Goal
Show the downstream effect of a slip before it happens. The product's headline feature.

## Scope
**In**
- Core: input node (task or project) + N working days; propagate along `blocks` and `depends_on`, consuming slack per step.
- Output: affected tasks (old/new finish, slip absorbed, remaining slip), affected projects (new projected finish vs target), objectives (via `contributes_to`), people (via `assigned_to`).
- Command: `run_impact_analysis(node_id, slip_days)`.
- Screen: pick node + slip days; results grouped by tasks / projects / objectives / people with before→after dates; "late vs target" highlighted.
- "What if this slips?" button on every task and project detail.
- Read-only: nothing is saved.

## Acceptance criteria
- [ ] Demo data: a 5-day slip on a chosen task matches an `insta` snapshot.
- [ ] Proptest: impact never moves any task earlier; absorbed + passed-on = slip at every step.

## Open questions
- Allow multiple simultaneous slips in one scenario?
- Offer "apply this slip" (update dates) or keep it purely what-if?
