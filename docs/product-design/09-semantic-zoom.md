# 09 — Zoom: move between the whole portfolio, an objective, a project and a task

Status: Draft · Milestone: — · Priority: Could · Size: M
Depends on: specs 02, 04, 05, 06, 18, 35 (implemented) · Read first: [README](README.md)
Expected ADR: none

## Goal
A game's minimap scrolls out to the **world map** and back in to where you stand, and you never lose your place. Minimap's hierarchy (objective → project → task) is spread over separate screens and the pane; going "up" from a task to its project and objective takes several clicks and you lose where you were.

Add **zoom levels** with two keys and a breadcrumb:
- **World**: everything (Overview).
- **Objective**: one objective with its projects (Objectives screen, objective in the pane).
- **Project**: one project with its tasks (Tasks board filtered to the project, project in the pane).
- **Item**: one task, note, decision… (its pane).

`Alt+↑` zooms out one level from what you're looking at; `Alt+↓` zooms back in to where you came from (or to the highlighted row). The breadcrumb in the page header shows the path and is clickable.

In the Dependencies graph, scrolling out past the smallest zoom of the **Tasks** level switches to the **Projects** level centred on the project you were looking at, and scrolling in on a project in the Projects level switches back to its tasks.

## User stories
- I'm in *Gateway auth*'s pane on the Tasks board. `Alt+↑`: the board filters to *EU Region* and its panel opens; breadcrumb "World › Launch in the EU › EU Region". `Alt+↑` again: Objectives, *Launch in the EU* open. Again: Overview. `Alt+↓` three times: back to *Gateway auth*, exactly where I was.
- An inbox task (no project) zooms out to World, skipping the missing levels.
- A project serving two objectives zooms out to the one with the highest priority (the breadcrumb lets me pick the other: a small ▾ next to the objective).
- On Dependencies (Tasks level) I scroll out until the graph is tiny; one more notch and it becomes the Projects level with *EU Region* centred and highlighted.

## Rules (`minimap-core`, pure)
New module `crates/minimap-core/src/zoom.rs`.

```rust
pub enum Level { World, Objective, Project, Item }
pub struct ZoomStep { pub level: Level, pub node: Option<NodeSummary>, pub alternatives: Vec<NodeSummary> }
pub fn path_of(node: NodeRef, world: &ZoomInput) -> Vec<ZoomStep>; // World first, the node last
```
- Task: `[World, Objective?, Project?, Item(task)]`. Project = the task's project. Objective = the project's objective (below), or, for an inbox task with a direct `contributes_to`, that objective.
- Project: `[World, Objective?, Project]`. Objective choice: among active objectives it `contributes_to`, highest priority (1 first), then highest weight, then target date, then title. `alternatives` = the others in that order.
- Objective: `[World, Objective]`.
- Note, decision, waiting-on: `[World, …the path of the first item they mention / affect / are about…, Item]`; if none, `[World, Item]`. Person, team: `[World, Item]`.
- Archived links ignored; meetings behave as tasks.
**Tests**: each node kind; tie-breaks; inbox task; archived objective skipped; alternatives order. **proptest**: the path always starts with World and ends with the node, and levels strictly deepen.

## Data
No migration, no setting. Types in `crates/minimap-types/src/zoom.rs`: `Level`, `ZoomStep`.

## Commands
`get_zoom_path(node: NodeRef)` in a new `src-tauri/src/commands/zoom.rs` (`zoom_path_at` not needed: no dates). Register in the 4 places. Needs: objectives, projects, tasks (project ids), active `contributes_to`/`mentions`/`affects`/`about` edges. Cheap enough per call; cache nothing.

