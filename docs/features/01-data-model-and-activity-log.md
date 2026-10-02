# 01 — Data model and activity log

Status: Done · Milestone: M1 · Priority: Must
Depends on: —

## Goal
Persist every node type, the typed edges table and the activity log, behind repository methods that write activity in the same transaction. Everything else builds on this.

## Scope
**In**
- Migration(s) for: objectives, projects, tasks, people, teams, notes, decisions, waiting_on, edges, activity (schemas in `CLAUDE.md` §4).
- Shared Rust types in `minimap-types`: one struct per node, `Create*`/`Update*` inputs, status enums, `NodeType`, `EdgeType`, `NodeRef { node_type, id }`.
- Repositories in `minimap-store`: create, update (partial), archive, unarchive, hard delete (only when archived), get, list.
- Activity row on every write: `created`, `updated` (diff of changed fields only), `archived`, `edge_added`, `edge_removed`.
- Archiving a node archives its edges in the same transaction.
- uuid v7 ids; timestamps as ISO-8601 UTC; dates as `YYYY-MM-DD`.

**Out**
- UI (each node feature adds its own screens).
- Edge validation rules (07).

## Data model notes
- Foreign keys: `tasks.project_id → projects`, `projects.owner_person_id → people`, `teams.parent_team_id → teams`, `waiting_on.person_id → people`. Edges reference nodes polymorphically, so no FK; integrity checked in code.
- `people.is_self`: enforce exactly one with a partial unique index (`WHERE is_self = 1`).
- `estimate_days` stored as REAL.
- `activity.diff` never contains note bodies at log level (it's in the DB, fine; just don't `tracing::info!` it).

## Rules and edge cases
- `update` with no actual change writes no activity row.
- Hard delete removes the node, its edges and keeps activity rows (history survives).
- All SQL parameterized; no string formatting.

## Acceptance criteria
- [x] Migrations apply on an empty DB and on a DB at schema v1.
- [x] Every repo write produces exactly one activity row (plus one per archived edge).
- [x] Hard delete of a non-archived node fails with a typed error.
- [x] `cargo test -p minimap-store` covers create/update/archive/delete for every node type.

## Tests
- Unit tests per repository against an in-memory DB.
- Proptest: random sequence of create/update/archive → activity count matches writes.

## Open questions
- ~~Separate `unarchive` action?~~ **Yes**: `unarchived`, plus `deleted` for hard deletes (history survives, so the delete itself must be recorded). `CLAUDE.md` §4.3 lists the original five actions; this adds two (ADR-0003).
- ~~Hard delete in the MVP?~~ **Yes**, via `nodes::delete`, only after archive.

## Implementation notes
- Types: `minimap-types` (`enums`, `nodes`, `edges`, `activity`, `patch`, `timefmt`). Nullable fields in `Update*` use `Patch<T>` (`keep` / `set` / `clear`).
- Store API: `minimap_store::{objectives,projects,tasks,people,teams,notes,decisions,waiting_on}::{create,get,list,update}`, generic `nodes::{archive,unarchive,delete}`, `edges::{add,remove,get,list_for_node,list_active}`, `activity::{list_for_node,list_recent,count}`.
- Edge activity rows attach to the edge's `from` node. `edges::add` revives an archived edge with the same (type, from, to).
- `unarchive` restores edges archived with the node (same timestamp) unless the other endpoint is still archived.
- `tasks::update` sets/clears `completed_at` when status moves to/from `done`.
- Not here (later specs): edge matrix and cycle rules (07), blocking self-archive/delete (03).
