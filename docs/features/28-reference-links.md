# 28 — Reference links on tasks

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06

## Goal
Keep the documents and pages behind a task one click away: a Google Doc or Drive folder, a ticket, a spec, any web page.

## Scope
**In** (built)
- **A list of links on a task** (`Task.links: Vec<RefLink{title, url}>`, migration 0010, stored as JSON in `tasks.links`, default `[]`). A title is optional; without one the list shows a short form of the address. Order is the order added.
- **Only `http`, `https` and `mailto` addresses** (`core::links::normalize`): a bare `example.com/page` gets `https://`; spaces, other schemes (`file:`, `javascript:`, `data:`, `ftp:` ...) and paths on the disk are refused with a message. At most 50 links per task, 2000 characters per address, 200 per name; a repeated address is kept once.
- **Task detail pane**: a *Reference links* section (`components/reference_links.rs`) with the list (a Drive/Web/Email tag, the name, the host), an address box, an optional name box and *Add link*; ✕ removes one. Clicking a link calls `open_link`, which checks the address again and opens it with the system (`open::that_detached`); the webview never navigates.
- Edits go through `update_task { links }` (the whole list replaces the old one), so they are one activity row (`links: [old, new]`), undoable and redoable, merged by the Drive sync like any column, and in the full export (`tasks.json`) and the Markdown project files (`[name](url)` after the task's facts).
- **A repeating task's next one keeps the links.**
- Demo data: three tasks carry links (a Google Doc and Sheet, a Drive folder, web pages).

**Out**
- Links on other kinds of item (projects, notes, ...): the field is on tasks only; the same shape (`RefLink`, `core::links`, `components/reference_links.rs`) can be reused.
- Editing a link in place (remove and add again), reordering, fetching page titles or previews (no network from the app for this), picking a file from Google Drive inside the app (the app only has `drive.file` scope, ADR-0011), links in quick-add.

## Acceptance criteria
- [x] Links are stored with the task, tidied and checked on the way in, and refused when unsafe (core tests `web_addresses_*`, `anything_that_could_run_*`, proptest `accepted_addresses_are_safe_and_stable`; store tests in `tests/task_links.rs`).
- [x] Adding and removing is undone and redone exactly (store `undo.rs` `adding_and_removing_reference_links_*`).
- [x] The next task of a repeating series keeps them; the export says them (`export_md` `a_tasks_reference_links_are_markdown_links`).
- [ ] Click-through (see below).

## Decisions
- **A column, not a table.** Like the repeat rule (spec 27): a short list read and written whole, so merge, export, undo and activity come for free; a table of links would need tombstones, merge rules and an export file of its own. The cost is that two devices editing the same task's links at once keep the newest list rather than combining them (the same as any other field).
- **Link, don't attach.** A link keeps the file where it lives (Drive, a wiki) and adds nothing to what is uploaded; attachments (spec 22) copy bytes into the encrypted store. Both are on the task.
- **The backend opens the link.** The webview has no navigation to the outside, and the check (`normalize`) is in one place: the same rules at save and at open, so a link edited in the database file still can't open anything but a web or email address.

## Implementation notes
- Types: `RefLink` (+ `kind()`, `host()`, `label()`), `LinkKind`; `Task.links`, `CreateTask.links`, `UpdateTask.links: Option<Vec<RefLink>>` (plain, not a `Patch`).
- Core `links` (pure): `normalize`, `clean`, limits.
- Store: `tasks` (`COLS`, `from_row`, create/update clean the list, `make_next` copies it), `convert::{links_s, col_links}`; `demo::demo_links`.
- Commands: `open_link` (`commands/links.rs`; manifest + capability `allow-open-link`).
- UI: `components/reference_links.rs`, `api::open_link`; Help: `tasks.md`.

## Not yet verified by hand
- Open a task: paste `docs.google.com/document/d/...` and press Enter: it appears tagged Drive; click it: the browser opens it
- paste `javascript:alert(1)` or `/home/me/a.pdf`: a toast refuses it and nothing is added
- add a named link and an unnamed one, remove one with ✕, Ctrl+Z brings it back
- finish a repeating task: the new one has the links
- the Markdown export lists the links on the task line
