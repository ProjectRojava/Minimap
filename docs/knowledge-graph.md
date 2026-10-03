# Minimap knowledge graph

**Read this first, then open only the files it points to.** It is a map of the code: what exists, how it connects, where each rule is enforced and tested. It is not a spec (see `CLAUDE.md`, `docs/features/`) and not a tutorial.

- **Keep it current**: any change that adds, removes, renames or re-wires something listed here must update this file in the same change (checklist in §14). If something here disagrees with the code, the code wins; fix this file.
- **Format**: every thing has an id `kind:name` (`crate:`, `mod:`, `fn:`, `cmd:`, `table:`, `type:`, `ui:`, `spec:`, `rule:`). Edges are written `A -> B  (relation)`. Grep an id to find every line that mentions it, e.g. `grep -n "cmd:add_edge" docs/knowledge-graph.md`.
- **Last verified against code**: after spec 07 edges + generic Links editor (2026-10-03). Specs 08–27 not started.

---

## 1. Orientation

Local-first desktop app (Tauri 2.12 shell, Leptos 0.8 CSR/WASM UI built by Trunk + Tailwind v4, SQLite via rusqlite/SQLCipher). Rust everywhere, no npm. Single user; people are records. Network allowed only for user-connected features (ADR-0002); no telemetry.

```
UI (wasm)  --invoke-->  Tauri commands  --rules-->  core (pure)
                              |                        
                              +--------------------->  store (SQLite + activity log)
all of them share DTOs from crate:minimap-types
```

Status: M0 done (progress.md M0 checkboxes are stale), M1 in progress: specs 01-02 done, 03-07 implemented (manual check pending).

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
- `nodes.rs`: node structs, `Create*`, `Update*` (with `apply(&self-patch, &mut node)`), `Project.slug` (handle; `CreateProject.slug: Option`, `UpdateProject.slug`), `AssigneeChoice{Me,Nobody,Person(uuid)}` (`CreateTask.assignee`, default Me), `NodeRef{node_type,id}`, `NodeSummary{node,label,archived}`, constants `DEFAULT_PRIORITY`, `DEFAULT_WEEKLY_CAPACITY_HOURS`.
- `patch.rs`: `Patch<T> = Keep | Set(T) | Clear` for nullable fields in `Update*` (non-null fields use `Option<T>`). `Task.completed_at` is **not** patchable (store sets it).
- `edges.rs`: `Edge`, `NewEdge{edge_type,from,to,attrs}`, `EdgeLink{edge,outgoing,other:NodeSummary}`. `AttrKind{WholeNumber{min,max},Number{min,max},Choice{options},Text}`, `AttrSpec{key,label,kind,hint}`, `LinkOption{edge_type,outgoing,others,attrs}` (a relation addable from a node type).
- `activity.rs`: `Activity`. `lib.rs`: `PingResponse`, `AppError{code,message}`, re-exports `Uuid`.
- `views.rs` (read models): `Membership{edge_id,node,role}`, `LinkedNode{edge_id,node}`, `PersonRow`, `PersonDetail`, `PersonArchivePreview`, `TeamRow{team,depth,member_count}`, `TeamDetail`; objectives: `ObjectiveGrouping{None,Quarter}`, `ObjectiveRow{objective,contribution_count}`, `ObjectiveGroup{label:Option<String>,rows}`, `Contribution{edge_id,node,status,weight}`, `ObjectiveDetail`. projects: `ProjectLayout{List,Board}`, `ProjectFilter{status,owner_person_id,objective_id}`, `ProjectRow{project,owner,objectives,task_count,done_task_count}`, `ProjectGroup{label,objective,status,rows}`, `ProjectTask`, `ProjectDetail{project,owner,objectives:Vec<Contribution>,depends_on:Vec<LinkedNode>,needed_by,tasks}`, `TaskDisposition{Archive,Inbox}`, `ProjectArchivePreview`. tasks: `TaskFilter{status,project_id,assignee_id,due_from,due_to,text,no_project,include_closed}`, `TaskRow{task,project,assignee}`, `TaskDetail`; settings: `Settings{hours_per_day, theme}` (defaults 8 and `DEFAULT_THEME` = `minimap-dark`), `UpdateSettings`.
- `timefmt.rs`: `fmt_date`, `parse_date`, `fmt_ts`, `parse_ts`.
- Serde: dates/uuids are human-readable strings; timestamps use `time::serde::rfc3339`.

