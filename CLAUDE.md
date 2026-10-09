# Minimap

> **Start with [`docs/knowledge-graph.md`](docs/knowledge-graph.md)**: the map of what exists in the code (crates, modules, commands, tables, UI, rules and where they're tested, spec status). Read it before searching the codebase, open only the files it points to, and **update it in the same change whenever you change code** (its §14 lists what to update for each kind of change).
>
> **The in-app Help wiki ([`docs/help/`](docs/help/), shown under Help in the app) is the product's user documentation. Every change to the codebase is checked against it, and changes the user can see or feel update it in the same change.** See §11 for the rule and §13 for the working agreement.

Local-first, private desktop app: a command center for CTOs/CXOs/PMs. Models objectives, projects, tasks, people and teams as a typed graph (dependencies, critical paths, capacity, downstream impact of slips).

## 1. Hard constraints (never violate)
- **No Node.js, npm, yarn, pnpm, bun or Electron** in build, tooling or runtime. If a library assumes npm, find another way.
- **Local-first, single user.** No server, accounts or login. People are records, not users.
- **No telemetry or analytics.** Network features (e.g. Google Drive storage, importers) are allowed; user data only leaves the device to a destination the user has connected. See ADR-0011.
- **All structured data in one SQLite file** in the OS app-data dir, with an encryption option (SQLCipher). Attachments (media) are blobs stored alongside: on Google Drive plus a local cache (ADR-0011).
- **Rust everywhere.** Backend, domain logic, frontend (WASM).

## 2. Stack
Tauri 2.x (system webview) · Leptos CSR (stable 0.8.x) built with Trunk (`wasm32-unknown-unknown`) · Tailwind via Trunk's built-in support (or plain CSS) · `rusqlite` with `bundled-sqlcipher-vendored-openssl` (sync; call from commands via `tauri::async_runtime::spawn_blocking`) · `rusqlite_migration` (plain SQL via `include_str!`) · `petgraph` · `uuid` v7 (TEXT) · `time` (ISO-8601 UTC TEXT; dates `YYYY-MM-DD`) · `thiserror` in libs, `anyhow` only at app edge · `serde`/`serde_json` · `keyring` (DB passphrase) · `tracing` (file log in app-data; never log note bodies/user content at info) · tests: `#[test]`, `proptest`, `insta`.
Check crates.io for current stable versions; pin in workspace `Cargo.toml`.

**Bridge:** `app.withGlobalTauri: true`; `ui/src/api.rs` has a hand-written `wasm-bindgen` wrapper over `window.__TAURI__.core.invoke`, one typed async fn per command. All request/response types come from `minimap-types`.

## 3. Layout & dependency rules
`crates/minimap-types` (DTOs/enums, no IO, wasm-compatible; depends on nothing internal) · `crates/minimap-core` (domain + graph algorithms, no IO/SQLite; depends only on types) · `crates/minimap-store` (SQLite, migrations, repos, snapshot + merge for sync; depends on types, core only for validation) · `crates/minimap-sync` (Google Drive, OAuth, encryption of what is uploaded, the sync engine; no SQL; depends on types, core, store; ADR-0011) · `src-tauri` (commands in `src/commands/` one module per area, state, wiring; capabilities/) · `ui` (Leptos+Trunk; depends **only** on `minimap-types` — never rusqlite/petgraph/tauri) · `docs/decisions/` (ADRs) · `docs/quick-add-grammar.md` · `docs/progress.md`.
Trunk↔Tauri: `beforeDevCommand: trunk serve --config ui/Trunk.toml`, `beforeBuildCommand: trunk build --release --config ui/Trunk.toml`, `devUrl: http://localhost:1420`, `frontendDist: ../ui/dist`.

## 4. Domain model
Every node: `id` (uuid v7), `created_at`, `updated_at`, `archived_at` (soft delete).
- **Objective**: title, description, target_date, status (on_track/at_risk/off_track/done), priority 1–5, ongoing (no end: no target_date, never done, ADR-0014), review_every_days, last_reviewed_on
- **Project**: title, slug (unique handle among active projects, e.g. `api-launch`), description, owner_person_id, start_date, target_date, status (planned/active/paused/done/cancelled), priority
- **Task**: title, description, project_id (nullable), status (todo/in_progress/blocked/done/cancelled), estimate_days (decimal), start_date, due_date, completed_at, priority, task_type (optional id of an entry in the user's task-type list in Settings: design, decision, bug... spec 32; the built-in `meeting` type makes it a meeting: `due_date` is the day, `start_minute` the start on the user's clock and `length_minutes` the length, all required for a meeting; it starts and ends itself, spec 38, ADR-0018)
- **Person**: name, role_title, email?, weekly_capacity_hours (default 40), is_self (exactly one), notes
- **Team**: name, description, parent_team_id (nestable)
- **Note**: title, body (Markdown), note_date, kind (one_on_one/meeting/general)
- **Decision**: title, context, decision, rationale, decided_on, status (proposed/decided/superseded)
- **WaitingOn**: description, person_id, asked_on, expected_by, follow_up_on (snooze until), resolved_on

### Edges (single `edges` table)
Columns: id, edge_type, from_type, from_id, to_type, to_id, attrs (JSON, default '{}'), created_at, archived_at; `UNIQUE(edge_type, from_id, to_id)`; indexes `idx_edges_from(from_id, edge_type)`, `idx_edges_to(to_id, edge_type)`.
Matrix (enforce in core, reject anything else):
| type | from → to | attrs |
|---|---|---|
| blocks | Task → Task | lag_days (cross-project ok) |
| depends_on | Project → Project | note |
| contributes_to | Project/Task → Objective | weight 0–1 |
| assigned_to | Task → Person | allocation_pct 1–100 |
| member_of | Person → Team | role (lead/member) |
| reports_to | Person → Person | — |
| relates_to | any → any | note |
| mentions | Note → any | — |
| affects | Decision → Project/Task/Objective | — |
| about | WaitingOn → Task/Project | — |
| supersedes | Decision → Decision (newer → older) | — |
| subtask_of | Task → Task (sub-task → parent) | — (organisation only: no effect on dates or the schedule; ADR-0017) |
| follows_up | Task → Task (meeting → the meeting it follows up; both meetings) | — (no effect on dates; spec 38, ADR-0018) |

Invariants (enforced in core, tested): no cycles in `blocks`, `depends_on` (report the cycle path), `supersedes`, `reports_to`, or team nesting; no self-edges; a task is a sub-task of at most one task and the tree is one level deep (`subtask_of`, spec 33); a meeting follows up on at most one meeting, both ends are meetings and the chain has no loop (`follows_up`, spec 38); archiving a node archives its edges; hard delete only after archive, with UI confirmation.

### Activity log
`activity(id, at, node_type, node_id, action [created/updated/archived/edge_added/edge_removed], diff JSON {field:[old,new]})`. Every write goes through a repository method that appends to `activity` in the same transaction.

## 5. Core algorithms (pure, in `minimap-core`)
1. Graph build into `petgraph::StableGraph` (id→index map).
2. Cycle check before inserting blocks/depends_on/reports_to; return the cycle path.
3. CPM schedule + critical path: working days Mon–Fri; duration = estimate_days or 1 (flag "unestimated"); forward/backward pass (backward from project target date or latest finish); slack = LS − ES; critical = zero slack; done tasks fixed at actual dates; per project and portfolio-wide.
4. Impact analysis: task/project + slip of N working days propagated along blocks/depends_on consuming slack; output affected tasks (new projected finish, slip absorbed), projects, objectives (contributes_to), people (assigned_to).
5. Health scoring (computed): project from projected finish vs target, blocked/overdue share, unestimated work; objective rolls up weighted; expose reasons ("3 tasks overdue; projected 6 days late").
6. Capacity per person/week: sum(allocation_pct × hours) of active assigned tasks scheduled that week ÷ weekly_capacity_hours; flag >100%.
Each needs unit tests on small hand-built graphs and `proptest` for: no cycles after accepted insert, slack ≥ 0, impact never moves a task earlier.

## 6. Tauri commands (`src-tauri/src/commands/`)
Thin: load, call core, persist, return. Take/return `minimap-types`; return `Result<T, AppError>` (`{code, message}`).
- nodes: create_/update_/archive_/get_/list_* per type (filters: status, project, person, team, date range, text)
- edges: add_edge, remove_edge, list_edges_for(node_id)
- graph: get_dependency_graph(scope), get_schedule(scope), get_critical_path(scope), run_impact_analysis(slips) (a scenario of one or more task/project slips), preview_apply_slips(slips), apply_slips(slips) (ADR-free: spec 14)
- dashboard: get_portfolio_overview, get_capacity(from,to), get_waiting_on(open_only)
- review: get_weekly_review(week_start), render_report(report_kind, params)
- quick_add: parse_quick_add(text) → preview; commit_quick_add(text)
- export: export_markdown(report_kind, params, path)
- settings: get_settings, update_settings, get_data_info, show_data_folder, set_db_passphrase, backup_now(path)
- undo: undo_last, redo_last (spec 25: Ctrl/Cmd+Z; built from the activity log, session only)
- recurring: set_recurrence(node, text, template?) (spec 27: tasks make the next one when finished; notes on their date; quick-add `every:`)
- export: export_all(path, format) (spec 26: JSON files per node type + edges + activity + manifest into a new folder, optional Markdown)
- drive (ADR-0011): get_sync_status, update_sync_settings, connect_drive, get_recovery_key, finish_drive_connect, cancel_drive_connect, disconnect_drive, sync_now, list_drive_checkpoints, recover_checkpoint; attachments: list_attachments, add_attachment, add_attachment_data, remove_attachment, open_attachment (+ the `minimap-media` protocol)
Capabilities: frontend may call only these commands (app manifest) plus dialog/fs permissions export/backup need, plus the minimal window-control permissions the custom title bar needs (ADR-0004). Nothing broader.

## 7. UI (Leptos)
Keyboard-first, dense, calm; dark by default, switchable colour themes (Settings → Appearance; palettes are data in `ui/src/themes.rs`). Visual style: flat, IDE-like, grey with one accent colour and sparing status colours; follow `docs/design.md` (theme tokens only, no raw colours). This week is the landing screen (overdue, due this week, blocked, my work in progress, stale waiting-ons, 1:1s; Monday-Sunday). Overview = computed health (thresholds in Settings), top-5 risks, overloaded people, stale waiting-ons. Sidebar: Overview, Objectives, Projects, Tasks, People, Teams, Notes, Decisions, Waiting On, Weekly Review, Settings, About. Command palette (Ctrl/Cmd+K): navigate, actions, quick-add. Right-side detail pane for any node (fields, edges grouped by type & editable, activity).
Screens: (1) Overview: objectives w/ computed health, projects under them, top-5 risks, overloaded people, stale (>7d) waiting-ons; (2) Projects: list+board, detail with tasks, timeline w/ critical path, deps in/out; (3) Dependency graph: SVG from Rust, layered L→R (Sugiyama-style layering in core), filters project/team/objective, critical path highlighted, click opens detail; (4) Impact analysis ("What if this slips?" from any task); (5) People: capacity heatmap people×weeks, person detail; (6) Notes/decisions: Markdown editor, `@` links person/project/task (creates `mentions`), `[ ]` lines → tasks; (7) Waiting on: sorted by age, one-click resolve; (8) Weekly review: slipped, blocked, overloaded, stale waiting-ons, decisions; ends with "Export as Markdown status report"; (9) Settings: db location, encryption + passphrase, backup folder, working days, default capacity, theme.

### Quick-add grammar (full doc in `docs/quick-add-grammar.md`; parsing is pure, in core; UI always previews before commit)
```
task Fix login timeout @priya #api-launch !2 due:fri est:3d blocks:"Release 1.2"
project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU"
wait @raj on "Security review sign-off" by:next-wed
note 1:1 @priya
decision "Postgres over Mongo" affects:#api-launch
```
`@name` fuzzy person (ask if ambiguous; `@me` = self) · `#project` · `!1`–`!5` priority · `due:`/`by:`/`target:` natural dates (today, fri, next-wed, +3d, ISO; `fri` is today on a Friday, `next-wed` is that day in the next Mon-Sun week); no leading keyword = task · `est:` (3d, 4h) · `type:` a task type by name or id · `blocks:`/`for:`/`affects:` edges to a named node.

## 8. Storage, encryption, backup
DB at `<app_data_dir>/minimap.db`; `PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;`. Encryption off by default for M1–M3, added in M4: random key in the OS keychain via `keyring`, or a user passphrase (never a stored key); re-keying is a verified export-and-swap because SQLCipher can't encrypt in place (ADR-0010). `backup_now` uses SQLite online backup API → timestamped copy; optional daily auto-backup (keep 14). **Google Drive storage** (ADR-0011, spec 22): optional; once connected, every change autosaves an encrypted snapshot of the local DB to Drive (one file per device, 5 s idle, at most 30 s) and devices merge each other's snapshots automatically (row-level, newest `updated_at` wins, hard deletes via tombstones, invariants repaired and reported); no locks, no conflict dialogs. Hourly/daily checkpoints; attachments are encrypted content-addressed blobs on Drive with a local LRU cache. Everything uploaded is encrypted with a vault key (recovery key shown once). Without Drive the user is warned that data is on this device only. Desktop OAuth: loopback redirect + PKCE, `drive.file` scope only, refresh token in the OS keychain via `keyring` (local DB fallback); network calls happen in the Rust backend (`minimap-sync`), never the webview. The live DB never sits in a synced folder. Migrations run on startup in a transaction after a pre-migration backup.

## 9. Milestones (one at a time; each ends with tests passing, `cargo clippy --workspace -- -D warnings` clean, working `cargo tauri dev`)
- **M0 Scaffold**: workspace, Tauri 2 + Leptos + Trunk + Tailwind; `ping` command shown in UI; SQLite opens + first migration; app-data path on Win/macOS/Linux; README w/ prerequisites (no npm). Done when `cargo tauri dev` shows "pong" and `cargo tauri build` produces an installer.
- **M1 Core data**: all node tables, edges, activity, repos + tests; CRUD commands + list/detail screens; sidebar, detail pane; self person on first run.
- **M2 Graph engine**: cycle detection w/ readable UI error; CPM, critical path, dependency graph view; impact analysis. Done when the seeded demo (3 projects, ~40 tasks, cross-project blocks) highlights the critical path correctly and a 5-day slip shows right downstream changes; `insta` snapshots.
- **M3 People & exec layer**: capacity + heatmap; health with reasons + Overview; waiting-on, notes with `@`, decisions; command palette + quick-add (all section-7 examples work).
- **M4 Review, export, security**: weekly review + Markdown export; SQLCipher w/ keychain key, backup + auto-backup, Google Drive storage with autosave and attachments; Settings. Done when encrypting an existing DB loses no data, a local backup and a Drive checkpoint restore, a second device picks up the Drive data, and the review exports a readable report.
- **Later (do not build)**: PDF export (Typst), holidays, Jira/Linear/GitHub read-only importers, sync, shared snapshots, local AI summary.

## 10. Demo data
`seed_demo_data` command (debug builds only, from Settings): 2 objectives, 3 projects (one at risk), ~40 tasks with cross-project blocks, 8 people in 2 nested teams with reporting lines (one overloaded), a few notes/decisions/waiting-ons (one stale). Used for manual testing and snapshot tests. Lives in `crates/minimap-store/src/demo.rs` (`seed`), with `insta` snapshots in `src-tauri/src/commands/demo.rs`; details in `docs/features/24-demo-data.md`. `seed` also records the ids it created (`local.demo_items`) so **Remove demo data** (`get_demo_status` / `remove_demo_data`, Settings → Data & backup, every build, backup first, never touches non-demo items; `store::demo_remove`) is exact: a new node type or table needs a line in `demo_remove` (`ORDER`, `labelled`, delete order) as well.
**Keep it in step with the schema.** Whenever a change adds a table, a column, a node type, an edge type or an attribute (a migration, a new field on a `Create*` type, a new edge/attr in the matrix), update the demo data **in the same change**: fill the new field or table with realistic, varied values in `seed` (so every screen that shows it has something to show), update the counts in `DemoSummary`, the store tests in `crates/minimap-store/tests/demo.rs` and the Developer card text, then review the snapshots (`INSTA_UPDATE=always cargo test -p minimap demo`, read the `.snap` diff; the story must still hold: EU Region amber, one overloaded person, one stale waiting-on) and fix `docs/features/24-demo-data.md` and `docs/knowledge-graph.md`. A change that makes the seed fail to compile or the demo tests fail is not done. If a new column genuinely needs no demo value (a purely internal flag), say so in the commit message.

## 11. Conventions
- `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- No `unwrap()`/`expect()` outside tests and `main` setup.
- Domain logic in `minimap-core`; commands thin; UI has no business rules.
- All SQL in `minimap-store`, parameterized only. Every write is a transaction that also writes the activity row.
- ADR in `docs/decisions/` for any significant architectural choice or deviation from this file.
- Undo (spec 25) works from the activity log, so **every write goes through a repository function that logs it** (already the rule) and a new kind of row or diff key must be considered in `minimap_core::undo` (undoable, ignored or irreversible). A new nullable field on an `Update*` type goes in `store::undo::nullable_fields` (a test enforces it).
- The full export (spec 26) is everything the user put in the database: a **new table or node type** is added to `DataExport` (types), `store::export::collect`, the files `commands/export.rs` writes, and the README text in `core::export_md::readme`; a new column on an existing node type is exported automatically (the structs are the files), but check `export_md` if it should read well in Markdown.
- **The in-app Help is documentation of the product** (`docs/help/*.md`, shown under Help; 29 pages indexed by `PAGES` in `ui/src/help/mod.rs`). **Every change to the codebase triggers a check of the wiki**, before the work is reported done:
  1. Ask: does this change what a user can see, do, type, press, read or rely on? That includes a screen, control, shortcut, setting or default, a rule or computation (health, schedule, capacity, impact, recurrence, undo), a grammar or accepted text, an error message people will meet, what is stored, backed up, synced, encrypted or exported, a limit, and behaviour in an edge case a page describes.
  2. If yes, **edit the matching page(s) in the same change** (find them with `grep -rn "<word>" docs/help`; a new feature gets a page or a section, a removed one loses its text, a renamed screen or button is renamed everywhere in the wiki). A new page is a file in `docs/help/` plus a line in `PAGES`, and a link to it from a related page (a test requires every page to be reachable).
  3. If no (a refactor, a performance fix, a test, an internal rename), say so in your closing summary: "no Help change needed because ...". Do not skip the question.
  Tests catch some drift (every shortcut chord, screen, settings tab, report placeholder, quoted default, quick-add example and link is checked against the code), but they cannot tell whether prose is still true, so reading the pages your change touches is part of the change. Write the pages for a user, not a developer: say what to do and what will happen.
- **Changelog** ([CHANGELOG.md](CHANGELOG.md)): **Add to `[Unreleased]` only.** When you complete a spec, fix a bug, or add a feature that users will see or feel, add one line to the appropriate subsection (Added/Changed/Fixed) under `[Unreleased]`. The release manager moves the whole section to a version number and date when tagging. Format: one line, active voice, user-facing language. Link specs (`Spec 24`) or ADRs (`ADR-0012`) if notable. Do not edit version-numbered sections.
- Small commits, one logical change each.

## 12. Commands
```bash
rustup target add wasm32-unknown-unknown
cargo install tauri-cli --version "^2" --locked
cargo install trunk --locked
cargo tauri dev | cargo tauri build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```
Linux also needs Tauri system deps (webkit2gtk-4.1 etc.).

## 13. Working agreement
Start each session by reading this file, then `docs/knowledge-graph.md` (the code map), then `docs/progress.md`. **Keep `docs/knowledge-graph.md` and the Help wiki (`docs/help/`) in sync with the code**: the knowledge graph for developers, the wiki for users (§11 says how). For the graph: any change that adds, removes, renames or re-wires a crate, module, public function, type, table, command, route, component, rule or test count must update the matching section (see its §14) before the work is reported done; a schema change (table, column, node or edge type) also updates the demo data (§10); if the graph and the code disagree, trust the code and fix the graph. Feature specs live in `docs/features/` (index in its README); implement only specs marked `Status: Ready`, and when one conflicts with this file, follow the spec and record an ADR. One milestone at a time; don't start the next until "Done when" passes. If a section-1 constraint blocks something, stop and explain the trade-off. Check official docs when unsure of Tauri 2 / Leptos / Trunk APIs.
