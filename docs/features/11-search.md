# 11 — Search

Status: Draft · Milestone: M1 · Priority: Must
Depends on: 03–06

## Goal
Find any node by text in under 100 ms.

## Scope
**In**
- SQLite FTS5 index over titles, descriptions, names and note bodies (kept in sync via triggers or repository writes).
- Command: `search(query, types?, limit)` → ranked `NodeRef` + title + snippet.
- Search box in the sidebar (`/` to focus); results open the detail pane.
- Used by the edge picker (07), `@` picker (09) and command palette (12).

## Rules
- Prefix matching (`fix log` finds "Fix login timeout").
- Archived nodes excluded unless toggled.

## Acceptance criteria
- [ ] With demo data, typing returns results in < 100 ms.
- [ ] Note body text is searchable.

## Open questions
- Fuzzy (typo-tolerant) matching needed, or prefix is enough?
