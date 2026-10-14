# Product design: the minimap

Design docs for a set of features that come from the product's name. Each doc is written to be handed to an AI agent (or a person) on its own: it says what to build, where it goes in the code, what it must not break, how it is tested and when it is done.

These are **design docs, not yet specs**. Every doc starts as `Status: Draft`. To build one:

1. Read it, answer its *Open questions* (or accept the recommended answers), and set `Status: Ready`.
2. Hand it over: *"implement docs/product-design/01-fog-of-war.md"*.
3. The agent registers it in [`docs/features/README.md`](../features/README.md) as the next numbered spec (a row that links to this file; the file stays here), implements it, ticks the acceptance criteria, sets `Status: Implemented — awaiting manual check`, and updates `docs/progress.md`, `docs/knowledge-graph.md`, the Help wiki and `CHANGELOG.md`.

`CLAUDE.md` still applies to all of it. Where a doc disagrees with `CLAUDE.md`, the doc wins once it is `Ready`, and the agent records the deviation as an ADR (each doc names the ADRs it expects).

---

## The idea

In a game, the **minimap** is the small map in the corner of the screen. You don't play on it; you glance at it for half a second and look back at the game. In that glance it tells you:

- **where you are** (you are always the centre),
- **where you're going** (the objective marker, the route line),
- **what's coming at you** (enemy blips, arrows at the edge pointing at things off-screen),
- **where your team is** (ally markers, party frames),
- **what you don't know** (fog of war over the parts you haven't scouted lately).

The people Minimap is for (CTOs, CXOs, PMs) don't do their real work in a planning tool; they do it in meetings, 1:1s and decisions. So:

> **Minimap should be glanced at, not lived in.** It succeeds when you look away sooner and better informed.

## Principles (every doc follows these)

1. **You are the centre.** The default is your week, your exposure, your people. The self person (`people.is_self`) is the anchor.
2. **Signal at the edges.** Threats arrive as small, specific marks at the edge of what you're looking at, not as another dashboard to read. Colour is scarce (`docs/design.md`), so when it shows up it means something.
3. **Fog of war is honest.** Show what you *don't* know. Information nobody has touched in a while must look its age.
4. **The map is not the territory.** This is a private model of the organisation. Nothing in these features is shared, scored or reported on anyone's behalf.
5. **Use the metaphor, not the game.** No XP, levels, streaks, badges, leaderboards, sounds, confetti or game art. The metaphor shapes how things *work*, never how they're decorated. People are colleagues, not party members to be rated: nothing here ranks, scores or judges a person's performance.
6. **Calm by default.** No new notifications or pop-ups that interrupt. Motion is rare, short, and off under `prefers-reduced-motion`.

## Shared vocabulary

Use these words in the UI and the Help wiki, and keep them consistent across features.

| Word | Means | Introduced by |
|---|---|---|
| **Fog** / **haze** / **clear** | How long since something in play was touched or confirmed | 01 |
| **Confirm** ("Still accurate") | Saying an item is still true without changing it | 01 |
| **Visibility** | The share of in-play work that is clear | 01 |
| **Safe zone** | Working days of room between the projected finish and the target | 02 |
| **Approaching** | A problem outside what you're looking at that will reach it | 03 |
| **Route** | The critical path, drawn as the line to the finish | 04 |
| **Recalculated** | The route after a slip | 04 |
| **Ping** | A marker you drop on any item to come back to it on a day | 05 |
| **Your people** | You, your direct reports, and their load, waiting-ons and 1:1 cadence | 06 |
| **Portfolio view** / **My view** | Seeing everything, or everything as it touches you | 07 |
| **Save point** | Any restorable copy: a local backup or a Drive checkpoint | 08 |
| **Zoom out / zoom in** | Moving between objective, project, task and the item itself | 09 |
| **Radar** | The small always-available minimap window | 10 |

Avoid: "health" for a person, "score" for a person, "XP", "quest", "level up", "enemy", "player".

## The docs, in suggested build order