## 5. Core crate (`crate:minimap-core`, pure)
- `mod:edge_rules`: `is_allowed`, `validate_types`, `validate_attrs`, `validate(edge_type, from:NodeRef, to:NodeRef, attrs)` (self-edge first), `must_be_acyclic`, `EdgeRuleError{NotAllowed,SelfEdge,BadAttr}`. Attribute rules are schema-driven: `attr_schema(edge_type) -> Vec<AttrSpec>` is the single definition used by `validate_attrs` and by editors; `link_options(node_type) -> Vec<LinkOption>` lists every relation addable from a node type in both directions (exactly what the matrix allows; `relates_to` symmetric so offered once, outgoing).
- `mod:cycles`: `find_cycle(existing:&[(from,to)], from, to) -> Option<Vec<Uuid>>` returns `[from,to,...,from]` (BFS over petgraph `DiGraphMap`).
- `mod:tasks`: `filter(rows,&TaskFilter)` (closed tasks hidden unless `include_closed` or an explicit status; `no_project` = inbox; due range inclusive and skips undated; text = every word in title/description/project/assignee), `sort` (due date, undated last; priority; title), `parse_estimate(text, hours_per_day)` (`3d`, `1.5d`, `4h`, bare number = days; rounds to 4 dp; `EstimateError` "Estimates look like 3d or 4h"), `parse_lines(text)` (one title per non-empty line, strips `-`, `*`, `1.`, `[ ]` markers).
- `mod:slug`: `slugify(title)` (lowercase ascii words joined by `-`, max 40, fallback `project`), `validate(slug)`, `unique(base, is_taken)` (`-2`, `-3`...). `mod:projects`: `filter(rows, &ProjectFilter)`, `by_objective(rows)` (one section per objective by name, "No objective" last; a project under several objectives appears under each; inside: status active>planned>paused>done>cancelled, then priority, date), `by_status(rows)` (always 5 board columns: planned, active, paused, done, cancelled; priority then date), `status_title`.
- `mod:objectives`: `quarter_of(date)`, `quarter_label`, `arrange(rows, grouping)` = sort (priority asc, **1 = highest**, then target date with undated last, then title, id) + group (flat, or calendar quarters chronological + final "No date"). The backend returns groups already arranged because the UI can't call core.
- `fn:pong` (M0 smoke).
- Not yet: CPM schedule, impact, health, capacity, quick-add parser (specs 12-17).

## 6. Store crate (`crate:minimap-store`, all SQL lives here)
Migrations (`migrations/`, embedded with `include_str!`, `fn:migrations` in lib.rs): `0001_init.sql` (`app_meta`), `0002_core_data.sql` (all node tables, `edges`, `activity`, unique index `idx_people_single_self`). Next migration must be `0003_*.sql` appended in `migrations()`. `0003_project_slug.sql` (+ Rust hook `backfill_project_slugs` giving existing projects unique handles via core::slug), `0004_project_slug_index.sql` (unique index on `slug` for active projects; archiving frees the handle). `0005_settings.sql` (`settings(key, value JSON)`).

