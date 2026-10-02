# 25 — Undo

Status: Draft · Milestone: M4 · Priority: Should
Depends on: 01

## Goal
Make mistakes cheap: undo the last action(s).

## Scope
**In**
- `Ctrl/Cmd+Z` undoes the last write; toast "Undone: archived task X".
- Implemented from `activity` diffs: apply the inverse (restore old field values, unarchive, remove added edge). The undo itself is logged as an action.
- Undo stack per session, last 20 actions.
- Command: `undo_last()`.

## Acceptance criteria
- [ ] Undo works for create, update, archive, edge add and edge remove.
- [ ] Undo after app restart does nothing (session scoped) — or decide otherwise.

## Open questions
- Redo needed?
- Group multi-row actions (bulk create, archive with edges) as one undo step — needs a `batch_id` on activity.
