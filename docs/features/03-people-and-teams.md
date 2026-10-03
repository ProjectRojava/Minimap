# 03 — People and teams

Status: Implemented — awaiting manual check · Milestone: M1 · Priority: Must
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
- [x] First run creates exactly one self person. *(store test `self_person_...`; UI prompt unverified by hand)*
- [x] Can create a 2-level team hierarchy and see members under each. *(store test `team_rows_are_a_tree...`; screens unverified by hand)*
- [x] Setting A reports_to B when B reports_to A is rejected with the cycle shown. *(command test `reports_to_cycle_is_rejected_with_the_path`: "Cy → Ann → Bob → Cy"; shown as a toast, unverified by hand)*

## Decisions
- **Nested teams and the `reports_to` org chart are both in the MVP.** The scope and acceptance criteria above already depend on them, and the cost is small now that cycle detection exists.
- A person has at most one manager (`set_manager` replaces the edge).
- Archiving a team that still has active sub-teams is refused ("Move or archive its sub-teams first"). Archiving a team archives its memberships.
- Active task = assigned and `todo` / `in_progress` / `blocked`.

## Implementation notes
- **Core** (`minimap-core`): the full edge matrix + attribute validation (`edge_rules`) and cycle detection with the loop path (`cycles::find_cycle`, petgraph; proptest: accepted inserts stay acyclic). Spec 07 builds on these.
- **Store**: `views::{people_rows, person_detail, person_archive_preview, team_rows, team_detail}`, `people::ensure_self`, `edges::list_active_of_type`; the generic `nodes::archive`/`delete` refuse the self person.
- **Commands**: `get_self_person`, `create_self_person`, `list_people`, `get_person`, `get_person_detail`, `create_person`, `update_person`, `preview_archive_person`, `archive_person`, `list_teams`, `get_team`, `get_team_detail`, `create_team`, `update_team`, `archive_team`, plus from 07: `add_edge`, `remove_edge`, `set_manager`. Rules run in the command layer (`check_new_edge`, `check_new_parent`), unit-tested without Tauri.
- **UI**: first-run name prompt; People screen (name, role, teams, active tasks, open waiting-ons; inline "New person"); Teams screen (tree with indentation); person and team panels in the detail pane with auto-saving fields (save on blur / Enter), teams (add, change role, remove), manager, parent team, archive with confirmation (lists the tasks that lose their assignee). Lists use `NodeRow`/`ListNav`, so `j`/`k`/`Enter` work on them.
- A write bumps `DataVersion`, which reloads lists and the pane; a rejected edit also reloads so controls snap back.
- Changing a membership role removes and re-adds the edge (the old edge is revived with the new role), so the activity log shows both.
- `api.rs` now serializes maps as plain objects; otherwise edge `attrs` would arrive as `{}`.

## Not yet verified by hand
Run `cargo tauri dev` (delete `~/.local/share/app.minimap.desktop/minimap.db` first to see the first-run prompt) and check:
- first-run prompt appears once, creates "you" in People
- New person / New team forms; the new item opens in the pane
- editing fields saves on blur; bad input (empty name) shows a toast
- adding a person to a team, changing role, removing; manager select; a loop attempt shows the cycle toast
- nesting teams two levels; moving a team under its own child is rejected
- archive person lists their active tasks first; yourself has no archive button
- `j`/`k`/`Enter`/`Esc` on the People and Teams lists
