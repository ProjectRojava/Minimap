# Minimap knowledge graph

**Read this first, then open only the files it points to.** It is a map of the code: what exists, how it connects, where each rule is enforced and tested. It is not a spec (see `CLAUDE.md`, `docs/features/`) and not a tutorial.

- **Keep it current**: any change that adds, removes, renames or re-wires something listed here must update this file in the same change (checklist in §14). If something here disagrees with the code, the code wins; fix this file.
- **Format**: every thing has an id `kind:name` (`crate:`, `mod:`, `fn:`, `cmd:`, `table:`, `type:`, `ui:`, `spec:`, `rule:`). Edges are written `A -> B  (relation)`. Grep an id to find every line that mentions it, e.g. `grep -n "cmd:add_edge" docs/knowledge-graph.md`.
- **Last verified against code**: after spec 03 + monochrome theme (2026-10-03). Specs 04–27 not started.

---

## 1. Orientation

Local-first desktop app (Tauri 2.12 shell, Leptos 0.8 CSR/WASM UI built by Trunk + Tailwind v4, SQLite via rusqlite/SQLCipher). Rust everywhere, no npm. Single user; people are records. Network allowed only for user-connected features (ADR-0002); no telemetry.

```
UI (wasm)  --invoke-->  Tauri commands  --rules-->  core (pure)
                              |                        
                              +--------------------->  store (SQLite + activity log)
all of them share DTOs from crate:minimap-types
```

Status: M0 done (progress.md M0 checkboxes are stale), M1 in progress: specs 01, 02, 03 done/implemented; 07 partly (see §12).

## 2. Crates and dependency edges

```
crate:minimap-types  -> (serde, serde_json, uuid[serde only], time)        # wasm-safe, no IO
crate:minimap-core   -> crate:minimap-types, petgraph, serde_json, thiserror
crate:minimap-store  -> crate:minimap-types, rusqlite(+sqlcipher), rusqlite_migration, uuid[v7], time, serde, serde_json, thiserror
crate:minimap (src-tauri) -> types, core, store, tauri, tracing, anyhow
crate:minimap-ui (ui)     -> types ONLY (+ leptos, leptos_router, wasm-bindgen, serde-wasm-bindgen, js-sys, serde_json)
```
Rules: ui must never depend on store/core/rusqlite/petgraph/tauri. `uuid` v7 generation only in store (the `v7` feature would break wasm builds if enabled in types). Versions are pinned in the workspace `Cargo.toml`.

## 3. Domain graph

### 3.1 Node types (`type:NodeType`) -> table -> store module -> DTOs -> label column
| NodeType | table | store mod | DTO / Create / Update | label col | spec |
|---|---|---|---|---|---|
| objective | `objectives` | `mod:objectives` | Objective / CreateObjective / UpdateObjective | title | 04 |
| project | `projects` | `mod:projects` | Project / CreateProject / UpdateProject | title | 05 |
| task | `tasks` | `mod:tasks` | Task / CreateTask / UpdateTask | title | 06 |
| person | `people` | `mod:people` | Person / CreatePerson / UpdatePerson | name | 03 |
| team | `teams` | `mod:teams` | Team / CreateTeam / UpdateTeam | name | 03 |
| note | `notes` | `mod:notes` | Note / CreateNote / UpdateNote | title | 09 |
| decision | `decisions` | `mod:decisions` | Decision / CreateDecision / UpdateDecision | title | 10 |
| waiting_on | `waiting_on` | `mod:waiting_on` | WaitingOn / CreateWaitingOn / UpdateWaitingOn | description | 08 |

Common columns: `id` (uuid v7 TEXT), `created_at`, `updated_at`, `archived_at` (soft delete). Dates `YYYY-MM-DD`; timestamps `YYYY-MM-DDTHH:MM:SS.mmmZ` UTC (`mod:timefmt`).
Foreign keys: `tasks.project_id`, `projects.owner_person_id`, `teams.parent_team_id`, `waiting_on.person_id`. Everything else links through `table:edges`.
Defaults: priority 3, weekly capacity 40h, project `planned`, task `todo`, objective `on_track`, decision `proposed`, note kind `general`.

