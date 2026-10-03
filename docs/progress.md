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
