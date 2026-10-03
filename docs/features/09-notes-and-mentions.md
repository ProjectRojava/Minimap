# 09 — Notes and @mentions

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Must
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
- [x] `@priya` in a note creates a mention; the note shows on Priya's page. *(store test: the `mentions` link appears on both ends; Priya's panel lists the note under Notes; the picker itself is unverified by hand)*
- [x] Converting `[ ] Send budget` creates a task and links it. *(store tests; the button is unverified by hand)*

## Decisions
- **Mentions are stored in the body as `@[Name](node:<id>)`** (not plain `@name`), so they survive renames and ambiguous names. The label is only a cache: the preview and lists show the node's current name, and a mention of a node that has been archived or deleted shows as plain muted text. The node's type is looked up from its id.
- **`[ ]` → task conversion is in the MVP.** A line like `[ ] Send budget to @[Priya](node:…)` (with or without a `-`/`*` bullet) can be converted: the task is assigned to the first *person* mentioned on the line (otherwise to me, the default), filed in the first *project* mentioned, and the line becomes `- [x] @[Send budget to Priya](node:<task>)`, which also links the note to the task. A converted line is no longer offered.
- **One activity row per editing session**: saves within 5 minutes of the previous `updated` row for the note fold into it (earliest "old", latest "new" per field; a change that nets out to nothing removes the row). A link change in between (a new mention) starts a fresh row.
- **Bodies stay out of the history**: activity records a body's size ("1,204 chars"), never its text, and nothing logs it.
- The preview is rendered by the backend and is **safe**: raw HTML is shown as text, links don't navigate (the address is in a tooltip), images show their alt text only (nothing is fetched). Mentions in the preview open the node.
- A new note is created straight away as "Untitled note" and opened for writing, so autosave and mentions always have a note to attach to.

## Implementation notes
- **Types**: `mention_token`, `NoteFilter`, `NoteItem`, `NoteRow`, `MentionRef`, `ChecklistItem`, `NoteDetail`.
- **Core** `minimap-core::notes` (uses `pulldown-cmark`): `parse_mentions`, `mention_ids`, `checklist`, `convert_line`, `excerpt`, `arrange`, `render`; property tests (parsing never panics, tokens round-trip, rendering never emits `<script`).
- **Store**: `nodes::find`; `notes::create/update` sync the note's `mentions` edges in the same transaction; `activity::record_update_merged`; `notes::convert_checklist_item(note, line, expected_text)` (one transaction; refuses a line whose text changed since the caller looked, so a stale checklist can't convert the wrong line); `views::{note_items, note_item}`. No migration.
- **Commands**: `list_notes(filter_by)`, `get_note`, `get_note_detail`, `create_note`, `update_note`, `archive_note`, `render_markdown`, `convert_checklist_item(note_id, line, text)`.
- **UI**: Notes screen (newest first; filters for text, kind, who is mentioned and a date range; excerpt and mentioned names per row); note panel: title, date, kind, Write/Preview, a monospace textarea that autosaves 800 ms after the last keystroke and on blur ("Saving… / Saved"), the `@` picker (people, projects, tasks; arrows, Enter or Tab to insert, Esc to close), the checklist with "Convert to task", the mentions list and archive. A person's panel lists the notes that mention them, with **New 1:1** (a note titled "1:1 with <name>", kind 1:1, already mentioning them). The generic Links list leaves a note's mentions to its text.
- Rendered-Markdown styles live in `ui/style/input.css` (`.md`), using the theme tokens.

## Not yet verified by hand
- New note (`n` on the Notes screen): type, wait a second: "Saved"; close and reopen: the text is there
- type `@pri`: the picker lists matching people, projects and tasks; ArrowDown + Enter inserts `@[Priya](node:…)` at the caret and the caret ends up after it; Esc closes only the picker
- the note shows under "Mentions"; Priya's panel lists it under Notes; deleting the mention text removes it
- Preview: headings, lists, code render; `<script>` shows as text; a mention shows `@Priya` and opens her panel when clicked; rename Priya and the preview shows the new name
- `[ ] Send budget to @Priya` lists under Checklist; Convert to task creates it (assigned to Priya), the line turns into `- [x] @[…]` in the editor, and the task appears in Tasks
- editing above a checklist item and clicking Convert before the autosave catches up shows an error rather than converting the wrong line
- Priya → New 1:1 opens a note already mentioning her
- the note's activity shows one "Updated" entry with sizes, not text