### 3.2 Edge matrix (`fn:edge_rules::is_allowed`; table `edges`, polymorphic, no FKs)
| edge_type | from -> to | attrs (all optional) | acyclic |
|---|---|---|---|
| blocks | task -> task | lag_days (int >= 0) | yes |
| depends_on | project -> project | note | yes |
| contributes_to | project/task -> objective | weight 0-1 | |
| assigned_to | task -> person | allocation_pct 1-100 | |
| member_of | person -> team | role lead/member (default member) | |
| reports_to | person -> person | - | yes (one manager per person, UI-enforced via `cmd:set_manager`) |
| relates_to | any -> any | note | |
| mentions | note -> any | - | |
| affects | decision -> project/task/objective | - | |
| about | waiting_on -> task/project | - | |

Team nesting is `teams.parent_team_id` (not an edge) and is cycle-checked by `fn:check_new_parent`.

### 3.3 Activity log (`table:activity`)
Actions (`type:ActivityAction`): created, updated, archived, **unarchived**, **deleted**, edge_added, edge_removed (last-but-two are additions beyond CLAUDE.md, ADR-0003). `diff` = `{field: [old, new]}`; created = `[null, value]` for every non-meta field; archive = `{archived_at: [null, ts]}`; edge rows = `{edge: [null|desc, desc|null]}` and are attached to the edge's **from** node. History survives hard delete (no FK).

## 4. Types crate (`crate:minimap-types`, files in `crates/minimap-types/src/`)
- `enums.rs`: `str_enum!` macro -> NodeType, EdgeType, ObjectiveStatus, ProjectStatus, TaskStatus, NoteKind, DecisionStatus, ActivityAction. Each has `as_str()`, `FromStr`, `ALL`; serde is snake_case and **must equal** `as_str()` (tested). SQL CHECK constraints repeat the status values.
- `nodes.rs`: node structs, `Create*`, `Update*` (with `apply(&self-patch, &mut node)`), `NodeRef{node_type,id}`, `NodeSummary{node,label,archived}`, constants `DEFAULT_PRIORITY`, `DEFAULT_WEEKLY_CAPACITY_HOURS`.
- `patch.rs`: `Patch<T> = Keep | Set(T) | Clear` for nullable fields in `Update*` (non-null fields use `Option<T>`). `Task.completed_at` is **not** patchable (store sets it).
- `edges.rs`: `Edge`, `NewEdge{edge_type,from,to,attrs}`, `EdgeLink{edge,outgoing,other:NodeSummary}`.
- `activity.rs`: `Activity`. `lib.rs`: `PingResponse`, `AppError{code,message}`, re-exports `Uuid`.
- `views.rs` (read models): `Membership{edge_id,node,role}`, `LinkedNode{edge_id,node}`, `PersonRow`, `PersonDetail`, `PersonArchivePreview`, `TeamRow{team,depth,member_count}`, `TeamDetail`.
- `timefmt.rs`: `fmt_date`, `parse_date`, `fmt_ts`, `parse_ts`.
- Serde: dates/uuids are human-readable strings; timestamps use `time::serde::rfc3339`.

## 5. Core crate (`crate:minimap-core`, pure)
- `mod:edge_rules`: `is_allowed`, `validate_types`, `validate_attrs`, `validate(edge_type, from:NodeRef, to:NodeRef, attrs)` (self-edge first), `must_be_acyclic`, `EdgeRuleError{NotAllowed,SelfEdge,BadAttr}`.
- `mod:cycles`: `find_cycle(existing:&[(from,to)], from, to) -> Option<Vec<Uuid>>` returns `[from,to,...,from]` (BFS over petgraph `DiGraphMap`).
- `fn:pong` (M0 smoke).
- Not yet: CPM schedule, impact, health, capacity, quick-add parser (specs 12-17).

## 6. Store crate (`crate:minimap-store`, all SQL lives here)
Migrations (`migrations/`, embedded with `include_str!`, `fn:migrations` in lib.rs): `0001_init.sql` (`app_meta`), `0002_core_data.sql` (all node tables, `edges`, `activity`, unique index `idx_people_single_self`). Next migration must be `0003_*.sql` appended in `migrations()`.

