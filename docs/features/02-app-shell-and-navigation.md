# 02 — App shell and navigation

Status: Draft · Milestone: M1 · Priority: Must
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

## Open questions
- Keybinding scheme: Linear-style `g` chords, or single keys?
- Should the detail pane be a split view or an overlay on narrow windows?
