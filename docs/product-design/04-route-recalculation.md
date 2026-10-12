# 04 — Route recalculation: a slip shown as the new route to the finish

Status: Draft · Milestone: — · Priority: Should · Size: M
Depends on: specs 13, 14 (implemented) · Read first: [README](README.md)
Expected ADR: none

## Goal
Game maps and satnavs draw the **route** to the objective as a line, and when you go off it they say *"recalculating"* and draw the new one. The critical path is the route to a project's finish. Today the what-if screen (spec 14) lists every affected task, project, objective and person as before → after tables; it's accurate but you have to read it all. Add a **Route** view that shows, for each affected project, the old route faded, the new route in the accent colour, the new arrival date, and a short **turn-by-turn list**: where the slip enters, where it is absorbed, where it passes on, and where it arrives.

## User stories
- I run *What if this slips?* on *Gateway auth* for 5 days. The what-if screen opens on **Route**: "EU Region · recalculated: arrives 2027-04-14 instead of 2027-04-07 (+5 working days), 3 days past target." Under it a small timeline: the old critical chain dashed and muted, the new one solid accent. The route now goes through *Load testing* instead of *Data residency review*.
- Turn by turn: "1. *Gateway auth* slips 5 days (start). 2. *API migration* +5. 3. *Data residency review* absorbs 2 of 5 (had 2 days of slack). 4. *Load testing* +5, now on the route. 5. Arrives at *Go-live* +5: 3 days past target."
- If a project's route doesn't change, it says "No change to the route; finish unchanged" and shows no timeline.
- The old tables are still one click away (*Details* tab).

## Rules (`minimap-core`, pure)
Extend `crates/minimap-core/src/impact.rs` (do not fork it).

`analyze` already re-runs `schedule::compute_with` with the slips as earliest-start floors and diffs against the baseline. Keep both schedules long enough to derive, **per affected project**:

```rust
pub struct ProjectRoute {
    pub project_id: Uuid,
    pub title: String,
    pub before: Vec<Uuid>,      // baseline critical chain, in order (start -> finish)
    pub after: Vec<Uuid>,       // scenario critical chain, in order
    pub changed: bool,          // before != after as sequences
    pub old_finish: Option<Date>,
    pub new_finish: Option<Date>,
    pub target_date: Option<Date>,
    pub late_after: Option<u32>,
    pub turns: Vec<Turn>,
}
pub enum TurnKind { Slips /* a slipped task, the start */, PassesOn /* delay > 0, absorbed 0 */, Absorbs /* absorbed > 0, delay == 0 */, PartlyAbsorbs /* both > 0 */, JoinsRoute /* now critical, wasn't */, LeavesRoute /* was critical, isn't */, Arrives /* the project's last task: the finish */ }
pub struct Turn { pub task: NodeSummary, pub kind: TurnKind, pub incoming_days: f64, pub absorbed_days: f64, pub delay_days: f64, pub new_finish: Date }
```

- **Chain order**: the critical tasks of a project from the schedule are a set; order them into a chain by following `blocks` links between critical tasks from the earliest ES (ties: lowest `lf`, then title). Where the critical set is not a single chain (parallel critical branches), the chain is the one that ends at the task with the latest EF; the other branches are dropped from `before`/`after` (the timeline still draws all critical tasks; see UI). Put this in a pure `chain_of(schedule, project_id) -> Vec<Uuid>` in `schedule.rs` and reuse it.
- **Turns**: walk the tasks the slip reaches (`ImpactReport.tasks` for this project) in scenario ES order and emit one turn per task whose kind is not plain "unaffected"; consecutive `PassesOn` turns of the same delay **collapse** into one turn with a count ("*API migration* and 2 more +5"; add `also: u32` to `Turn`). `JoinsRoute`/`LeavesRoute` are added for tasks in `after \ before` and `before \ after`. The last turn is always `Arrives` for an affected project. At most `MAX_TURNS = 8` turns; beyond that, the middle ones collapse into "… N more steps" (`Turn` with kind `More`).
- Add `routes: Vec<ProjectRoute>` to `ImpactReport` (affected projects only, same order as `projects`).

