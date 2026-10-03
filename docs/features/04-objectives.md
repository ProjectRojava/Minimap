# 04 — Objectives

Status: Implemented — awaiting manual check · Milestone: M1 · Priority: Must
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
- [x] CRUD works from list and detail pane. *(store + command tests; screens unverified by hand)*
- [x] Contributing projects can be linked with a weight 0–1. *(store/command tests incl. weight validation; the picker is empty in the UI until projects and tasks can be created, specs 05/06)*

## Decisions
- **Manual status stays, as "Your assessment".** "Done" can't be computed, and your own call (e.g. a key hire falling through) isn't in the task data. When health is computed (15) it is shown next to the assessment with a "differs from computed health" marker; only computed health feeds the Overview, risks and roll-ups (a `done` objective drops out of risk lists). No schema change. *The marker itself is part of spec 15.*
- **Quarterly grouping is derived, not stored.** A "Group by quarter" toggle buckets objectives by the calendar quarter of `target_date` (Q1 2027), chronologically, with a final "No date" bucket. A fiscal-year start month can be added to Settings (23) later; an explicit `period` field only if the derived quarter proves insufficient.
- **Priority 1 is the highest** (matches `!1`–`!5` in quick-add). List order: priority, then target date (undated last), then title.
- New objectives default to priority 3, status on track.

## Implementation notes
- **Core** `minimap-core::objectives`: `quarter_of`, `quarter_label`, `arrange(rows, grouping)` (sorting + grouping; the UI can't call core, so the backend returns groups already arranged). Unit-tested incl. quarter boundaries.
- **Types**: `ObjectiveGrouping`, `ObjectiveRow`, `ObjectiveGroup`, `Contribution`, `ObjectiveDetail`.
- **Store**: `views::{objective_rows, objective_detail}`, `nodes::list_summaries(node_type)` (picker source), `edges::update_attrs` (one `updated` activity row on the from node; unchanged attrs write nothing).
- **Commands**: `list_objectives(grouping)`, `get_objective`, `get_objective_detail`, `create_objective`, `update_objective`, `archive_objective`, plus generic `list_node_summaries(node_type)` and `update_edge_attrs(edge_id, attrs)` (validated by core; part of 07).
- **UI**: Objectives screen (columns: priority, objective, assessment, target, work count; New objective form; group-by-quarter toggle; `j`/`k`/`Enter` follow on-screen order across groups) and an objective panel in the detail pane (fields, assessment, priority, contributing projects/tasks with status and editable weight, add via picker with default weight 1, archive with confirmation). "Needs attention" (at risk / off track) shows as heavier text, since the theme is monochrome.
- Changing a team role now uses `update_edge_attrs` instead of remove + re-add.
- Update (spec 05): the picker now lists projects, and a project's own panel edits the same links (`contributes_to`, weight shared via `WeightInput`).
- Contributions are listed from the objective's side; a project or task's own "contributes to" editor comes with 05/06 (and the generic link editor in 07).

## Not yet verified by hand
- New objective (with and without date); it opens in the pane; fields save on blur; bad date shows a toast
- priority and assessment selects; list re-sorts after a priority change
- "Group by quarter": quarter headings in date order, "No date" last
- archive confirmation; the objective disappears from the list
- contribution picker, weight edits and removal can only be exercised once projects/tasks exist (05/06)
