# 10 — Decisions

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Should
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
- [x] A decision linked to a project appears on that project's detail. *(Links section, "Decisions"; unverified by hand)*
- [ ] Decisions made this week appear in the weekly review (19). *(the date-range filter is ready and tested in core; the review screen is spec 19)*

## Decisions
- **Decisions are their own node type**, not a note kind: they have structured fields, a status and `affects` links, and the weekly review lists them separately.
- **Superseding is a new edge type, `supersedes`** (newer -> older, no attributes, acyclic), see ADR-0006. Adding the link marks the older decision `superseded` in the same transaction; removing it leaves the status for you to change.
- **Deciding stamps a date**: moving a decision to `decided` (or creating it as decided) with no date sets `decided_on` to today (UTC). An explicit date is kept; you can clear or change it afterwards.
- **"Newest first"** sorts by the decision date, falling back to when the decision was written down (so undated proposals sit by their creation day).
- **Created at once**: "New decision" creates "Untitled decision" (proposed) and opens it, like notes, so links can be added straight away.
- **Shown on affected nodes** through the generic Links section: on a project, task or objective the incoming `affects` links are listed under **Decisions** (click to open). No separate section.
- **Weekly review**: `list_decisions` already accepts a decision-date range (`date_from`/`date_to`, inclusive, undated left out), which spec 19 will use for "decided this week".

## Implementation notes
- Core `decisions::arrange` (filter by status, affected node, date range, words; order). Store `decisions::supersede`, `views::decision_items`. Commands `list_decisions`, `get_decision`, `create_decision`, `update_decision`, `archive_decision`; `add_edge` routes `supersedes` through the store's atomic function.
- UI: Decisions screen (search + status filter; date, status, title and excerpt, "replaced by" / "affects" per row; superseded rows are struck through and muted) and a panel with Status/Decided on (follow every write), the write-up (context, decision, rationale) and archive. The Links section adds Affects and Supersedes. The Placeholder page is gone: every route is a real screen.

## Not yet verified by hand
- Decisions screen: `n` creates "Untitled decision" and opens it; rename it, fill context/decision/rationale, reload: it persisted
- set status to Decided: "Decided on" fills with today; set it to Proposed and back: the date stays
- Links -> Affects -> pick a project: the project's panel lists the decision under **Decisions**; the decision's row shows "affects <project>"
- create a second decision, Links -> Supersedes -> pick the first: the first becomes Superseded (struck through, "replaced by …"); adding the reverse link is refused with a loop message
- open the first one's panel before adding the link from the second: its Status switches to Superseded without reopening
- filter by status and search by a word from the rationale
- archive a decision: it leaves the list and the project's Decisions