**Tests**: single chain, slip passes through; partly absorbed; fully absorbed (route unchanged, `changed=false`, no `Arrives` delay); a branch takes over (JoinsRoute/LeavesRoute); parallel critical branches; collapse of PassesOn runs; MAX_TURNS; cross-project slip produces routes for both projects; `depends_on` knock-on project gets a route whose first turn is `Slips` with the held-up first task. Update existing `insta` snapshots of impact deliberately (the report gains a field).
**proptest**: `after` is a valid chain (each consecutive pair joined by an active `blocks` edge); every turn's `delay_days >= 0`; `Arrives.new_finish == new_finish`.

## Data
No migration, no setting. Types: `ProjectRoute`, `Turn`, `TurnKind` in `crates/minimap-types/src/impact.rs`; `ImpactReport.routes`.

## Commands
No new command: `run_impact_analysis` returns the richer report. Check `preview_apply_slips`/`apply_slips` are unaffected.

## UI (`ui/src/pages/what_if.rs`)
- Tabs under the scenario editor: **Route** (default) | **Details** (today's sections unchanged). Use the existing toggle button styles (`BUTTON_ON`). Remember the tab in the query string `?view=route|details`.
- **Route** tab: one `RouteCard` per `ProjectRoute`:
  - Header line from pure `route_headline(route)`: "EU Region · recalculated: arrives 2027-04-14 instead of 2027-04-07 (+5 working days), 3 days past target" / "Platform Cost Reduction · no change to the route; finish unchanged" (then the card is collapsed to that line).
  - **Mini timeline**: reuse `ui/src/timeline.rs::layout` with a new option to draw two bar sets: baseline bars for `before` (outlined, dashed `border-line`, `muted` text) and scenario bars for `after` (filled `accent` at the critical-path style already used by `schedule_panel`); other affected tasks as plain bars. The route line itself is a 2px polyline joining the bar ends of `after` in `accent`, and of `before` dashed in `faint`. Target day line as in the schedule panel. Keep it ≤ 12 rows; if more, show the route tasks only.
  - **Turn list**: an ordered list; each row: a step glyph by kind (start dot, arrow, absorb bracket, join/leave marks, finish flag; new `icons.rs` entries), task name (click opens the pane), and pure `turn_text(turn)`: "slips 5 days", "+5 days", "absorbs 2 of 5 (had 2 days of slack)", "now on the route", "off the route", "arrives +5: 3 days past target". Tone: `danger` only for the arrival when newly late, `success` for full absorption, otherwise neutral.
- The *Apply to plan…* button stays where it is and works from either tab.
- Motion: when the scenario changes, the new route fades in over 150 ms; no animation under `prefers-reduced-motion`. Optional; skip if fiddly.
- Pure helpers (`route_headline`, `turn_text`, glyph choice, collapse display) unit-tested in `ui`.

## Help wiki
- `what-if.md`: a *Route* section (old and new route, turn by turn, what absorbs means, Details tab has the full tables); update any sentence that describes the screen's first view.
- `schedule.md`: one line: "the critical path is the *route*; the what-if screen shows how a slip recalculates it".

## Demo data
No schema change. The demo-data what-if snapshot (5-day slip, spec 14) now includes `routes`; review the diff: the EU Region route must show the slip passing on and arriving late, consistent with the existing numbers.

## Export, undo, sync
Nothing stored.

## Out of scope
A "recalculating" toast after ordinary edits (needs before/after schedules on every write; see Open questions); routes for objectives; editing the scenario from the timeline.

## Acceptance criteria
- [ ] `ImpactReport.routes` computed in core with the listed tests and proptest; impact snapshots updated and reviewed.
- [ ] What-if has Route and Details tabs; Route shows headline, mini timeline with both routes, turn list; Details unchanged.
- [ ] Unchanged projects collapse to one line; very long chains collapse to ≤ 8 turns.
- [ ] Help updated; CHANGELOG; knowledge graph (§5 `mod:impact`, `mod:schedule` `chain_of`, §8 what-if); `docs/progress.md`; gate passes.

## Not yet verified by hand
- Demo: open a critical EU Region task, *What if this slips?* (5 days): Route tab shows EU Region recalculated with a later arrival; the turn list matches the Details tab numbers.
- Slip a task with plenty of slack: the card says no change to the route.
- Add a second slip in another project: two cards.
- Apply to plan from the Route tab: same preview as before.

## Open questions
1. After an ordinary date change on a critical task, show a toast "Route recalculated: EU Region now arrives … (+3)"? **Later, separate doc**: it needs the command layer to compute the schedule before and after a write.
2. Should Route be the default tab? **Yes.**
