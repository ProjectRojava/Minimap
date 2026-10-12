# 03 — Approaching: marks at the edge for trouble elsewhere that will reach you

Status: Implemented as spec 45 — awaiting manual check · Milestone: — · Priority: Should · Size: M
Depends on: specs 07, 13, 14, 16 (implemented) · Read first: [README](README.md)
ADR: none (a new view of the schedule; no ADR is affected, checked against `docs/decisions/`)

## Goal
Games put a small arrow at the edge of the screen pointing at something important you can't see. In Minimap, when you are looking at one project (or at your own week), **problems in other projects that will reach this one through `blocks` or `depends_on` links are invisible** unless you go looking in the dependency graph or run a what-if. Show them where you already are, as a thin mark at the edge: *"↗ 2 approaching from EU Region · first in 4 days"*. Click it to see the chain and act.

## User stories
- I open the *Security and Compliance* project panel. A strip at the top says "↗ Approaching: 1 from EU Region, reaches *Pen test* in 3 working days". I click it: "*Gateway auth* (EU Region) is blocked → *API migration* (EU Region) → *Pen test* (here). Pen test has 1 day of slack." Buttons: *Open Gateway auth*, *What if it slips?*.
- On This week, a line under the header says "↗ 2 things approaching your work". It lists the chains into tasks assigned to me.
- When nothing outside is in trouble, nothing is shown (no "all clear" noise).

## Rules (`minimap-core`, pure)
New module `crates/minimap-core/src/approach.rs`.

### Scope
```rust
pub enum ApproachScope { Project(Uuid), Me /* tasks assigned to the self person */, Objective(Uuid) /* tasks of projects that contribute, plus tasks that contribute directly */ }
```
`in_scope(task)` per scope. Meetings and closed tasks are never in scope and never sources.

### Sources (things "in trouble")
An open task **T outside the scope** is a source when any holds (reasons, all that apply, in this order):
1. `Overdue`: due date before today;
2. `Blocked`: status blocked;
3. `Late`: its scheduled finish (schedule EF date) is after its due date;
4. `NoSlack`: its slack is negative.

A **project Q outside the scope** is a source when its forecast is late (`late_by_days > 0`) or its computed health is red.

### Reach
- **Task chains**: from each in-scope open task S, walk `blocks` predecessors (edge `X blocks S` means X → S) breadth-first through **open** tasks only, at most `MAX_HOPS = 4` hops, leaving the scope (a path may go out and stay out; paths that re-enter the scope stop at the first in-scope task, which then becomes the target instead). Every source T reached gives an `Approaching { source, target: S, path: [T, ..., S], reasons }`. Keep only the **shortest** path per (T, S).
- **Project chains**: for scope `Project(P)`: each `P depends_on Q` (Q outside, transitively up to 3 hops through `depends_on`) where Q is a source gives `Approaching { source: Q, target: P, path: [Q, ..., P] }`. For scope `Me`: through projects owned by me. For `Objective`: through its projects.

### Timing and severity
- `arrives_in` (working days) = from today to S's scheduled early start (0 if S is in progress, blocked, or its start is past). For project chains: to P's earliest open-task start.
- `severity`:
  - `Hits` — S is already late (its EF > its due date or its slack < 0) **and** the path is on S's driving chain (the predecessor whose finish sets S's start; the schedule knows this: as built `ScheduledTask.driver: Option<Uuid>` is computed in the forward pass: the open predecessor with the latest finish (plus lag), when it is at least as late as every other constraint, ties to the lowest id);
  - `Threatens` — S's slack is `<= THREAT_SLACK` (2 working days);
  - `Watch` — otherwise.
- `Watch` items are returned but the UI shows them only in the popover, never as the headline count.

### Grouping and order
`approaching(world, scope, today, week) -> ApproachReport { groups: Vec<ApproachGroup { source_project: Option<NodeSummary> /* None = inbox */, items: Vec<Approaching> }>, headline: Option<Headline { count, first_in_days, worst: Severity }> }`. Items: Hits first, then Threatens, then Watch; inside, smallest `arrives_in`, then title. Groups ordered by their worst item. `headline` counts Hits + Threatens only; `None` when zero.

### Cost
One `schedule::compute_with` (already done by callers) + BFS bounded by `MAX_HOPS`. Fine for 400+ tasks.

**Tests**: hand-built graphs for each reason; a chain that re-enters scope stops there; shortest path kept; MAX_HOPS respected; done tasks break chains; driving vs non-driving predecessor (`Hits` only on the driver); project `depends_on` chain; meetings ignored; Me scope with no self person → empty. **proptest**: every reported path is a real chain of active `blocks` edges, starts outside scope, ends inside, and has length ≤ MAX_HOPS + 1; reports are deterministic (same input, same order).