| module | public API |
|---|---|
| `lib.rs` | `open(path)`, `open_in_memory()`, `schema_version(conn)`; sets WAL + foreign_keys, runs migrations; re-exports `Connection`, `StoreError`, `Result` |
| `error.rs` | `StoreError`: Sqlite, Migration, NotFound{node_type,id}, EdgeNotFound, NotArchived, AlreadyArchived, NotArchivedYet, DuplicateEdge, Invalid(msg), Constraint(msg) (any SQLite constraint violation), Json |
| each node mod (`objectives`..`waiting_on`) | `get`, `list(conn, include_archived)`, `create`, `update(id, patch)`; every write = one tx + one activity row; no-op update writes nothing; validation (blank title/name, priority 1-5, capacity > 0, estimate >= 0) |
| `projects.rs` extra | `create` generates the handle from the title when none is given (validated; must be free among active projects; error names the owner), `update` re-checks it, `archive(id, TaskDisposition)` = one tx that archives the project and either archives its active tasks or clears their `project_id` (inbox; `updated` row each) |
| `tasks.rs` extra | `create` / `create_many` (one tx, all-or-nothing) also create the `assigned_to` edge from `CreateTask.assignee` (`Me` = self person if one exists; unknown person fails the whole create); `set_assignee(task, Option<person>)` replaces the assignment in one tx (same person = no-op) |
| `settings.rs` | `get` (defaults for missing keys), `update` (all-or-nothing; hours_per_day must be 0 < h <= 24; `theme` must be 1-40 chars of lowercase letters, digits, hyphens) |
| `people.rs` extra | `get_self`, `ensure_self(name)` (blank -> "Me", idempotent) |
| `tasks.rs` | `update` sets/clears `completed_at` when status enters/leaves `done` |
| `nodes.rs` | generic over NodeType: `list_summaries(node_type)` (active, by label), `archive` / `archive_in_tx` (archives touching edges; one activity row each), `unarchive` (restores edges archived at the same timestamp unless other end archived), `delete` (only if archived; removes edges; keeps history), `summary(node)`; archive/delete **refuse the self person** |
| `edges.rs` | `get`, `list_for_node(id, include_archived)`, `list_active`, `list_active_of_type`, `update_attrs(id, attrs)` (one `updated` row on the from node, key `"<type> link"`; no-op if unchanged), `add` (revives archived edge with same (type,from,to); errors DuplicateEdge/NotFound/Invalid if an end is archived), `remove` (soft), `links_for_node` -> `EdgeLink` |
| `activity.rs` | `list_for_node`, `list_recent(limit)`, `count`; internal `record`, `record_created`, `diff` |
| `views.rs` | `people_rows`, `person_detail`, `person_archive_preview`, `team_rows` (tree order, depth), `team_detail`, `objective_rows` (with contributor counts, unsorted), `objective_detail` (projects first), `project_rows` (owner, objectives, task/done counts, unsorted), `project_detail`, `project_archive_preview`, `task_rows` (project + assignee summaries, unsorted), `task_detail` |
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
| add_edge | edges.rs | `fn:check_new_edge` (core edge_rules + cycles; `relates_to` rejected as `duplicate` if the reverse link exists) then edges::add | person panel (teams), objective panel (contributions), ui:LinksEditor (any relation) |
| remove_edge | edges.rs | edges::remove | person panel, objective panel, LinksEditor |
| set_manager | edges.rs | `fn:set_manager_impl` (validate, remove old, add new) | person panel |
| update_edge_attrs | edges.rs | store::edges::get, core `validate_attrs`, store::edges::update_attrs | ui:Contributions (weight), PersonPanel (team role), LinksEditor attribute inputs |
| list_link_options | edges.rs | core `edge_rules::link_options(node_type)` | ui:LinksEditor (relation picker + attribute controls) |
| list_node_summaries | nodes.rs | store::nodes::list_summaries | objective picker |
| list_objectives | objectives.rs | store::views::objective_rows then `core::objectives::arrange` | ui:Objectives |
| get_objective / get_objective_detail | objectives.rs | objectives::get / views::objective_detail | ui:ObjectivePanel |
| create_objective / update_objective | objectives.rs | objectives::create / update | ui:Objectives form / ObjectiveFields |
| archive_objective | objectives.rs | nodes::archive | ui:ArchiveObjective |
| list_projects | projects.rs | views::project_rows, then core `projects::filter` and `by_objective` / `by_status` per layout | ui:Projects |
| get_project / get_project_detail | projects.rs | projects::get / views::project_detail | ui:ProjectPanel |
| create_project / update_project | projects.rs | projects::create / update | ui:Projects form / ProjectFields (board drag also calls update_project) |
| preview_archive_project / archive_project | projects.rs | views::project_archive_preview / `projects::archive(id, disposition)` | ui:ArchiveProject |
| list_tasks | tasks.rs | views::task_rows, then core `tasks::filter` + `sort` | ui:TaskList |
| get_task / get_task_detail | tasks.rs | tasks::get / views::task_detail | api::get_task unused by UI (parity); ui:TaskPanel uses detail |
| create_task / update_task | tasks.rs | project must be active, then tasks::create / update | ui:TaskList form / rows, TaskFields |
| set_task_estimate | tasks.rs | settings::get + core `parse_estimate`, then tasks::update (empty text clears) | ui:TaskFields |
| set_assignee | tasks.rs | tasks::set_assignee | ui:TaskList rows, TaskFields |
| archive_task | tasks.rs | nodes::archive | ui:ArchiveTask |
| parse_task_lines / create_tasks_bulk | tasks.rs | core `parse_lines` / tasks::create_many (project active check) | ui:TaskList paste preview |
| get_settings / update_settings | settings.rs | settings::get / update | ui:Settings |

