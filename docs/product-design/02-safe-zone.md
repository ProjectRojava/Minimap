# 02 — Safe zone: the room left before a project's target

Status: Draft · Milestone: — · Priority: Should · Size: S
Depends on: specs 05, 13, 15 (implemented) · Read first: [README](README.md)
Expected ADR: none (a new view of existing schedule data)

## Goal
In battle-royale games, a shrinking circle tells you how much room you have left and how fast it's going. For a project, the room is the **working days between when it will really finish (the schedule's projected finish) and when it has to (its target date)**. Today that number appears only as "projected N days late" once it's already negative. Show it **before** it runs out, as a small gauge that reads in half a second, on every place a project appears.

## User stories
- On the Projects board each card has a thin bar: most of it is the work left, the end is the **safe zone**, "6 days of room".
- When the room is down to 2 days the bar's zone is amber and says "2 days of room".
- When the projected finish is past the target, the bar overflows its track in red: "3 days past target".
- A project with no target date shows "No target", and a project with no open work shows nothing.

## Rules (`minimap-core`, pure)
New module `crates/minimap-core/src/zone.rs`.

```rust
pub enum Zone {
    /// Nothing to show: done/cancelled/paused/planned-without-tasks, or no open tasks.
    None,
    /// The project has open work but no target date.
    NoTarget { remaining: u32 },
    /// Finishes on or before the target.
    Inside { remaining: u32, room: u32, level: ZoneLevel },
    /// Finishes after the target.
    Outside { remaining: u32, over: u32 },
}
pub enum ZoneLevel { Wide, Narrow, Closing } // room > narrow, 1..=narrow, 0
pub fn of(forecast: &ProjectForecast, status: ProjectStatus, today: Date, week: &WorkWeek, narrow_days: u32) -> Zone;
```
- `remaining` = working days from today (inclusive, using `schedule::working_index`) to `projected_finish` (inclusive); 0 if the finish is today or earlier.
- `room` = working days strictly after `projected_finish` up to and including `target_date`. Equal dates → `room = 0` → `Closing`.
- `over` = `forecast.late_by_days` (already working days); never 0 when `Outside`.
- Status `done`, `cancelled` → `None`; `paused` → `None` (a paused project's clock isn't running); `planned` and `active` are computed. `open_tasks == 0` → `None`.
- `narrow_days` default **3** (setting below). `Narrow` when `1 <= room <= narrow_days`.

The zone does **not** feed health: health already judges lateness (spec 15). It shares the same schedule run, so they can't disagree about the dates.

**Tests**: each variant; boundary (room 0, 1, 3, 4); a work week that isn't Mon–Fri (e.g. Sun–Thu) counts correctly; weekends between finish and target don't count as room; a paused project is `None`. **proptest**: for any forecast with a target, exactly one of `Inside`/`Outside` holds and `room`/`over` match `target - finish` in working days with the sign flipped.

## Data
- No migration.
- **Setting**: `HealthThresholds` gains nothing; add `zone_narrow_days: u32` (default 3, 0..=30) to `Settings` / `UpdateSettings` (synced). Validate in `store::settings`.
- **Types** (`crates/minimap-types/src/schedule.rs`): `Zone`, `ZoneLevel` (serde, tagged enum).
- **DTOs**: add `zone: Zone` to `ProjectForecast`? **No**: `ProjectForecast` is built inside `schedule::compute` which knows nothing about settings. Instead add `zone: Zone` to the *rows* the UI already loads for projects:
  - `ProjectHealthRow` (Overview, health sections),
  - the Projects list/board rows (find the row type returned by `list_projects` in `crates/minimap-types/src/views.rs`),
  - `ProjectDetail` (project panel).
  Each command that builds those already has (or can cheaply get) the schedule; compute `zone::of` there. If `list_projects` doesn't run the schedule today, run `schedule::compute` once for all projects (the same call `get_portfolio_overview` makes) and look forecasts up by id. Measure: the Projects page must still answer in well under 100 ms on the demo data.

## Commands
No new command. Changed: `list_projects`, `get_portfolio_overview`, `get_project_detail` fill `zone`. Keep `*_at(conn, today)` variants for tests.