## Data
No migration, no setting. (As built the constants `MAX_HOPS` = 4 and `THREAT_SLACK` = 2 live in `minimap-types`, next to the types, so the UI and the Help test can quote them; `MAX_PROJECT_HOPS` = 3 is in core.)
Types in `crates/minimap-types/src/approach.rs`: `ApproachScope`, `Reason { Overdue, Blocked, Late, NoSlack, ProjectLate, ProjectRed }`, `Severity`, `Approaching { source: NodeSummary, target: NodeSummary, path: Vec<NodeSummary>, reasons: Vec<Reason>, overdue_days: Option<u32> /* added: for "Overdue 3 days" */, arrives_in: u32, severity: Severity, target_slack: Option<i32> }`, `ApproachGroup`, `Headline`, `ApproachReport`.

## Commands
- `get_approaching(scope: ApproachScope)` (as built the red projects come from `overview_at`, the same health the Overview shows) in a new `src-tauri/src/commands/approach.rs` (`approaching_at(conn, scope, today)`): settings (work week), `tasks::list_planned`, active edges, projects, objectives, people (self), the schedule, project health (reuse what `overview` computes; if that's expensive, compute only project forecasts and use `late_by_days` + a `health::project_health` call per candidate source project). Unknown project/objective → `not_found`.
- Register in the 4 places.

## UI
New component `ui/src/components/approaching.rs`:
- **`ApproachStrip { scope }`**: a 28px strip, `bg-panel`, hairline below, left-aligned: an arrow glyph (new `icons.rs` entry `approach`, a chevron pointing up-right), then pure `headline_text(headline, groups)`: "2 approaching from EU Region · first in 4 days" / "3 approaching from 2 projects · first today". Tone: `warning` text when worst is Threatens, `danger` when Hits. Hidden when `headline` is `None`. Click or `Enter` when focused → popover.
- **Popover** (as built: the chains open **inline**, under the strip, instead of in a floating popover: flat, nothing to position, nothing to cover the work; the strip is a toggle button with `aria-expanded`): one block per group (source project chip with its objective colour), each item: the path as `Name (Project) → Name → **Target**` with each reason as a pill (`Overdue 3 days` danger, `Blocked` warning, `Late` warning, `No slack` warning), "reaches *Target* in N working days", "Target has N days of slack". Buttons: *Open* (opens the source in the pane), *What if it slips?* (`Scenario.start(source, days)` with days = the source's lateness or 5, then `/what-if`). Watch items under a collapsed "Also watching (N)".
- **Where**:
  - Project panel (`project_panel.rs`): at the top, under the pill row, scope `Project(id)`.
  - Tasks screen when the project filter is set (board and list): under the filter bar (`FilteredApproach`). The Dependencies screen is **not** done (Open question 2: "if cheap"; it is not, because that screen has its own filters and no single scope).
  - This week: under the header stats, scope `Me`.
  - Objective panel: scope `Objective(id)`.
- Reload on `DataVersion` (version-keyed `LocalResource`).
- Pure helpers `headline_text`, `reason_text`, `path_text`, `arrives_text` with tests.

## Help wiki
- New page `docs/help/approaching.md` (*Approaching: trouble elsewhere that reaches you*): what counts as trouble, how far it looks (4 links), what *reaches in N days* means, the three severities in plain words, where the strip shows, why nothing shows when all is well. Add to `PAGES` (Planning and insight), link from `dependencies.md`, `projects.md`, `this-week.md`, `what-if.md`.

## Demo data
No schema change and no seed change. The snapshot `what_is_approaching_in_the_demo_data` (`src-tauri/src/commands/demo.rs`, at 2027-03-03, scopes Me and each project) shows what the data says: *Security and Compliance* has 1 approaching, from EU Region (`Provision EU network and clusters` → `Regional auth and tenancy` → `Security review sign-off`, no slack, arrives in 14 working days, threatens); EU Region, Platform Cost Reduction and Me hear nothing.

## Export, undo, sync
Nothing stored.

## Out of scope
Notifications; approaching threats from people (overload) or waiting-ons; scopes for teams; drawing the arrows in the dependency graph (it already shows the links).

## Acceptance criteria
- [x] `core::approach` tests and proptests pass; schedule exposes the driving predecessor if it was needed, with tests.
- [x] `get_approaching` command tests for each scope at a fixed date.
- [x] Strip and popover on the project panel, filtered Tasks, This week and objective panel; hidden when nothing approaches; Watch items never in the headline.
- [x] Help page, CHANGELOG, knowledge graph (§4, §5 `mod:approach`, §7, §8), `docs/progress.md`; demo snapshot reviewed; gate passes.

## Not yet verified by hand
- Demo data: open Security and Compliance: the strip names EU Region; the popover's path matches the dependency graph.
- Mark the blocking EU Region task done: the strip disappears (after the data reload).
- This week: assign yourself a task downstream of a blocked task in another project: the line appears.
- *What if it slips?* opens the what-if screen with the source already in the scenario.

## Open questions
1. Make `MAX_HOPS` / `THREAT_SLACK` settings? **No**, constants; revisit after use.
2. Show the strip on the Dependencies screen when it's filtered to one project? **Yes, if cheap** (same scope as the filter); otherwise later.
