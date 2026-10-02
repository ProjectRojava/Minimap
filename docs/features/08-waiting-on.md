# 08 — Waiting-on

Status: Draft · Milestone: M3 · Priority: Must
Depends on: 03, 07

## Goal
Track what other people owe the user, so nothing silently stalls.

## Scope
**In**
- Fields: description, person_id, asked_on (default today), expected_by, resolved_on.
- Optional `about` edge to a task or project.
- List sorted by age (oldest first); stale (> 7 days, or past expected_by) highlighted.
- One-click resolve; reopen.
- **Snooze / follow-up date**: hide until a date, then resurface.
- Shown on the person detail and on the linked task/project.
- Commands: `create_waiting_on`, `update_waiting_on`, `resolve_waiting_on`, `archive_waiting_on`, `get_waiting_on(open_only)`.

## Acceptance criteria
- [ ] Create, resolve and reopen a waiting-on.
- [ ] Stale items are highlighted and appear on Overview/This week.
- [ ] Snoozed items are hidden until their date.

## Open questions
- Stale threshold: fixed 7 days or a setting?
- Need a `follow_up_on` column, or reuse `expected_by`?
