# 03 — People and teams

Status: Draft · Milestone: M1 · Priority: Must
Depends on: 01, 02

## Goal
Track the people the user works with (not app users) and group them into teams. Exactly one person is the user ("me").

## Scope
**In**
- Self person created on first run (name asked in a small first-run prompt; default "Me").
- People list: name, role, team(s), active task count, open waiting-ons. Create/edit/archive.
- Person detail: fields, teams, manager and reports, assigned tasks, notes mentioning them, waiting-ons.
- Teams list and detail; nesting via `parent_team_id` (no cycles).
- `member_of` (with role lead/member) and `reports_to` edges editable from the person detail.
- Commands: `create_person`, `update_person`, `archive_person`, `get_person`, `list_people`; same for teams.

**Out**
- Capacity heatmap (17).

## Rules and edge cases
- The self person can't be archived or deleted.
- No cycles in `reports_to` or team nesting (uses cycle check from 07).
- Archiving a person leaves their tasks unassigned (assigned_to edges archived) and shows a warning listing affected tasks first.

## Acceptance criteria
- [ ] First run creates exactly one self person.
- [ ] Can create a 2-level team hierarchy and see members under each.
- [ ] Setting A reports_to B when B reports_to A is rejected with the cycle shown.

## Open questions
- Is nested teams needed for MVP, or a flat team list enough?
- Should the org chart (`reports_to`) be in the MVP?
