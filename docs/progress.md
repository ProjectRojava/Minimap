# Progress

## Current milestone: M0 (Scaffold) — in progress

- [x] Workspace, crates, toolchain file
- [x] Tauri 2 shell with `ping` command, restricted to app commands via capability
- [x] SQLite open + first migration (+ tests)
- [x] Leptos UI + Trunk + Tailwind wiring, `api.rs`
- [ ] `cargo tauri dev` shows "pong" (needs Linux system deps: webkit2gtk-4.1, libsoup3, etc.)
- [ ] `cargo tauri build` produces an installer
- [ ] README, ADRs

## M1 progress
- [x] 01 Data model and activity log (store + types; no UI/commands yet)
- [x] 02 App shell and navigation
- [x] 03 People and teams (implemented; manual click-through pending, see spec). Also landed: edge matrix + cycle detection in core, `add_edge`/`remove_edge`/`set_manager` (part of 07)
- [x] 04 Objectives (implemented; manual click-through pending). Also landed: `update_edge_attrs`, `list_node_summaries`, quarter grouping in core
- [x] 05 Projects (implemented; manual click-through pending). Includes the board view, project handles (migration 0003/0004), archive with a tasks choice
- [x] 06 Tasks and inbox (implemented; manual click-through pending). Includes the hours-per-day setting (settings table + minimal screen), default assignee, paste-a-list preview, row shortcuts
- [x] Dark by default + theme system (ADR-0005): 17 built-in themes as data, Settings → Appearance, `settings.theme`
- [x] 07 Edges, linking and cycle detection (implemented; manual click-through pending). Includes `relates_to`, the generic Links editor, schema-driven attribute validation
- [x] 08 Waiting-on (implemented; manual click-through pending). Includes snooze (`follow_up_on`, migration 0006) and the stale-threshold setting; Overview/This week will consume it (15, 16)
- [x] 09 Notes and @mentions (implemented; manual click-through pending). Mentions stored as `@[Name](node:id)`, checklist conversion, safe Markdown preview, autosave with folded activity
- [x] 10 Decisions (implemented; manual click-through pending). Own node type; new `supersedes` edge (ADR-0006) marks the older decision superseded; deciding stamps today; decisions show under "Decisions" on affected nodes; a date-range filter is ready for the weekly review (19)
- [x] 11 Search (implemented; manual click-through pending). FTS5 index kept by triggers (migration 0007), prefix + typo-tolerant matching (ADR-0007), sidebar box with `/`; the edge and @ pickers keep their own filtering for now
- [x] 12 Command palette and quick-add (implemented; manual click-through pending). Ctrl/Cmd+K, grammar in `docs/quick-add-grammar.md`, preview with inline pick/create/skip, one-transaction commit; "What if this slips?" waits for impact analysis (14)
- [x] 13 Schedule and critical path (implemented; manual click-through pending). Core CPM in working days, negative slack against an unrealistic target, `get_schedule`/`get_critical_path`, project-panel timeline with Days/Weeks; the demo-data snapshot criterion waits for spec 24
- [x] 14 Impact analysis (implemented; manual click-through pending). Multi-slip scenarios, absorbed vs passed-on slack, `depends_on` knock-on, objectives and people, late-vs-target flags, previewed all-or-nothing "Apply to plan"; the demo-data snapshot criterion waits for spec 24
- [x] 15 Health scoring and Overview (implemented; manual click-through pending). Configurable thresholds (Settings), worst-signal project health with reasons, weighted objective health, top-5 risks (severity x priority x scope), overloaded people (next five days) and stale waiting-ons on a real Overview; Health sections in the project and objective panels
- [x] 16 This week (implemented; manual click-through pending). The default screen on launch (`/`; Overview is now `/overview`), Monday-Sunday, six sections plus a day strip, inline complete / reschedule / resolve / snooze
- [x] 17 Capacity (implemented; manual click-through pending). Weekly load per person (schedule x allocation / capacity) as a heatmap with click-through to the tasks, the open-task-count flag with a setting, and the Overview's overloaded list built on it
- [x] 18 Dependency graph view (implemented; manual click-through pending). Layered left-to-right layout in core, Tasks and Projects levels, project/team/objective filters with dimmed context, critical path highlighted, SVG with pan and zoom; M2 (graph engine) is now complete apart from the demo-data checks that wait for spec 24
- [x] 19 Weekly review and export (implemented; manual click-through pending). Seven-step review with inline fixes, a user-editable Markdown report template (Settings), a board/exec-style status report, save dialog (save permission only, ADR-0009) and a checked export path; `insta` snapshots on a hand-built week, demo data waits for spec 24
- [x] 20 Local backup and restore (implemented; manual click-through pending). Default folder inside app data, changeable in Settings; manual, daily (newest 14 kept), pre-migration and pre-restore backups with SQLite's online backup; a restore is validated, shows what the file holds, saves the current data first and restores into the live connection
- [x] 21 Encryption at rest (implemented; manual click-through pending, the real OS keychain especially). SQLCipher with a random key in the OS keychain (recovery key shown once) or a passphrase; with no keychain only the passphrase is offered, never a stored key; verified export-and-swap re-keying with a backup first (ADR-0010); locked start with an unlock screen; encrypted backups, plain ones flagged and deletable
- [x] 22 Google Drive storage, autosave, multi-device sync and attachments (implemented; manual check pending, and it needs your Google OAuth client). ADR-0011: every change autosaves an encrypted snapshot to your Drive (one file per device); devices merge each other's snapshots automatically, row by row with the newest write winning, deletes via tombstones, rules re-checked and every repair reported; hourly/daily checkpoints and "Recover"; attachments (images, SVG, Markdown, PDF, Word, Excel, PowerPoint) stored as encrypted blobs with a local cache and shown in notes through a `minimap-media` protocol; status bar, a This week banner and the Settings card say plainly while data is local only. Verified against a folder remote with two simulated devices and a mock Google server; real Drive and the screens need a manual check.
- [x] 23 Settings (implemented; manual click-through pending). The screen is split into tabs by kind (General, Thresholds, Reports, Data & backup, Security; the tab is in `?tab=` so other screens can link to one). New: working days (a `WorkWeek` threaded through the schedule, capacity, impact, overview, dependency graph and weekly review; Gantt axis follows it), default weekly capacity for new people, and a read-only Data location card with Show in folder. Decided: the database folder is not movable (backup/restore and Drive already move data safely). The Developer tab waits for spec 24.