| module | public API |
|---|---|
| `lib.rs` | `open(path)`, `open_in_memory()`, `schema_version(conn)`; sets WAL + foreign_keys, runs migrations; re-exports `Connection`, `StoreError`, `Result` |
| `error.rs` | `StoreError`: Sqlite, Migration, NotFound{node_type,id}, EdgeNotFound, NotArchived, AlreadyArchived, NotArchivedYet, DuplicateEdge, Invalid(msg), Constraint(msg) (any SQLite constraint violation), Json |
| each node mod (`objectives`..`waiting_on`) | `get`, `list(conn, include_archived)`, `create`, `update(id, patch)`; every write = one tx + one activity row; no-op update writes nothing; validation (blank title/name, priority 1-5, capacity > 0, estimate >= 0) |
| `people.rs` extra | `get_self`, `ensure_self(name)` (blank -> "Me", idempotent) |
| `tasks.rs` | `update` sets/clears `completed_at` when status enters/leaves `done` |
| `nodes.rs` | generic over NodeType: `archive` (archives touching edges; one activity row each), `unarchive` (restores edges archived at the same timestamp unless other end archived), `delete` (only if archived; removes edges; keeps history), `summary(node)`; archive/delete **refuse the self person** |
| `edges.rs` | `get`, `list_for_node(id, include_archived)`, `list_active`, `list_active_of_type`, `add` (revives archived edge with same (type,from,to); errors DuplicateEdge/NotFound/Invalid if an end is archived), `remove` (soft), `links_for_node` -> `EdgeLink` |
| `activity.rs` | `list_for_node`, `list_recent(limit)`, `count`; internal `record`, `record_created`, `diff` |
| `views.rs` | `people_rows`, `person_detail`, `person_archive_preview`, `team_rows` (tree order, depth), `team_detail` |
| internal | `convert.rs` (row/param helpers, `now()` ms-truncated), `repo.rs` (`fetch`, `fetch_all`, `table()`) |

Store does **not** enforce the edge matrix or cycles (command layer does, using core).

## 7. Command layer (`crate:minimap`, `src-tauri/src/`)
Infra: `main.rs` (setup: app-data dir, log file `minimap.log`, open DB `minimap.db`, `manage(AppState)`, `invoke_handler`), `state.rs` (`AppState{db:Arc<Mutex<Connection>>}`, `run(|conn|...)` = `spawn_blocking`), `error.rs` (`app_error`, `store_error` code mapping, `rule_error`, `cycle_error`).
Error codes: `not_found`, `invalid`, `constraint`, `duplicate`, `state`, `store`, `invalid_edge`, `cycle`, `internal`, `ipc` (UI-side).
**Adding a command = 4 places**: the fn + registration in `main.rs` `generate_handler!`; name in `build.rs` `COMMANDS`; `allow-<kebab-name>` in `capabilities/default.json`; wrapper in `ui/src/api.rs`. (Tauri arg keys: snake_case Rust params are camelCase on the JS side.)

| cmd | file | calls | UI wrapper -> used by |
|---|---|---|---|
| ping | commands/system.rs | store::schema_version, core::pong | api::ping -> ui:Overview |
| get_node_summary | nodes.rs | store::nodes::summary | detail pane header |
| list_edges_for | nodes.rs | store::edges::links_for_node | detail pane Links |
| list_activity_for | nodes.rs | store::activity::list_for_node | detail pane Activity |
| get_self_person / create_self_person | people.rs | people::get_self / ensure_self | ui:FirstRun |
| list_people | people.rs | views::people_rows | ui:People, person panel pickers |
| get_person / get_person_detail | people.rs | people::get / views::person_detail | ui:PersonPanel |
| create_person | people.rs | people::create (forces is_self=false) | ui:People form |
| update_person | people.rs | people::update | ui:PersonFields |
| preview_archive_person / archive_person | people.rs | views::person_archive_preview / nodes::archive | ui:ArchivePerson |
| list_teams / get_team / get_team_detail | teams.rs | views::team_rows / teams::get / views::team_detail | ui:Teams, team panel |
| create_team | teams.rs | check_parent_active, teams::create | ui:Teams form |
| update_team | teams.rs | `fn:check_new_parent` (core::cycles) then teams::update | ui:TeamFields, Structure |
| archive_team | teams.rs | refuses if active sub-teams, then nodes::archive | ui:ArchiveTeam |
| add_edge | edges.rs | `fn:check_new_edge` (core edge_rules + cycles) then edges::add | person panel (teams) |
| remove_edge | edges.rs | edges::remove | person panel |
| set_manager | edges.rs | `fn:set_manager_impl` (validate, remove old, add new) | person panel |

Tauri config: `app.withGlobalTauri: true`, CSP `default-src 'self'` (+ `connect-src ipc:`), devUrl `localhost:1420`, frontendDist `../ui/dist`, identifier `app.minimap.desktop`. Permissions are an allow-list (app manifest); generated files in `src-tauri/permissions/autogenerated/` and `gen/` are build output.