## UI
New component `ui/src/components/zone.rs`:
- **`ZoneBar { zone, compact: bool }`**: a flat 4px-high track (`bg-hover`), full width of its slot.
  - `Inside`: the first `remaining / (remaining + room)` of the track filled `bg-muted`-ish (use the `fg` token at low opacity via an existing utility; no raw colours); the rest is the zone, filled with `success` (Wide) or `warning` (Narrow/Closing) at the theme's chip tint strength.
  - `Outside`: track filled to 100% `muted`, plus a `danger` segment extending past a 1px `fg` tick that marks the target; the overflow is drawn inside the slot by scaling: track = `remaining`, target tick at `(remaining - over) / remaining`.
  - `NoTarget`: an empty track with a dashed `border-line` end and no colour.
  - `None`: renders nothing.
  - Always followed (or, when `compact`, titled) by text from pure `zone_text(zone)`: "6 days of room", "2 days of room", "No room left", "3 days past target", "No target". The word carries the meaning; colour is second (`docs/design.md`).
- **Where**: Projects board card (compact, under the title), Projects list (a new narrow *Room* column, `hidden @[46rem]:block` like Owner/Target so the title never collapses), Overview project rows (compact), project panel pill row (a pill with `zone_text`, tone by level) and the Schedule section (full bar above the timeline, with the target tick lined up visually is **not** required).
- Pure helpers in the module (`zone_text`, `zone_tone`, `fill_fractions(zone) -> (f64, f64)`) with unit tests.
- **Settings → Thresholds**: a *Safe zone* line in the project health card: "Call the zone narrow at [3] days of room or fewer" (`NumberSetting`).

## Help wiki
- `projects.md`: a *Safe zone* section (what room means, working days, the three looks, paused projects show none, where to change the narrow threshold).
- `schedule.md`: one paragraph linking to it ("the safe zone is the room between the projected finish and the target").
- `settings.md`: the Thresholds tab text; quote the default `3` exactly (the help test compares quoted defaults).
- `overview.md`: the mark on project rows.

## Demo data
No schema change, so no seed change is required. Check the story at the fixed date 2027-03-03 and record it in the demo snapshot (`src-tauri/src/commands/demo.rs`, the overview snapshot will now include `zone`): EU Region (amber health) should be `Outside` or `Narrow`; Platform Cost Reduction `Inside` and wide; Security and Compliance whatever it computes. If none of the three shows `Narrow`, adjust nothing in the seed: say so in the PR; the hand-built tests cover each variant.

## Export, undo, sync
Nothing stored except the setting (syncs like other settings). Not undoable (settings aren't).

## Out of scope
History of the zone ("lost 4 days of room in two weeks") — needs stored forecasts; see Open questions. Zones for objectives. Zones on tasks (that's slack, already in the schedule panel).

## Acceptance criteria
- [ ] `core::zone` tests and proptest pass.
- [ ] `list_projects`, `get_portfolio_overview`, `get_project_detail` return `zone`; command tests at a fixed date cover Inside/Outside/NoTarget/None.
- [ ] `ZoneBar` appears on the board, list, Overview and panel; text is always present; colour never alone (ui unit tests on the pure helpers).
- [ ] Setting validated and persisted.
- [ ] Demo snapshot reviewed; story holds.
- [ ] Help, CHANGELOG, knowledge graph (§4 types, §5 `mod:zone`, §7 changed commands, §8 component), `docs/progress.md` updated; gate passes.

## Not yet verified by hand
- Projects board on the demo data: each active project has a bar and words; EU Region's is amber or red.
- Move a task's estimate up in Platform Cost Reduction until the projected finish passes the target: the bar turns red and reads "N days past target".
- Pause a project: its bar disappears.
- Change Sun–Thu working days in Settings: the room changes accordingly.

## Open questions
1. Track the zone over time (store one forecast per project per day, a "shrinking" sparkline)? **Not now.** It needs a derived-data table that must be excluded from activity/undo and decided for sync; write a separate doc if wanted.
2. Should `planned` projects show a zone? **Yes** (they have a target and a schedule); revisit if noisy.