## UI
- **Where each level "is"** (pure `ui/src/zoom.rs`, tested: `route_for(step) -> (path, selection)`):
  - World → `/overview`, no selection.
  - Objective → `/objectives`, select the objective (pane open).
  - Project → `/tasks?project=<id>` (the board filtered; `TaskFilters` already has a project filter: make it settable from the query string if it isn't) **and** select the project (pane open).
  - Item → keep the current route if the item is visible there, else the item's list route (`nav::list_path`), select it.
- **Zoom stack**: `ZoomCtx` in `state.rs`: a stack of `(route, selection, list cursor)` pushed on each zoom-out and popped on zoom-in, so `Alt+↓` returns exactly. A normal navigation (sidebar, chord, link) clears the stack. When the stack is empty, `Alt+↓` zooms into the cursor row's item (if any): from Overview into the cursor objective, etc.
- **Keys**: `Alt+ArrowUp` / `Alt+ArrowDown` in `keyboard.rs` (handled **before** the early return for Ctrl/Cmd/Alt combos; not while typing in a text box, so Alt+arrows keep their text meaning there). Also palette actions "Zoom out" / "Zoom in". Add to `docs/help/keyboard.md`.
- **Breadcrumb** (`components/page.rs`): `PageHeader` gains an optional `crumbs: Vec<ZoomStep>` shown under the title in `muted` 11px: "World › Launch in the EU ▾ › EU Region". Shown on Overview (just "World"), Objectives (when one is selected), Tasks (when filtered to a project) and in the pane header for any item (pane header shows the crumbs of the open item; clicking a crumb zooms there and pushes the stack). The ▾ opens a themed menu of `alternatives`.
- **Dependencies graph** (`pages/graph.rs`): the wheel/± zoom keeps working geometrically. New: at the minimum scale on the Tasks level, one further zoom-out step (wheel notch or `-`) switches to Projects level with the project of the task nearest the viewport centre selected and centred (`fit` around it); at the maximum scale on the Projects level, one further zoom-in on a project (wheel over it, or `+` with a project selected) switches to Tasks level filtered to that project. Show a one-line hint the first time it happens in a session: "Zoomed out to projects". Pure `next_level(scale, min, max, delta, level) -> Option<Level>` tested.
- **Motion**: none beyond the graph's existing pan/zoom; route changes are instant.

## Help wiki
- New page `docs/help/zoom.md` (*Zoom: from the whole portfolio to one task*): the four levels, the keys, the breadcrumb and ▾, how a project with several objectives picks one, the graph's level switch.
- Link from `keyboard.md` (keys), `dependencies.md` (level switch), `welcome.md` (one line), `objectives.md`, `projects.md`.

## Demo data
No change. Add a store/command test on the demo data: the path of a known EU Region task is World › Launch in the EU › EU Region › task.

## Export, undo, sync
Nothing stored.

## Out of scope
A dedicated "map" screen; zoom levels for teams/people; animated transitions.

## Acceptance criteria
- [ ] `core::zoom` tests and proptest; `get_zoom_path` command test incl. the demo path.
- [ ] `Alt+↑/↓` and palette actions work from tasks, projects, objectives, notes, decisions; the stack returns exactly to the previous screen, selection and cursor.
- [ ] Breadcrumb in headers and the pane, clickable, with ▾ alternatives.
- [ ] Graph switches levels at the zoom bounds, both ways.
- [ ] Help page and links; CHANGELOG; knowledge graph (§5, §7, §8 incl. keys and `ZoomCtx`); `docs/progress.md`; gate passes.

## Not yet verified by hand
- From a task's pane, `Alt+↑` three times, `Alt+↓` three times: back to the same task, same board scroll.
- A project with two objectives: the ▾ lists the other; choosing it zooms there.
- Dependencies: scroll out past the minimum on Tasks: Projects level with the right project centred; scroll in on a project at maximum: its tasks.
- In a text box, `Alt+↑` doesn't zoom.

## Open questions
1. Use `Alt+↑/↓` (works on all three OSes) rather than `Ctrl+↑` (macOS Mission Control) or `-`/`=` (the graph's zoom keys)? **Yes.**
2. Should Project level be the Tasks board or the project panel's schedule? **Board + panel**; the schedule is one click away in the panel.