## 8. UI (`crate:minimap-ui`, `ui/src/`)
- Entry: `main.rs` -> `app.rs::App` (provides contexts, `<Router>`, `Shell`). `Shell` = Sidebar | `<Routes>` | DetailPane | ToastHost | FirstRun; installs `keyboard::use_global_shortcuts()`; clears `ListNav` on route change.
- Routes: `/` Overview; `/people` People; `/teams` Teams; `/inbox /objectives /projects /tasks /notes /decisions /waiting-on /settings` = `Placeholder` (name the spec); `/:type/:id` DeepLink (selects the node, redirects to its list). `/this-week` and `/weekly-review` are hidden (`enabled:false` in `nav::NAV`).
- Contexts (`state.rs`): `Selection(RwSignal<Option<NodeRef>>)` (open node in pane), `ListNav` (rows + cursor for j/k/Enter; `set_items` keeps cursor), `Toasts` (`error(&AppError)`, `info`), `DataVersion` (`track()` inside resource closures, `bump()`), helper `finish(result, toasts, version)` (bumps always, toasts errors).
- Data flow for writes: handler -> `spawn_local(api::x)` -> `finish(...)` -> `DataVersion` bump -> version-keyed `LocalResource`s reload (lists, pane Links/Activity/summary). Fields resources are **not** version-keyed so typing isn't overwritten.
- `api.rs`: one typed async fn per command; `invoke` serializes args with maps-as-objects (else JS `Map` -> `{}`), maps errors to `AppError` (code `ipc` for bridge failures). Mirrors §7.
- Pages (`pages/`): `overview` (ping demo), `people`, `teams`, `placeholder`, `deep_link` (+`NotFound`).
- Components (`components/`): `sidebar` (from `nav::NAV`), `detail_pane` (header, `PersonPanel`/`TeamPanel`/placeholder, Links grouped by `link_heading`, Activity via `describe`; `edited_elsewhere` hides links the panel edits; split view >= 1100px, overlay below), `people_panel` (`PersonPanel`, fields, Organization: manager + teams/roles, waiting-ons, `ArchivePerson`; helpers `NodeButtons`, `team_options`, `indented`, `error_line`), `team_panel` (fields, Structure: parent + sub-teams, Members, `ArchiveTeam`), `node_row` (list row using `ListNav`/`Selection`), `form` (`TextField` commit-on-change, `SelectField`, class consts `INPUT/BUTTON/BUTTON_PRIMARY/BUTTON_DANGER`), `toasts`, `first_run`.
- `nav.rs` (pure, unit-tested): `NAV` table (label, path, chord, enabled), `chord_target`, `list_path`, `type_label`, `move_cursor`, `is_typing_target`. `keyboard.rs`: `g`+letter chords (1s window), `j/k/Enter` on lists, `Esc` closes pane; ignored while typing and for Ctrl/Cmd/Alt.
- Chords: o Overview, i Inbox, w This week*, b Objectives, p Projects, t Tasks, e People, m Teams, n Notes, d Decisions, a Waiting on, r Weekly review*, s Settings (*hidden).
- Style: `ui/style/input.css` defines theme tokens (CSS vars, light/dark via `prefers-color-scheme`, `data-theme` override) mapped to Tailwind utilities (`bg-canvas bg-panel bg-hover bg-active border-line text-fg text-muted text-faint text-danger bg-scrim`). **No raw palette colours, no `dark:` variants, no shadows** (`docs/design.md`). Logo from `branding/` copied by Trunk (`/minimap-logo-auto.svg`).
- `view!` macro gotchas: closures with `match` bodies and method chains must be named variables outside the macro; `ts!`-style macros can't expand to struct fields.

## 9. Key flows
- **First run**: UI `FirstRun` -> `get_self_person` None -> prompt -> `create_self_person` -> `DataVersion` bump.
- **Add link**: UI -> `cmd:add_edge` -> `check_new_edge` (matrix, attrs, self-edge, cycle for blocks/depends_on/reports_to, error text lists node names: "Can't add this link: this would create a loop — A → B → A") -> `edges::add` (tx + activity).
- **Archive person**: `preview_archive_person` (active assigned tasks) -> confirm -> `nodes::archive` (person + edges in one tx; self refused).
- **Nest team**: `update_team` with `Patch::Set(parent)` -> `check_new_parent` (parent active, no loop incl. archived teams) -> `teams::update`.

