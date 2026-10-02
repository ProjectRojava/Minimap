# 05 — Projects

Status: Draft · Milestone: M1 · Priority: Must
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
- [ ] Create a project under an objective with an owner; it shows grouped correctly.
- [ ] Board drag changes status and writes activity.
- [ ] Slug is unique and usable in quick-add later.

## Open questions
- Is the board view needed for MVP, or list only?
