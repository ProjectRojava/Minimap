# 09 — Notes and @mentions

Status: Draft · Milestone: M3 · Priority: Must
Depends on: 03, 07

## Goal
Markdown notes (especially 1:1s) linked to the people and work they mention.

## Scope
**In**
- Fields: title, body (Markdown), note_date, kind (`one_on_one`, `meeting`, `general`).
- Editor: plain textarea with Markdown preview toggle (Rust-side rendering, e.g. `pulldown-cmark`, sanitized).
- Typing `@` opens a picker for people, projects and tasks; inserting creates a `mentions` edge. Removing the mention text removes the edge on save.
- 1:1 view on a person: their notes in reverse date order; "New 1:1" button pre-fills title, date, kind and mention.
- `[ ]` lines: a "Convert to task" action per line creates a task (assigned to the mentioned person if any) and replaces the line with a link.
- Commands: `create_note`, `update_note`, `archive_note`, `get_note`, `list_notes` (filters: kind, person, date range, text).

## Rules
- Note bodies are never logged at info level.
- Autosave while typing (debounced); one activity row per save session, not per keystroke.

## Acceptance criteria
- [ ] `@priya` in a note creates a mention; the note shows on Priya's page.
- [ ] Converting `[ ] Send budget` creates a task and links it.

## Open questions
- Store mentions in the body as `@[Name](node:id)` or plain `@name`?
- Is `[ ]`→task conversion MVP or later?