## 10. Invariants -> where enforced -> where tested
| rule | enforced in | tested in |
|---|---|---|
| exactly one self person | unique index + `ensure_self` + `create` check | store tests `exactly_one_self_person`, `self_person_cannot_be_archived_or_deleted` |
| self can't be archived/deleted | `store::nodes::ensure_not_self` | same |
| edge matrix + attrs | core `edge_rules` (called by `check_new_edge`) | core `matrix_matches_spec` (exhaustive), command tests |
| no cycles (blocks/depends_on/reports_to) | core `cycles` via `check_new_edge`/`set_manager_impl` | core proptest, command tests |
| no team-nesting cycles | `check_new_parent` | `commands/teams.rs` tests |
| archive cascades to edges; unarchive restores | `store::nodes` | store tests + proptest on activity counts |
| hard delete only after archive | `store::nodes::delete` | store tests |
| every write -> activity in same tx | each repo fn | store tests (`create_writes_one_created_activity...`, proptest) |
| no-op update writes no activity | repo `update` fns | `noop_update_writes_no_activity` |
| CHECK/FK integrity | migration 0002 | `invalid_input_is_rejected...`, `delete_of_referenced_node...` |

## 11. Tests inventory
`cargo test --workspace`: core 12 (edge_rules, cycles + proptest), types 4, store 3 unit + 25 integration (`crates/minimap-store/tests/repos.rs`, incl. proptest `activity_count_matches_writes`), src-tauri 7 (`commands/edges.rs`, `commands/teams.rs`), ui 8 (`nav.rs`, `detail_pane.rs`, `people_panel.rs`). No UI/browser tests; UI behaviour is verified by hand (checklists in specs 02, 03). Gate: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p minimap-ui --target wasm32-unknown-unknown -- -D warnings`, `cargo test --workspace`, `cd ui && trunk build`.

## 12. Feature specs (`docs/features/NN-*.md`; index in its README)
| # | feature | status | touches / notes |
|---|---|---|---|
| 01 | data model + activity | Done | store, types |
| 02 | app shell + navigation | Done | ui shell, node-summary/edges/activity cmds |
| 03 | people + teams | Implemented, manual check pending | people/team cmds, views, UI screens/panels, core cycles+edge_rules |
| 04 | objectives | Draft | next: node CRUD cmds + screen + panel |
| 05 | projects | Draft | |
| 06 | tasks + inbox | Draft | will make `NodeRow`/inline edit shine; `assigned_to` counts already feed People rows |
| 07 | edges + cycle detection | Draft, **partly built** | done: core rules, `add_edge`, `remove_edge`, `set_manager`; todo: `update_edge_attrs`, generic Links editor in pane, blocks/depends_on UI |
| 08-12 | waiting-on, notes, decisions, search, palette/quick-add | Draft | |
| 13-18 | CPM, impact, health/overview, this-week, capacity, graph view | Draft | core algorithms |
| 19-27 | review/export, backup, encryption, Drive, settings, demo data, undo, data export, recurring | Draft | |
ADRs: 0001 versions + command allow-list, 0002 network allowed + Drive backup, 0003 extra activity actions + `Patch<T>` + completed_at. Design: `docs/design.md`.

## 13. Conventions and gotchas
- No `unwrap/expect` outside tests/main setup (exceptions: `build.rs`). Domain rules in core; commands thin; UI has no business rules; all SQL in store, parameterized (table/column names are compile-time constants only).
- Every write is a transaction that also writes `activity`.
- Specs: implement only `Status: Ready` (user may override by asking); record answers to open questions + deviations in the spec/an ADR.
- Commits: user commits; end messages with the attribution line from the session system reminder.
- `ui/dist/` and `src-tauri/gen/` are build output (in `.gitignore`; `gen/` was committed earlier and may still be tracked).
- WSL: `libEGL`/`MESA` warnings from WebKitGTK are harmless.
- Role change on a membership = remove + re-add edge (revive path), so activity shows both.
- Tailwind classes live in Rust source; Trunk's Tailwind scans `.rs` files.

## 14. Update checklist (do this when you change code)
| you changed | update |
|---|---|
| a node/Create/Update type, enum, view DTO | §3.1, §4 |
| a migration or table | §3, §6 (and next migration number) |
| a store function | §6 |
| a core rule/algorithm | §5, §10 |
| a Tauri command (add/rename/remove) | §7 table (+ the 4 places), §8 api list, §9 if it changes a flow |
| a UI route, page, component, context, shortcut | §8 |
| theme tokens | §8 Style, `docs/design.md` |
| an invariant or where it is enforced/tested | §10 |
| test counts or the gate | §11 |
| a spec's status or scope | §12 (+ the spec file, `docs/progress.md`) |
| an ADR or convention | §12 / §13 |
Also bump "Last verified against code" at the top.
