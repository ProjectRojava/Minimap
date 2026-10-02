# 02 — App shell and navigation

Status: Implemented — awaiting manual check · Milestone: M1 · Priority: Must
Depends on: —

## Goal
The frame every screen lives in: sidebar, routing, the right-side detail pane, theming and keyboard basics.

## Scope
**In**
- Left sidebar: Overview, Inbox, This week, Objectives, Projects, Tasks, People, Teams, Notes, Decisions, Waiting on, Weekly review, Settings (hide entries whose feature isn't built yet).
- Client-side routing (`leptos_router`), one route per screen plus `/:type/:id` deep links.
- Detail pane: opens on the right when any node is clicked; shows fields (editable), edges grouped by type, and activity history. Closable with `Esc`.
- Light/dark theme following the OS (`prefers-color-scheme`), with an override from Settings later (23).
- Keyboard: `g` then a letter to jump (`g p` projects, `g t` tasks…), `j/k` to move in lists, `Enter` to open, `Esc` to close.
- Error toasts: render `AppError { code, message }` readably.
- App logo (`branding/minimap-logo-auto.svg`) in the sidebar.

**Out**
- Command palette (12).

## UI notes
- Dense, calm: 13–14px base text, compact rows, minimal borders.
- Detail pane width ~420px, resizable later.

## Acceptance criteria
- [ ] Every sidebar entry navigates; back/forward work.
- [ ] Clicking any node anywhere opens the detail pane; `Esc` closes it.
- [ ] Theme switches when the OS theme changes, without reload.
- [ ] Keyboard navigation works without a mouse on list screens.

## Decisions
- **Keybindings**: Linear-style `g` chords for navigation (`g p` Projects, `g t` Tasks…; second key within 1 s; hints shown on sidebar hover). Single keys only inside lists (`j`/`k`/`Enter`/`Esc`). All shortcuts are ignored while typing in an input, and Ctrl/Cmd combos are left alone (reserved for the palette, 12).
- **Detail pane**: split view at ≥ 1100 px window width, overlay with a dimmed scrim below that (pure CSS, `min-[1100px]:`).

## Implementation notes
- UI: `ui/src/{app,nav,keyboard,state,api}.rs`, `components/{sidebar,detail_pane,toasts,node_row}.rs`, `pages/`. Pure logic (chord table, cursor, link headings, activity text) is unit-tested natively.
- Sidebar entries are driven by `nav::NAV`; `enabled: false` hides an entry (This week and Weekly review until built). Unbuilt screens are placeholders naming their spec.
- `/:type/:id` (e.g. `/task/<uuid>`) opens the node in the pane over its list screen.
- Theme follows the OS through Tailwind's `dark:` media variant, so it switches live. The Settings override comes with 23.
- New read-only commands: `get_node_summary`, `list_edges_for`, `list_activity_for` (allow-listed in `build.rs` + capability). Store gained `nodes::summary` and `edges::links_for_node`.
- Detail pane shows header, links grouped by type/direction, and activity. The **Fields** section is a placeholder until the node screens (03–06) provide editors.
- `NodeRow` + `ListNav` give any list screen the cursor/Enter behaviour; no list screen exists yet, so it is not exercised in the app.

## Not yet verified by hand
`cargo tauri dev` starts without errors, but nobody has clicked through the window. Check:
- every sidebar entry navigates; back/forward work
- `g` chords navigate; hints show on hover
- `/task/<any-uuid>` style deep link opens the pane (shows "Unavailable" + error toast for an unknown id)
- OS light/dark switch repaints without reload
- pane is a side-by-side split when the window is wide, an overlay when narrow; `Esc` and the scrim close it