Tauri config: `app.withGlobalTauri: true`, `decorations: false` (custom title bar), `dragDropEnabled: false` (so HTML5 drag-and-drop works on the project board; Tauri's file-drop handler would swallow it on Windows), CSP `default-src 'self'` (+ `connect-src ipc:`), devUrl `localhost:1420`, frontendDist `../ui/dist`, identifier `app.minimap.desktop`. Permissions are an allow-list (app manifest) plus `core:window:*` for the title bar (minimize, toggle-maximize, internal-toggle-maximize, is-maximized, close, start-dragging, start-resize-dragging); generated files in `src-tauri/permissions/autogenerated/` and `gen/` are build output.

## 8. UI (`crate:minimap-ui`, `ui/src/`)
- Entry: `main.rs` -> `app.rs::App` (provides contexts, `<Router>`, `Shell`). `Shell` = TitleBar over (Sidebar | `<Routes>` | DetailPane), plus ToastHost and FirstRun; installs `keyboard::use_global_shortcuts()`; clears `ListNav` on route change.
- Routes: `/` Overview; `/objectives` Objectives; `/projects` Projects; `/tasks` Tasks; `/inbox` Inbox (same `TaskList`, `no_project`); `/settings` Settings (hours per day only); `/people` People; `/teams` Teams; `/notes /decisions /waiting-on` = `Placeholder` (name the spec); `/:type/:id` DeepLink (selects the node, redirects to its list). `/this-week` and `/weekly-review` are hidden (`enabled:false` in `nav::NAV`).
- Contexts (`state.rs`): `Selection(RwSignal<Option<NodeRef>>)` (open node in pane), `ListNav` (rows + cursor for j/k/Enter; `set_items` keeps cursor; `on_new(f)` fires on the `n` key; `on_row_key(f)` fires for row shortcuts x/s/d/a/1-5 pressed on the cursor row, ignoring presses from before mount), `Toasts` (`error(&AppError)`, `info`), `DataVersion` (`track()` inside resource closures, `bump()`), helper `finish(result, toasts, version)` (bumps always, toasts errors).
- Data flow for writes: handler -> `spawn_local(api::x)` -> `finish(...)` -> `DataVersion` bump -> version-keyed `LocalResource`s reload (lists, pane Links/Activity/summary). Fields resources are **not** version-keyed so typing isn't overwritten.
- `api.rs`: one typed async fn per command; `invoke` serializes args with maps-as-objects (else JS `Map` -> `{}`), maps errors to `AppError` (code `ipc` for bridge failures). Mirrors §7.
- Pages (`pages/`): `overview` (ping demo), `objectives` (group-by-quarter toggle; keyboard row index runs across groups), `projects` (List/Board toggle, status/owner/objective filters, New project form; board = HTML5 drag a card to another column -> `update_project` status; keyboard rows are list order or column-major), `tasks` (`Tasks`, `Inbox`), `settings` (Appearance theme picker with swatch cards + hours per day), `people`, `teams`, `placeholder`, `deep_link` (+`NotFound`).
- Components (`components/`): `sidebar` (from `nav::NAV`), `links_editor` (generic Links section: groups by relation heading, per-link attribute inputs generated from `AttrSpec`s, remove, "Add a link…" = relation select then name search over `list_node_summaries` of the allowed types; hides/does not offer relations the node's own panel edits via `kind_edited_elsewhere`), `detail_pane` (header, `PersonPanel`/`TeamPanel`/placeholder, Links grouped by `link_heading`, Activity via `describe`; `edited_elsewhere` hides links the panel edits (person: teams/manager; team: members; objective: contributors; project: objectives + dependencies; task: assignee); split view >= 1100px, overlay below), `people_panel` (`PersonPanel`, fields, Organization: manager + teams/roles, waiting-ons, `ArchivePerson`; helpers `NodeButtons`, `team_options`, `indented`, `error_line`), `titlebar` (custom 32px title bar: drag region, minimize/maximize/close, Linux resize handles; uses `window.rs`), `task_list` (filters: text, status, project, assignee, due range, show done; inline status/priority/project/assignee/due controls per row; row keys x toggle done, s next status, 1-5 priority, d/a focus the due/assignee control; new-task textarea, Enter adds, pasted multi-line text shows a preview then `create_tasks_bulk`), `task_panel` (all fields, estimate text via `set_task_estimate`, project, assignee, archive), `project_panel` (fields incl. handle, dates, status, priority, owner; Objectives with weight; Dependencies (depends_on, loops rejected by the backend); read-only Tasks; `ArchiveProject` with the tasks choice), `objective_panel` (exports `WeightInput`, shared with the project panel; fields incl. "Your assessment" + priority, Contributions with weight edit + picker, `ArchiveObjective`), `team_panel` (fields, Structure: parent + sub-teams, Members, `ArchiveTeam`), `node_row` (list row using `ListNav`/`Selection`), `form` (`TextField` commit-on-change, `SelectField`, class consts `INPUT/BUTTON/BUTTON_PRIMARY/BUTTON_DANGER`), `toasts`, `first_run`.
- `window.rs`: wasm-bindgen wrappers over `window.__TAURI__.window` (`minimize`, `toggle_maximize`, `close`, `is_maximized`, `start_resize`), `Platform::detect()` from the user agent, `ResizeDir`. The native title bar is off (`decorations:false`); macOS overlay config in `src-tauri/tauri.macos.conf.json` (ADR-0004).
- `labels.rs` (pure presentation): objective, project and task status labels, `estimate_text`, priority labels, `humanize`.
- `nav.rs` (pure, unit-tested): `NAV` table (label, path, chord, enabled), `chord_target`, `list_path`, `type_label`, `move_cursor`, `is_row_key`, `is_typing_target`. `keyboard.rs`: `g`+letter chords (1s window), `j/k/Enter` on lists, `n` new item on the screen, row keys, `Esc` closes pane; ignored while typing and for Ctrl/Cmd/Alt.
- Chords: o Overview, i Inbox, w This week*, b Objectives, p Projects, t Tasks, e People, m Teams, n Notes, d Decisions, a Waiting on, r Weekly review*, s Settings (*hidden).
- Style/themes: `ui/style/input.css` holds the DEFAULT (dark) token values as CSS vars, mapped to Tailwind utilities (`bg-canvas bg-panel bg-hover bg-active border-line text-fg text-muted text-faint text-danger bg-scrim`). `ui/src/themes.rs` = 17 built-in themes as data (`Theme{id,name,kind,tokens}`, `ALL`, `find`, `resolve(id, os_dark)`, `SYSTEM_ID`; tests enforce contrast and that input.css matches the default). `ui/src/theme.rs` = `ThemeCtx` (selected id signal, OS dark signal via matchMedia, `current()`, `select(id)` caches to localStorage `minimap.theme`; applies tokens + `color-scheme` + `data-theme` on `<html>`). `App` loads `get_settings` and calls `select`; Settings → Appearance picks (applies at once, then `update_settings{theme}`). **No raw palette colours, no `dark:` variants, no shadows** (`docs/design.md`). Titlebar logo switches dark/light variant by theme kind; Trunk copies the logo files.
- `view!` macro gotchas: closures with `match` bodies and method chains must be named variables outside the macro; `ts!`-style macros can't expand to struct fields.

## 9. Key flows
- **First run**: UI `FirstRun` -> `get_self_person` None -> prompt -> `create_self_person` -> `DataVersion` bump.
- **Group objectives**: UI toggle -> `list_objectives(grouping)` -> store rows -> `core::objectives::arrange` -> groups labelled "Q1 2027" ... "No date".
- **Move project on the board**: card `dragstart` (sets `dataTransfer`, WebKit needs it) -> column `drop` -> `update_project{status}` -> activity `updated` row -> `DataVersion` bump.
- **Archive project**: `preview_archive_project` (active tasks) -> choose "archive tasks too" or "move to inbox" -> `archive_project` (single tx).
- **Paste a task list**: textarea with newlines -> `parse_task_lines` -> preview (needs > 1 line) -> `create_tasks_bulk` (one tx, default assignee Me, project = the project filter unless in the inbox).
- **Estimate**: text field -> `set_task_estimate` -> core `parse_estimate` with the stored hours-per-day -> `estimate_days` (existing estimates never rewritten when the setting changes).
- **Contribution weight**: UI input -> `update_edge_attrs` -> core `validate_attrs` (0-1) -> `edges::update_attrs` (+activity).
- **Add link**: UI -> `cmd:add_edge` -> `check_new_edge` (matrix, attrs, self-edge, cycle for blocks/depends_on/reports_to, error text lists node names: "Can't add this link: this would create a loop — A → B → A") -> `edges::add` (tx + activity).
- **Generic link**: pane Links -> `list_link_options(node_type)` -> choose relation -> search candidates (`list_node_summaries` per allowed type, minus self and already-linked) -> `add_edge` (rules + cycle check run in the command; a loop toast names the path) -> `DataVersion` bump. Attribute inputs -> `update_edge_attrs`.
- **Archive person**: `preview_archive_person` (active assigned tasks) -> confirm -> `nodes::archive` (person + edges in one tx; self refused).
- **Nest team**: `update_team` with `Patch::Set(parent)` -> `check_new_parent` (parent active, no loop incl. archived teams) -> `teams::update`.

## 10. Invariants -> where enforced -> where tested
| rule | enforced in | tested in |
|---|---|---|
| exactly one self person | unique index + `ensure_self` + `create` check | store tests `exactly_one_self_person`, `self_person_cannot_be_archived_or_deleted` |
| self can't be archived/deleted | `store::nodes::ensure_not_self` | same |
| edge matrix + attrs | core `edge_rules` (called by `check_new_edge`) | core `matrix_matches_spec` (exhaustive), command tests |
| no cycles (blocks/depends_on/reports_to) | core `cycles` via `check_new_edge`/`set_manager_impl` | core proptest, command tests |
| `relates_to` is symmetric (A-B and B-A are one link) | `check_new_edge` | `relates_to_links_anything_to_anything_once` |
| task and project loops name the nodes ("A → B → A") | `check_new_edge` with core `find_cycle` | `task_loops_are_rejected_with_the_task_titles`, `project_dependency_loops_are_rejected` |
| link options == matrix; attribute schema drives validation | core `edge_rules::link_options/attr_schema` | `link_options_are_exactly_what_the_matrix_allows`, `schema_matches_the_matrix_documentation` |
| no team-nesting cycles | `check_new_parent` | `commands/teams.rs` tests |
| objective list order + quarter grouping | core `objectives::arrange` | core objectives tests |
| project handle unique among active, valid format | store `projects::create/update` + unique index 0004; format in core `slug::validate` | store `project_handles_*`, core slug tests (+proptest), migration backfill test |
| project archive moves/archives tasks atomically | `store::projects::archive` | `archiving_a_project_can_*`, `project_archive_is_atomic` |
| project list/board ordering, filtering, grouping | core `projects` | core projects tests |
| new tasks default to assignee = self; bulk create is all-or-nothing | store `tasks::create/create_many` | `new_tasks_are_assigned_to_me_by_default`, `create_many_is_all_or_nothing` |
| one assignee per task; failed change keeps the old one | store `tasks::set_assignee` | `set_assignee_replaces_the_assignment` |
| estimate text -> days uses hours-per-day; hours-per-day in (0, 24] | core `parse_estimate`, store `settings::update` | core tasks tests (+proptest), `estimates_follow_the_hours_per_day_setting`, `settings_have_defaults_and_are_validated` |
| inbox = open tasks without a project; list order due then priority | core `tasks::filter/sort` | core tasks tests |
| edge attrs validated before save | `cmd:update_edge_attrs` via core `validate_attrs` | `edge_attrs_are_validated_before_saving` |
| archive cascades to edges; unarchive restores | `store::nodes` | store tests + proptest on activity counts |
| hard delete only after archive | `store::nodes::delete` | store tests |
| every write -> activity in same tx | each repo fn | store tests (`create_writes_one_created_activity...`, proptest) |
| no-op update writes no activity | repo `update` fns | `noop_update_writes_no_activity` |
| CHECK/FK integrity | migration 0002 | `invalid_input_is_rejected...`, `delete_of_referenced_node...` |

## 11. Tests inventory
`cargo test --workspace`: core 36 (edge_rules + link options, cycles + proptest, objectives, slug + proptest, projects, tasks + proptest), types 4, store 4 unit + 41 integration (`crates/minimap-store/tests/repos.rs`, incl. proptest `activity_count_matches_writes`), src-tauri 14 (`commands/edges.rs`, `commands/teams.rs`), ui 26 (`links_editor.rs`, `themes.rs`, `nav.rs`, `labels.rs`, `window.rs`, `form.rs`, `detail_pane.rs`, `people_panel.rs`, `objective_panel.rs`). No UI/browser tests; UI behaviour is verified by hand (checklists in specs 02, 03). Gate: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p minimap-ui --target wasm32-unknown-unknown -- -D warnings`, `cargo test --workspace`, `cd ui && trunk build`.

## 12. Feature specs (`docs/features/NN-*.md`; index in its README)
| # | feature | status | touches / notes |
|---|---|---|---|
| 01 | data model + activity | Done | store, types |
| 02 | app shell + navigation | Done | ui shell, node-summary/edges/activity cmds |
| 03 | people + teams | Implemented, manual check pending | people/team cmds, views, UI screens/panels, core cycles+edge_rules |
| 04 | objectives | Implemented, manual check pending | objective cmds/screen/panel, quarter grouping (core), `update_edge_attrs`, `list_node_summaries`; computed-health marker waits for 15 |
| 05 | projects | Implemented, manual check pending | handle (slug), list by objective + board with drag, filters, project panel, archive with task choice; read-only task list until 06 |
| 06 | tasks + inbox | Implemented, manual check pending | tasks list/inbox with filters, inline editing and keyboard shortcuts, paste-a-list preview, estimates (`3d`/`4h`) with the hours-per-day setting (new `settings` table + minimal Settings screen), default assignee = me; no subtasks (decision); `blocks` editing still waits for 07 |
| 07 | edges + cycle detection | Implemented, manual check pending | core matrix/attr schema/cycles/link options; add/remove/update-attrs/set_manager/list_link_options commands; generic Links editor in the pane; `relates_to` in the MVP (symmetric) |
| 08-12 | waiting-on, notes, decisions, search, palette/quick-add | Draft | |
| 13-18 | CPM, impact, health/overview, this-week, capacity, graph view | Draft | core algorithms |
| 19-27 | review/export, backup, encryption, Drive, settings, demo data, undo, data export, recurring | Draft | | (spec 23: the hours-per-day setting, the theme picker and a minimal Settings screen already exist)
ADRs: 0001 versions + command allow-list, 0002 network allowed + Drive backup, 0003 extra activity actions + `Patch<T>` + completed_at, 0004 custom title bar + window permissions. Design: `docs/design.md`.

## 13. Conventions and gotchas
- No `unwrap/expect` outside tests/main setup (exceptions: `build.rs`). Domain rules in core; commands thin; UI has no business rules; all SQL in store, parameterized (table/column names are compile-time constants only).
- Every write is a transaction that also writes `activity`.
- Specs: implement only `Status: Ready` (user may override by asking); record answers to open questions + deviations in the spec/an ADR.
- Commits: user commits; end messages with the attribution line from the session system reminder.
- Full-height overlays (detail pane scrim) start at `top-8`, below the title bar.
- `ui/dist/` and `src-tauri/gen/` are build output (in `.gitignore`; `gen/` was committed earlier and may still be tracked).
- WSL: `libEGL`/`MESA` warnings from WebKitGTK are harmless.
- Edge attribute changes (team role, contribution weight) go through `update_edge_attrs`, one `updated` row on the from node. The editor's attribute controls come from `edge_rules::attr_schema`, so a new attribute is one schema entry.
- Priority: 1 = highest everywhere.
- Anything the UI needs that is a business rule (sorting, grouping) is computed in core and returned by a command; the UI crate can't depend on core.
- Migrations that need data fixes use `M::up_with_hook` (runs after the SQL, same tx); the app DB is migrated on launch (there is no pre-migration backup yet, spec 20).
- HTML5 drag-and-drop in the webview needs `dragDropEnabled: false` and `dataTransfer.setData` on dragstart.
- No subtasks (decision in spec 06): break work into tasks and sequence with `blocks`; a Markdown checklist in descriptions may come later.
- Row shortcuts (`x s d a 1-5`) and `n` are global keys that screens opt into via `ListNav::on_row_key` / `on_new`; screens that don't register ignore them.
- Tailwind classes live in Rust source; Trunk's Tailwind scans `.rs` files.
- Themes: dark is the default (input.css + window `backgroundColor` + `settings.theme`); no theme JS (CSP), so a non-default theme can flash dark for a moment at launch (cache narrows it). Palettes are adapted; `fg`/`muted`/`danger` of several are nudged to pass the contrast tests.

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
