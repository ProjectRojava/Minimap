# 10 — Decisions

Status: Draft · Milestone: M3 · Priority: Should
Depends on: 07

## Goal
A lightweight decision log tied to the work it affects.

## Scope
**In**
- Fields: title, context, decision, rationale, decided_on, status (`proposed`, `decided`, `superseded`).
- `affects` edges to projects, tasks or objectives; optional "supersedes" link to an earlier decision.
- List (newest first, filter by status) and detail; shown on affected nodes.
- Commands: `create_decision`, `update_decision`, `archive_decision`, `get_decision`, `list_decisions`.

## Acceptance criteria
- [ ] A decision linked to a project appears on that project's detail.
- [ ] Decisions made this week appear in the weekly review (19).

## Open questions
- Could this be a note with `kind = decision` instead of its own type?
- Superseding: new edge type, or `relates_to` with a note?
