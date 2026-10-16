# 01 — Fog of war: show how old what you know is

Status: Implemented as spec 43 — awaiting manual check (open questions answered as recommended) · Milestone: — · Priority: Should · Size: M
Depends on: specs 13, 15, 16, 19, 30, 36, 37 (all implemented) · Read first: [README](README.md)
ADR: [ADR-0019](../decisions/ADR-0019-fog-as-a-view.md) *Fog of war: staleness is a view, not a status*

## Goal
A project can read *On track* only because nobody has updated it in three weeks. Health says what the data says; **fog says how far to trust the data**. Items that are in play but that nobody has touched or confirmed lately should **look their age**, everywhere they appear, and there should be a quick way to clear the fog: look at an item, then either change it or say it is still accurate.

In a game, fog covers the parts of the map you haven't scouted recently. The weekly review is the scouting run.

## User stories
- On Overview I see *Platform Cost Reduction* faded with a `≋ 19d` mark. I hover: "No change for 19 days. Is it still on track?" I open it, read it, press **Still accurate**, and it clears.
- The Overview header shows **Visibility 62%**: 62% of what's in play has been touched or confirmed this week or so.
- The weekly review has a **Fog** step listing everything fogged, each with *Still accurate*, *Open* and the usual quick fixes. When I finish the step, the step button says *All clear*.
- A task due in six months that nobody has touched is **not** fogged: it isn't in play yet.
- On **This week**, a task in my plan that nobody has touched for nine days carries the same `≋ 9d` chip. A section **Gone quiet** under the plan lists the fogged tasks that are *not* already on the page (an in-progress task of a report's, due next month), each with *Still accurate* and *Open*, so what I have lost sight of comes to me instead of waiting for the weekly review.

## Concepts and rules (all in `minimap-core`, pure)

### What is "in play"
Only items whose state matters now can be fogged. Closed and far-off items never are.

| Kind | In play when |
|---|---|
| Task (not a meeting) | open (todo, in_progress, blocked) **and** any of: status in_progress or blocked; due date within `FOG_TASK_HORIZON_DAYS` (14 calendar days) or past; in focus today (spec 37) |
| Project | status `active` (planned, paused, done, cancelled are not in play) |
| Objective | not done; for an ongoing objective (spec 30), fog is **not** computed: it already has a review rhythm and an overdue review turns it amber. Show the review instead (see *Ongoing objectives* below) |
| Waiting-on | not in scope: it already has *stale* (spec 08) |
| Person, team, note, decision | never |

### When an item was last seen
`last_seen(item)` = the latest of:
1. the item's own `updated_at` (any field change, including status, dates, focus);
2. `confirmed_on` (new, see *Data*), read as the start of that day UTC;
3. for a task: the newest note that mentions it (`note_items`, by `note_date`, falling back to `created_at`); the newest activity row on any of its sub-tasks (spec 33) is **not** counted (a busy child doesn't vouch for the parent);
4. for a project: the latest `last_seen` of its open tasks that are in play (a project whose tasks move is being watched) **and** the newest note mentioning the project;
5. for an objective: the latest `last_seen` of the projects and tasks that contribute to it, and the newest note mentioning it.

Activity on links (`edge_added`/`edge_removed`) counts as a change to the `from` node only (it already updates nothing on the node row, so read the activity log: `activity::list_for_node` is too slow per item, add a store view, see *Store*).

`age_days(item, today)` = whole calendar days from `last_seen` to `today`, floored at 0.

### Levels
`FogLevel { Clear, Haze, Fog }` from `age_days` against per-kind thresholds:

| Kind | Haze after (days) | Fog after (days) |
|---|---|---|
| Task | 7 | 14 |
| Project | 10 | 21 |
| Objective | 14 | 30 |

Strictly greater than: a task 7 days old is Clear, 8 is Haze. Thresholds are user settings (*Data*), validated `1 <= haze < fog <= 365`.

### Visibility
`visibility(items)` = Σ weight(clear items) ÷ Σ weight(all in-play items), as a whole percent, where weight = `health::priority_weight(priority)` × (`health::scope_weight`: project 1, task 0.6, objective 1). `None` when nothing is in play (show "—", never 100%). Haze counts as half-clear (weight × 0.5) so the number moves smoothly.

### Ongoing objectives
Not fogged. Where fog marks would appear for objectives, an ongoing objective whose review is overdue shows the existing review pill (spec 30) instead. Visibility leaves ongoing objectives out.

### Fog on This week
This week is where fog is most useful, so it shows it in two ways, both pure reads of the `FogReport`:
1. **Marks on rows.** Every task row This week already shows (*Needs attention*, *Focus*, *Priorities*; not meetings, which are never in play) carries its `FogChip` and the `.fog-haze` / `.fog-fog` look when it is fogged. A red flag is never hidden by fog: an overdue task in fog shows the overdue pill and the chip.
2. **A section _Gone quiet_** after *Priorities this week* (the card, `aria-label="Gone quiet"`, `Tone::Neutral`). It lists the fogged tasks (Fog first, then Haze, oldest first) that are in none of *Meetings*, *Needs attention*, *Focus* or *Priorities*, at most `QUIET_LIMIT` (8) of them; when there are more, a line says "and N more in the weekly review's Fog step". It is made in `core::this_week` (`WeekInput.fog: Option<FogReport>` in, `ThisWeek.quiet: Vec<QuietTask>` with `QuietTask { task: WeekTask, mark: FogMark }` out) so the rows can show assignee, project, notes and the usual actions. Rows are in the `j/k` order after *Priorities*, and `c` confirms the row under the cursor.
3. **Only for the live view.** Fog is about how stale things are *now*: when the screen looks back at a past day (`as_of`) there is no fog on This week (no chips, no section; the data cannot be rewound that far). A future week shows the chips (the facts are true today) and the section.
4. The section's tasks are everyone's, not only mine: a CTO's blind spots are mostly in other people's work. The row shows the assignee.
5. The tiles at the top and the headline do **not** count fog (they count red flags and the plan): fog is a nudge, not an error. The empty state ("Nothing needs attention") is shown only when the section is empty too.

### What fog does **not** do
It never changes health, status, the schedule, capacity, impact or any stored value. It is a way of looking (like focus, spec 37). Fog is not a red flag: an overdue task in fog is listed under *Needs attention* as before and also shows its fog mark.

## Data
- **Migration `0017_fog.sql`**: `ALTER TABLE tasks ADD COLUMN confirmed_on TEXT NULL; ALTER TABLE projects ADD COLUMN confirmed_on TEXT NULL; ALTER TABLE objectives ADD COLUMN confirmed_on TEXT NULL;` (ISO date). No index needed.
- **Types** (`minimap-types`): `confirmed_on: Option<Date>` on `Task`, `Project`, `Objective`; `Patch<Date>` field on `UpdateTask`, `UpdateProject`, `UpdateObjective` (so add each to `store::undo::nullable_fields`; the existing test will insist). Not on `Create*` (a new item is clear because it was just created).
- **Settings**: a new synced key `fog` holding `FogThresholds { task_haze, task_fog, project_haze, project_fog, objective_haze, objective_fog }` (u32 days) with `Default` = the table above and `validate()` (pattern: `HealthThresholds` in `crates/minimap-types/src/health.rs`). `Settings.fog`, `UpdateSettings.fog: Option<FogThresholds>`. Not in `settings::LOCAL_ONLY` (it follows you across devices).
- New types in a new file `crates/minimap-types/src/fog.rs`: `FogLevel`, `FogMark { node: NodeRef, label: String /* for lists that show only marks */, level: FogLevel, age_days: u32, last_seen: Date, reason: FogReason }` where `FogReason { NoChange, OnlyConfirmed }` (for the hover text), `FogReport { today, marks: Vec<FogMark> /* only Haze and Fog, oldest first */, visibility: Option<u8>, in_play: u32, clear: u32 }`. Every view DTO that lists tasks, projects or objectives does **not** gain a field: the UI looks marks up by id from one `FogReport` (see *UI*), so no existing DTO changes.

## Core (`crates/minimap-core/src/fog.rs`, new module)
```rust
pub struct FogInput<'a> {
    pub today: Date,
    pub thresholds: FogThresholds,
    pub tasks: &'a [FogTask],       // id, status, is_meeting, due, focus, priority, updated_at, confirmed_on, project_id
    pub projects: &'a [FogProject], // id, status, priority, updated_at, confirmed_on
    pub objectives: &'a [FogObjective], // id, ongoing, status, priority, updated_at, confirmed_on
    pub contributes: &'a [(Uuid, Uuid)], // (project or task, objective)
    pub last_link_change: &'a HashMap<Uuid, OffsetDateTime>, // from activity, per from-node
    pub last_mention: &'a HashMap<Uuid, Date>, // newest note date mentioning each node
}
pub fn in_play(task: &FogTask, today: Date) -> bool;
pub fn last_seen(...) -> Date;   // per kind, as above
pub fn level(age_days: u32, haze: u32, fog: u32) -> FogLevel;
pub fn build(input: &FogInput) -> FogReport;
pub fn visibility(items: &[(FogLevel, f64 /*weight*/)]) -> Option<u8>;
```
Keep the input structs small and in core (or reuse `TaskRow` etc. if simpler; the command builds them). `build` is O(n + edges).

**Tests** (`#[cfg(test)]` in the module): each in-play rule; each `last_seen` source wins when newest; level boundaries (7 → Clear, 8 → Haze); a project kept clear by a moving task; an objective kept clear by a contributing project; ongoing objective excluded; meetings excluded; closed items excluded; visibility weights and `None`; haze half-weight. **proptest**: (a) confirming any item (setting `confirmed_on = today`) never makes any level worse and makes that item Clear; (b) visibility is within 0..=100; (c) `marks` contains no Clear item and no item out of play.

## Store
- `views::last_link_changes(conn) -> HashMap<Uuid, OffsetDateTime>`: one query, `SELECT node_id, MAX(at) FROM activity WHERE action IN ('edge_added','edge_removed') GROUP BY node_id`.
- `views::last_mentions(conn) -> HashMap<Uuid, Date>`: newest `note_date` (else created day) per node mentioned by an active note (via active `mentions` edges).
- `confirmed_on` read/written by the existing repos (`COLS` lists, `convert` helpers); a normal logged update (diff key `confirmed_on`), undoable.
- `settings`: get/update/validate `fog`.

## Commands
- `get_fog()` (no argument; `fog_at(conn, today)` takes the day, for tests) in a new `src-tauri/src/commands/fog.rs` (`fog_at(conn, today)` for tests): settings + `task_rows`/project/objective lists + the two views + edges(`contributes_to`) → `core::fog::build`. Returns `FogReport`.
- `confirm_item(node: NodeRef)`: task, project or objective only (else `invalid`); sets `confirmed_on = store::today()` through the repo's `update` (so it is logged and undoable). **It does not bump `updated_at`**: when `confirmed_on` is the only change the repo leaves `updated_at` alone, so an undo brings the fog back exactly (otherwise the undo itself would count as a fresh edit); see ADR-0019 for the sync consequence. Archived item → `invalid`. Answers the new `FogMark`-free state (`()`).
- Register both in the 4 places (README checklist).

## UI
- **Context** `FogCtx` (new `ui/src/components/fog.rs`), provided by `Shell` like `ObjectiveColours`: a version-keyed `LocalResource` over `get_fog` that keeps the last answer; `mark(id) -> Option<FogMark>`.
- **The look** (`input.css`, tokens only, no gradients): class `.fog-haze` lowers the row/card's text to `muted` and its icons/borders to `faint`; `.fog-fog` additionally sets `opacity: .6` on the row content (not on buttons, which keep full contrast so they stay usable) and nothing on borders (board cards already carry their task type's border, spec 39). The chip is a direct child (`.fog-keep`) so it stays at full contrast. Never colour-only: every fogged item also shows a `FogChip`.
- **`FogChip { mark }`**: a small chip `≋ 19d` (the glyph is text, no icon), title text from pure `fog_hint(mark)`: "Fog: no change for 19 days. Is it still on track?" (project/objective) / "Haze: no change for 8 days. Is it still accurate?" (task) / "Haze: last confirmed 15 days ago. …" (`OnlyConfirmed`).
- **Where marks appear**: `NodeRow` (lists of tasks, projects, objectives), `TaskCard` (Tasks board), Projects board cards, Overview objective and project rows, This week rows (`FlagRow`, `FocusRow`, `PlanRow`, and the new `QuietRow`). Add an optional `fog: Option<FogMark>` prop where needed; the parent reads `FogCtx` (on This week it is `None` when looking back, see *Fog on This week*).
- **This week**: the *Gone quiet* card described above (rows: title, project, assignee, the chip with the age and a *Still accurate* button, *Open* is the row click). `Hints` gain `("c", "still accurate")`.
- **Detail pane**: in the pill row (`summary_chips`), a *Fog* pill with the age; next to it a **Still accurate** button (`BUTTON_SOFT`, only when Haze/Fog) → `confirm_item` → toast "Confirmed. Clear for 7 days." (days = the haze threshold for that kind) with the undo hint → `DataVersion` bump.
- **Row key `c`** (add to `nav::is_row_key`): confirm the cursor row (task/project/objective lists, This week, the weekly review). On a row that is not fogged it confirms anyway (harmless: it only stamps today). Check `c` isn't already a row key on any screen when implementing (today it is only the `g c` chord, which is not a row key).
- **Overview**: a `Stat` tile **Visibility 62%** (`—` when nothing in play) with title text "Share of active work touched or confirmed recently". Under 50% the number is `warning`, never `danger` (fog is a nudge, not an error).
- **Weekly review** (`ui/src/pages/weekly_review.rs`): a new step **Fog** before *Report* (`STEPS` becomes 9; `LAST_STEP` follows). Lists `FogReport.marks` (Fog first, then Haze, oldest first) as `NodeRow`s with *Still accurate*, *Open*, and for tasks the `TaskActions` reschedule buttons. The step button count is the number of marks; at zero the step shows `EmptyState` "All clear. Nothing in play has gone quiet." Add the step to `core::weekly_review`? **No**: the step reads `get_fog` directly; the review's data is unchanged. The Markdown report gets **no** fog section (an exec report shouldn't expose the author's bookkeeping); see Open questions.
- **Settings → Thresholds**: a *Fog* card with the six numbers (`NumberSetting`, same pattern as health thresholds), a sentence "Items in play fade after this many days without a change. Confirming clears them." and Reset to defaults.

## Help wiki
- New page `docs/help/fog.md` (*Fog: how fresh your information is*): what is in play, what counts as a change, the levels and defaults (quote the defaults exactly; the help test compares quoted defaults with `Default`), Still accurate and `c`, Visibility, the review step, what fog never changes. Add to `PAGES` (group "Planning and insight"), link from `overview.md`, `weekly-review.md`, `this-week.md`, `settings.md` (the Thresholds tab text), `keyboard.md` (`c`).
- `weekly-review.md`: the steps list gains *Fog*.
- `this-week.md`: the chips on rows, the *Gone quiet* section, that it is absent when looking back, `c`.

## Demo data (`crates/minimap-store/src/demo.rs`)
The seed's `backdate` already places activity in the past. Make the story include fog:
- *Platform Cost Reduction* (active) and all its in-play tasks last touched **22** days before `today` → project Fog (a project fogs after 21 days, so 19 would only be haze) and its tasks Fog.
- Two tasks in play in *Security and Compliance* ("Choose a penetration-test vendor" in progress, "Monthly access review" due in 14 days) last touched 9 days ago → Haze.
- *EU Region* stays clear (it's the busy one; it must stay amber for health).
- One task with `confirmed_on` = 3 days ago and `updated_at` 20 days ago → Clear, with `OnlyConfirmed` hover text available if it later hazes.
Update `DemoSummary` only if it gains a count (it needn't), the store demo tests (assert the fog levels at the fixed date), the snapshot (add a `fog` snapshot in `src-tauri/src/commands/demo.rs`: the `FogReport` at 2027-03-03), `docs/features/24-demo-data.md`. Visibility at the fixed date goes in the snapshot.

## Export, undo, sync
- Export: `confirmed_on` is a column on exported structs, so JSON gets it automatically; `export_md` needs nothing.
- Undo: `confirmed_on` is a `Patch` field → `nullable_fields`. Confirming is one undo step.
- Sync: plain columns merge with their row (newest `updated_at` wins). The `fog` setting syncs like `health`.

## Out of scope
Fog on people, teams, notes, decisions; a fog section in the Markdown report; notifications about fog; fog in the radar (doc 10 consumes `FogReport` itself); per-project custom thresholds; "confirm all".

## Acceptance criteria
- [x] `core::fog` rules and proptests pass (list in *Core*).
- [x] Migration 0017 adds `confirmed_on` to the three tables; old databases open; `confirm_item` writes one logged update, is undone and redone exactly (store `undo` test).
- [x] `get_fog` at a fixed date on a hand-built world returns the expected marks and visibility (`commands::fog` test); the demo snapshot shows Platform Cost Reduction in Fog, two Security tasks in Haze and EU Region clear.
- [x] Settings validate and persist `fog`; a bad value is refused with a message and leaves the stored one.
- [x] Fog marks appear on rows, cards, Overview and This week; never colour-only; buttons keep full contrast (pure `fog_hint`, class helpers tested in `ui`).
- [x] This week: fogged tasks show the chip and the faded look on every task row, a *Gone quiet* section lists the fogged tasks not already on the page (at most 8, "and N more" line), it is absent when looking back, red flags and tiles are unchanged, `c` confirms the row (`core::this_week` test with a hand-built `FogReport`; pure UI helpers tested).
- [x] Weekly review has a Fog step before Report; `n`/`p` still walk all steps; the report text is unchanged (existing `insta` snapshots unchanged).
- [x] Help page added and linked; help tests pass; CHANGELOG, knowledge graph (§3.1 columns, §4, §5 `mod:fog`, §6 views, §7 table, §8, §12) and `docs/progress.md` updated.
- [x] Gate passes (README checklist).

## Not yet verified by hand (for the manual check)
- Seed demo data: Platform Cost Reduction looks faded on Overview, Projects (list and board) and its tasks on the Tasks board, with `≋ 19d`.
- Open it, press *Still accurate*: it clears everywhere; `Ctrl/Cmd+Z` brings the fog back.
- On Tasks press `j` to a hazy task, then `c`: it clears.
- Weekly review → Fog lists the same items; confirming them empties the step.
- Settings → Thresholds → Fog: set task haze to 30: hazy tasks clear at once.
- This week with demo data: the Security tasks show `≋ 9d`; *Gone quiet* lists Platform Cost Reduction's tasks that the plan does not show; *Still accurate* on one removes it from the section; `Ctrl/Cmd+Z` brings it back; clicking a past day in the strip shows no fog.

## Open questions (answered: the recommendations were accepted)
1. Should fog lower a project's health (e.g. a reason "No update for 21 days")? **No: keep them orthogonal; revisit after use.**
2. Should the Markdown status report mention data freshness? **No.**
3. Should changing a sub-task count for its parent? **No** (stated above).
4. Should Visibility appear on This week too? **No: Overview only; This week stays about action.** (This week does show the fogged *tasks*, see *Fog on This week*, but not the percentage.)
