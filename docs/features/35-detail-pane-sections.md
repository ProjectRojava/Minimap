# 35 — Detail pane sections

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 02

## Goal
Make the sections of the right-hand pane clearly separate and easy to scan, without noise.

## Scope
**In** (built; `Section` in `components/detail_pane.rs`, so every panel gets it)
- **Header strip** per section: `bg-hover` band between two hairlines, a chevron, the tone dot, the title (11px semibold uppercase, `fg`), a quiet count or note (`meta`), and the section's own add buttons at the right (`actions`, style `SECTION_ACTION`).
- **Folding**: click the header (a real button with `aria-expanded`). State is remembered per heading in `localStorage` (`minimap.section.<title>`), shared by every panel; a missing or blocked store falls back to the default. The content stays mounted while folded, so drafts survive. `always_open` (Fields, Note, Decision, Status) never folds; `collapsed` sets the first-time default: *Activity* and every *Archive* start folded.
- **Buttons moved to headers**: Part of (*New sub-task…*, *Add existing…*, *Make it part of…*), Links (*New linked task…*, *Link an existing task…*; *Link to something else…* stays in the body with its picker), Attachments (*Attach file…*).
- **Counts**: Links, Part of (done of total), Attachments, Notes and findings, Activity.
- **Quieter empty states**: Part of says "None yet."
- **Field groups**: thin dividers (`FIELD_GROUP`) inside the Fields of a task (title and description / status, priority, type, project, assignee / dates, repeat), a project (… / dates / status, priority, owner) and an objective.

**Out**
- Fields as a label-left property list (needs label-left variants of the select, date and repeat controls); tabs; a section index; per-item (rather than per-heading) fold memory.

## Acceptance criteria
- [x] `parse_open` follows a stored `"1"`/`"0"` and the default otherwise (`ui::detail_pane` test).
- [x] The page builds, clippy is clean, Help describes it (`docs/help/concepts.md`).
- [ ] Click-through (see below).

## Not yet verified by hand
- Open a task: each section has a strip; click *Links* to fold it, reopen the pane (still folded), open a project (also folded)
- *Activity* and *Archive* start folded; the fields never fold
- *New sub-task…* and *Link an existing task…* in the headers open the dialog
- A narrow window (overlay pane): headers wrap their buttons onto a second line instead of overflowing