| # | Doc | What it is | Depends on (here) | Size |
|---|---|---|---|---|
| 01 | [Fog of war](01-fog-of-war.md) (implemented as spec 43) | Items in play that nobody has touched in a while fade; a *Visibility* figure; a weekly-review step that clears fog | — | M |
| 02 | [Safe zone](02-safe-zone.md) (implemented as spec 44) | A gauge per project of the room left before its target | — | S |
| 03 | [Approaching: off-screen indicators](03-off-screen-indicators.md) (implemented as spec 45) | Marks at the edge of a project / your week naming problems elsewhere that will reach it | — | M |
| 04 | [Route recalculation](04-route-recalculation.md) (implemented as spec 46) | Impact analysis shown as the old and new route, step by step | — | M |
| 05 | [Pings](05-pings.md) | Drop a marker on any item to come back to it on a day | — | L |
| 06 | [Your people](06-your-people.md) | Compact frames for you and your reports: load, waiting-ons, 1:1 cadence | — | M |
| 07 | [Portfolio view and My view](07-orientation.md) | One toggle: the organisation as it is, or as it touches you | 03 (reuses its reach) | M |
| 08 | [Save points](08-save-points.md) | One timeline of every restorable copy, a "Saved 2 min ago" mark, named saves | — | M |
| 09 | [Zoom](09-semantic-zoom.md) (implemented as spec 52) | Zoom out from a task to its project, objective and the whole portfolio, and back | — | M |
| 10 | [Radar window](10-radar-window.md) | A small, optionally always-on-top window: you at the centre, rings of time, sectors of objectives | 01, 02, 03, 05, 07 | L |
| 13 | [Splash screen](13-splash-screen.md) | The mark and one calm line while the app starts, then it fades out | 11 | S |
| 12 | [Dependencies, richer](12-dependency-graph.md) (implemented as spec 47) | Boxes that say who, what and when; lenses; trace a chain; edit the links on the graph | — | L |
| 11 | [Brand and voice](11-brand-and-voice.md) | Logo, icon, the words the app uses, empty states | — (any time) | S |

Sizes: S = a day or two of agent work, M = a few days, L = a week or more. The radar window comes last because it is mostly a new way to *show* what 01, 02, 03, 05 and 07 compute.

## Rules every doc inherits (the agent's checklist)

Each doc repeats the parts that apply to it; this is the full list.

- **Layering** (`CLAUDE.md` §3): rules and calculations are pure functions in `minimap-core` with unit tests (and `proptest` where a property exists); SQL only in `minimap-store`; commands thin; the UI depends only on `minimap-types` and has no business rules. Anything the UI needs that is a rule (sorting, grouping, thresholds) is computed in core and returned by a command.
- **A new command = 4 places**: the fn + `generate_handler!` in `src-tauri/src/main.rs`; the name in `src-tauri/build.rs` `COMMANDS`; `allow-<kebab-name>` in `src-tauri/capabilities/default.json`; a wrapper in `ui/src/api.rs` (argument structs with multi-word fields need `#[serde(rename_all = "camelCase")]`).
- **A schema change** (migration, table, column, node or edge type): next migration number (the latest is `0016`), demo data in `crates/minimap-store/src/demo.rs` + its tests + `insta` snapshots + `demo_remove`, the full export (`DataExport`, `store::export::collect`, `commands/export.rs`, `core::export_md::readme`), undo (`minimap_core::undo`, `store::undo::nullable_fields`), sync merge (`store::merge`: is it per-row with `updated_at`? device-local?).
- **Every write** is a transaction through a repository function that also writes an `activity` row.
- **Design** (`docs/design.md`): theme tokens only, flat (no shadows, gradients, blur), colour has five jobs, colour is never the only signal, `faint` never carries information, dense, no native pop-ups.
- **Help wiki** (`docs/help/`, `CLAUDE.md` §11): every change a user can see updates the matching page in the same change; a new page is a file plus a line in `PAGES` (`ui/src/help/mod.rs`) and a link from a related page. Every new shortcut chord appears on `keyboard.md` (a test checks it).
- **Knowledge graph** (`docs/knowledge-graph.md`): update the sections its §14 lists, and bump "Last verified against code".
- **Changelog**: one line under `[Unreleased]` (Added/Changed/Fixed), user-facing words.
- **Gate**: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `MINIMAP_GOOGLE_CLIENT_ID= MINIMAP_GOOGLE_CLIENT_SECRET= cargo test --workspace`, and the demo snapshots reviewed (`INSTA_UPDATE=always cargo test -p minimap demo`, read the `.snap` diff: EU Region still amber, one overloaded person, one stale waiting-on).
- **Numbers in these docs are provisional**: migration numbers (`0017`…) and ADR numbers (`ADR-0019`…) assume nothing else landed first. Take the next free number when implementing and fix the doc.
- **Fixed today in tests**: command tests take today as a parameter (`*_at` functions); the demo snapshots use Wednesday 2027-03-03.
