# 04 — Objectives

Status: Draft · Milestone: M1 · Priority: Must
Depends on: 01, 02

## Goal
Top-level outcomes the portfolio serves. Projects and tasks contribute to them.

## Scope
**In**
- Fields: title, description, target_date, status (`on_track`, `at_risk`, `off_track`, `done`), priority 1–5.
- List (sorted by priority, then target date) and detail.
- Detail shows contributing projects/tasks (`contributes_to`, with weight) and their status.
- Commands: `create_objective`, `update_objective`, `archive_objective`, `get_objective`, `list_objectives`.

**Out**
- Computed health (15). Until then, status is manual.

## Rules
- Once computed health exists (15), show computed health next to the manual status; a mismatch is highlighted, not overwritten.

## Acceptance criteria
- [ ] CRUD works from list and detail pane.
- [ ] Contributing projects can be linked with a weight 0–1.

## Open questions
- Keep manual status at all once health is computed?
- Quarterly grouping (e.g. Q1 2027) needed?
