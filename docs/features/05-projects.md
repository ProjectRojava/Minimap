# 05 — Projects

Status: Implemented — awaiting manual check · Milestone: M1 · Priority: Must
Depends on: 03, 04

## Goal
Bodies of work with an owner, dates and tasks; the main unit executives track.

## Scope
**In**
- Fields: title, description, owner_person_id, start_date, target_date, status (`planned`, `active`, `paused`, `done`, `cancelled`), priority.
- Short handle (slug) for quick-add `#api-launch`, auto-generated from title, editable, unique among active projects.
- List view (grouped by objective, then status) and board view (columns by status, drag to change).
- Project detail: fields, task list, dependencies in/out (`depends_on`), contributing objectives. Timeline + critical path arrive in 13.
- Commands: `create_project`, `update_project`, `archive_project`, `get_project`, `list_projects` (filters: status, owner, objective).

**Out**
- Timeline/critical path (13), health (15).

## Rules
- Archiving a project asks what to do with its tasks: archive them too, or move them to the inbox.
- `depends_on` must not create cycles (07).

## Acceptance criteria
- [x] Create a project under an objective with an owner; it shows grouped correctly. *(store/core tests; the New project form sets owner and objective; screens unverified by hand)*
- [x] Board drag changes status and writes activity. *(drag calls `update_project`, which writes an `updated` row — tested in the store; the drag itself is unverified by hand)*
- [x] Slug is unique and usable in quick-add later. *(unique among active projects, validated, backfilled for existing rows)*

## Decisions
- **The board view is in the MVP** (your call), next to the list view; a List | Board toggle in the header, shared filters (status, owner, objective).
- **List grouping**: one section per objective (by name), "No objective" last. A project that contributes to several objectives appears under each. Inside a section: active, planned, paused, done, cancelled, then priority (1 = highest), then target date.
- **Board columns**: Planned, Active, Paused, Done, Cancelled — always all five so a card can be dropped anywhere. Cards sort by priority then target date.
- **Handle (slug)**: lowercase letters, digits and single hyphens, max 40 characters (e.g. `api-launch`). Generated from the title (`-2`, `-3` on clashes), editable, unique among *active* projects (archiving frees it; restoring a project whose handle was reused is refused). Renaming a project keeps its handle.
- **Archiving** asks about the tasks: "Archive tasks too" or "Move tasks to inbox" (clears their project). Both happen in one transaction with the archive.
- Done and cancelled tasks are treated like any other when archiving (archived or moved).

## Implementation notes
- **Migrations**: `0003_project_slug.sql` + a Rust hook that backfills existing projects (via `core::slug`), `0004_project_slug_index.sql` (unique index on active projects). Verified on the real app database (migrated to v4 on launch).
- **Core**: `slug` (`slugify`, `validate`, `unique`; proptest that any title yields a valid handle) and `projects` (`filter`, `by_objective`, `by_status`).
- **Types**: `Project.slug`, `ProjectLayout/Filter/Row/Group/Task/Detail/ArchivePreview`, `TaskDisposition`.
- **Store**: handle rules in `projects::create/update`, `projects::archive(id, disposition)`, `views::{project_rows, project_detail, project_archive_preview}`; `nodes::archive_in_tx` so archives compose into one transaction. The store now depends on `minimap-core` for slug validation/generation.
- **Commands**: `list_projects(filter_by, layout)`, `get_project`, `get_project_detail`, `create_project`, `update_project`, `preview_archive_project`, `archive_project(id, tasks)`.
- **UI**: Projects screen (list + board with HTML5 drag-and-drop, filters, New project form with owner and objective) and a project panel (fields incl. handle and dates, owner, objectives with weight, dependencies, read-only task list, archive with the task choice). `tauri.conf.json` sets `dragDropEnabled: false`, otherwise Tauri's file-drop handler breaks HTML5 drag-and-drop on Windows.
- `depends_on` loops are rejected by the existing command-layer check (toast: "…this would create a loop — A → B → A").
- The objective panel's contribution picker now lists projects, and `WeightInput` is shared with the project panel.
- **Not done**: keyboard shortcut to move a card between columns (the Status select in the pane works by keyboard); creating tasks from the project panel (spec 06 — the list is read-only until then); the timeline (13) and health (15).

## Not yet verified by hand
- New project with owner + objective; it opens in the pane and appears under that objective
- handle auto-generation, editing, and the "already used" error; a handle clash on a second project
- List <-> Board toggle and the three filters; counts and ordering
- dragging a card to another column changes its status (and the card moves); dropping on the same column does nothing
- adding a dependency; a loop attempt shows the cycle toast
- archive: with tasks (both choices) and without; the tasks end up archived / without a project
- `j`/`k`/`Enter` on both layouts
