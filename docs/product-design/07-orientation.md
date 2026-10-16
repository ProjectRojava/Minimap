# 07 — Portfolio view and My view: north-up or player-up

Status: Implemented as spec 50 — awaiting manual check (open questions answered as recommended) · Milestone: — · Priority: Should · Size: M
Depends on: specs 15, 17, 18 (implemented); doc 03 (reuses its reach walk) · Read first: [README](README.md)
Expected ADR: none

## Goal
Game minimaps can be **north-up** (the world stays fixed) or **player-up** (the map turns with you). Minimap's screens are all north-up: Overview, Projects, Tasks, Dependencies and Capacity show the whole organisation. An exec often wants the other one: **everything as it touches me**: what I own, what's assigned to me, what my reports carry, what reaches my work through dependencies, which objectives that serves.

Add one app-wide toggle, **Portfolio** ↔ **Mine**, that every map-like screen respects, and say on each item *why* it's in My view.

## User stories
- I press `v`. The title bar's view button flips from **Portfolio** to **Mine**. Overview now shows only objectives my work serves, projects I own or have tasks in, and the top 5 risks *among those*. Each project row has a small chip: "you own it" / "3 of your tasks" / "reaches you".
- Dependencies in My view shows my tasks, what blocks them (up to 4 links back) and what they block, with everything else hidden.
- Capacity in My view shows me and my direct reports (and anyone with tasks in my projects, under a separate heading).
- Press `v` again: back to Portfolio. The choice is remembered on this computer.
- This week is unchanged (it's already about me); the toggle doesn't appear there.

## Rules (`minimap-core`, pure)
New module `crates/minimap-core/src/orientation.rs`.

```rust
pub enum View { Portfolio, Mine }
pub enum Why {
    Assigned,                 // task assigned to me
    Owner,                    // project I own
    InMyProject,              // task in a project I own
    Reaches { via: Uuid },    // upstream of my work (task blocks..blocks my task; project depends_on chain)
    Blocks  { via: Uuid },    // downstream: my work blocks it (one hop only)
    Serves,                   // objective my in-scope projects/tasks contribute to
    Report,                   // person who reports to me
    OnMyProjects,             // person assigned to tasks in projects I own
    Me,
}
pub struct MyScope {
    pub tasks: BTreeMap<Uuid, Vec<Why>>,
    pub projects: BTreeMap<Uuid, Vec<Why>>,
    pub objectives: BTreeMap<Uuid, Vec<Why>>,
    pub people: BTreeMap<Uuid, Vec<Why>>,
}
pub fn my_scope(input: &ScopeInput /* self id, tasks, projects, objectives, active edges */) -> MyScope;
```
Rules, in order (an item may have several reasons; keep them all, in this order):
1. Tasks: open or finished in the last 14 days, assigned to me → `Assigned`; in a project I own (`owner_person_id` = me) → `InMyProject`.
2. Projects: owned by me → `Owner`; containing an in-scope task → (no extra why: the UI says "N of your tasks" from the task set).
3. Reach: from every in-scope open task, walk `blocks` predecessors up to `MAX_HOPS = 4` (reuse `core::approach`'s walk from doc 03 via a shared helper, e.g. `approach::upstream(task, edges, max_hops)`; if doc 03 isn't built yet, put the helper here and doc 03 reuses it) → those tasks `Reaches{via: the in-scope task}`; their projects too. One hop downstream (`blocks` successors) → `Blocks{via}`. Projects: `depends_on` chains into an `Owner` project (3 hops) → `Reaches`.
4. Objectives: contributed to (`contributes_to`) by any in-scope project or task (not by `Reaches`/`Blocks` ones) → `Serves`.
5. People: me → `Me`; `reports_to` me (direct) → `Report`; assigned to in-scope tasks in projects I own → `OnMyProjects`.
6. Meetings are in scope like any task when assigned to me (they don't feed reach: they have no `blocks`).
7. No self person → every set is empty (the UI then disables the toggle with a hint).

**Tests**: each rule; multiple reasons ordering; reach stops at MAX_HOPS and at finished tasks; the 14-day window for finished tasks; objective served only via own work; empty without a self person. **proptest**: `my_scope` is monotone: adding an `assigned_to` edge to me never removes anything from any set.

## Data
No migration. The view choice is **per device and not stored in the database**: `localStorage["minimap.view"]` (like the board sort), read with try/catch; default `portfolio`.
Types (`crates/minimap-types/src/orientation.rs`): `View`, `Why`, `MyScope`.

## Commands
- `get_my_scope()` (new `src-tauri/src/commands/orientation.rs`, `my_scope_at(conn, today)`): returns `MyScope`. Used by list screens (Projects, Tasks, Objectives) to filter and to label rows.
- Screens whose *contents are computed* must compute in My view, not filter afterwards (a top-5 filtered down is not the top 5 of mine):
  - `get_portfolio_overview(view: Option<View>)`: core `overview::build` takes an optional `&MyScope` and restricts objectives, projects, risks (top 5 recomputed over in-scope items), overloaded (in-scope people), stale waiting-ons (all; they're all yours). Default `Portfolio` keeps today's output byte-for-byte (snapshot unchanged).
  - `get_dependency_graph(filter)`: add `GraphFilter.mine: bool` → in-scope tasks/projects are the nodes; the existing one-hop dimmed context still applies.
  - `get_capacity(.., view: Option<View>)`: rows = in-scope people (`Me`, `Report`, then `OnMyProjects` under a separate group heading).
- Keep the `*_at` test variants; argument structs use camelCase on the JS side.

## UI
- **State**: `ViewCtx(RwSignal<View>)` in `state.rs`, provided by `App`, initialised from localStorage; every screen that respects it reads it inside its resource closure (so switching reloads).
- **Title bar**: a `ViewToggle` button left of the window controls: a compass glyph (`icons.rs` `north` for Portfolio: an arrow with "N"; `me` for Mine: a filled triangle like a player marker) + the word. Click or `v` toggles. Tooltip: "Portfolio view: everything. Mine: everything as it touches you (v)".
- **`v` key**: a global single key in `keyboard.rs` (not while typing; `v` is unused today: `g`, `/`, `?`, `n`, `j`, `k` and the row keys are taken). Add to `docs/help/keyboard.md` (the test checks every `NAV` chord; add `v` to the prose and table too).
- **Screens that respect the view**: Overview, Objectives, Projects (list and board), Tasks (board and list; not the Inbox: inbox tasks have no project, and assignment already filters them), Dependencies, Capacity. Each shows a slim `MineBanner` under its header when in My view: "My view: showing what touches you. [Show everything]".
- **Why chips** (`WhyChip`, muted, small): rows and cards in My view show the first reason: "assigned to you", "you own it", "in your project", "reaches you via *Pen test*", "blocked by your work", "serves your work", "reports to you", "on your projects". Pure `why_text(why, names)` tested.
- **This week, Weekly review, Notes, Decisions, Waiting on, People, Teams, Settings, Help**: unchanged; the toggle stays visible but those screens ignore it (the banner isn't shown).
- No self person: the toggle is disabled with the tooltip "Set who you are in People first".

## Help wiki
- New page `docs/help/views.md` (*Portfolio view and My view*): what's in My view and why (the reason chips listed), which screens follow it, `v`, remembered per computer.
- Link from `overview.md`, `dependencies.md`, `capacity.md`, `projects.md`, `tasks.md`, `keyboard.md` (`v`), `welcome.md` (one line).

## Demo data
No schema change. "Me" in the demo owns or is assigned some work: check at 2027-03-03 that My view is non-trivial (at least one owned project or several assigned tasks, at least one `Reaches`). If "me" owns nothing, assign "me" as owner of one project **only if** the snapshots' story still holds; otherwise leave the seed and rely on hand-built tests. Add a `my_scope` snapshot and a My-view Overview snapshot.

## Export, undo, sync
Nothing stored in the database.

## Out of scope
A third view (a team's view); saving named views; This week in Portfolio view.

## Acceptance criteria
- [x] `core::orientation` tests and proptest; shared upstream walk with doc 03 (one implementation).
- [x] Overview, graph and capacity compute in My view; Portfolio output unchanged (existing snapshots unchanged).
- [x] List screens filter by `get_my_scope` and show why chips.
- [x] Title-bar toggle, `v`, banner, localStorage memory; disabled without a self person.
- [x] Help page and links; CHANGELOG; knowledge graph (§5, §7, §8 incl. the new key); `docs/progress.md`; gate passes.

## Implementation notes (spec 50)
- **One walk**: `approach::walk_back(start, preds, max_hops, visit)` is the breadth-first walk both *Approaching* (tasks and projects) and `my_scope` use; the Approaching code was refactored onto it (its tests and snapshots are unchanged).
- **Reach walks through your own tasks**: the walk from each of your open tasks continues past another task of yours and gives reach reasons only to tasks that are not yours. That keeps `my_scope` monotone (assigning one more task to you never removes anything from any set; the proptest checks membership, since a task that was in because your work blocked it is, once it is yours, in for being assigned instead).
- **Projects that hold downstream work** (`Blocks`) are in My view too, like projects that reach you, so the Overview can say "blocked by your work" on them. A project that is only here for your tasks has an empty reason list and `MyScope.project_tasks` gives its count ("3 of your tasks", "one of your tasks").
- **Computed, not filtered**: the Overview, graph and Capacity take the view; the Overview lists only in-scope objectives and projects and recomputes the top five risks (and the counts, the overloaded list) among them, while each objective's health is still judged by all its work. Portfolio view calls the same code with no scope and is byte-for-byte the old output (a test compares them; no snapshot changed).
- **List screens** (Objectives, Projects, Tasks board and list) filter what the backend returned by the scope (`Orientation::shows`); the Projects board keeps all five columns; the Inbox is not filtered. `get_portfolio_overview()` for the panes' health sections still sends no view.
- **The view lives in `Orientation`** (provided by `App`, `components/orientation.rs`) rather than a bare `ViewCtx` signal in `state.rs`, because the scope and the "no me" rule travel with it. If there is no "me" the stored choice stays but Portfolio view is in effect and the button is disabled.
- **Capacity** gets `Capacity.mine` and `PersonCapacity.on_my_projects`; `capacity::in_my_view` keeps you, your reports and the people on your projects, in that order, and the screen heads the last group *On your projects*. "Anyone with tasks in my projects" are the people assigned to tasks in projects you own.
- **Demo data**: nothing in the demo reaches your work from outside (every cross-project link runs out of the Platform Cost Reduction project, which you now own, so it blocks EU Region work). The My-view snapshots show owned, assigned and blocked-by-your-work items; reach has hand-built tests. No other snapshot changed.

## Not yet verified by hand
- Press `v` on Overview with demo data: fewer objectives/projects, each with a reason chip; top risks recomputed.
- Dependencies in My view: only your tasks, their blockers and what they block.
- Restart the app: the view is remembered.
- In a text box, `v` types a "v" and does not toggle.

## Open questions
1. Include tasks finished in the last 14 days? **Yes**, so the board's Done column isn't empty in My view.
2. Downstream reach beyond one hop? **No**; upstream matters more and one hop down keeps My view small.
