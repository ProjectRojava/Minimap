# 10 — Radar: the minimap itself, a small window you glance at

Status: Draft · Milestone: — · Priority: Should · Size: L
Depends on: docs 01 (fog), 02 (safe zone), 03 (approaching), 05 (pings), 07 (My view); specs 13, 15, 16 (implemented) · Read first: [README](README.md)
Expected ADR: ADR-0021 *A second window: the radar* (window label, its narrow capability, how it asks the main window to open an item)

## Goal
This is the feature the product is named after. A **small, frameless window** (about 260×260) that can stay on top of other apps, showing your world **centred on you**:
- **rings** are time horizons (this week, next two weeks, this quarter),
- **sectors** are your objectives, in their objective colours,
- **blips** are the items that matter, placed by how soon they're due and which objective they serve, sized by priority, shaped by health,
- **edge marks** are threats approaching your work (doc 03),
- **fog** dims what nobody has touched lately (doc 01),
- **pins** are your pings (doc 05).

It is for glancing during a meeting or between calls. Hover tells you what a blip is; click brings the main window forward with that item open. No editing happens in the radar.

## User stories
- I press the radar button in the title bar (or `Ctrl/Cmd+Shift+M`). A small round-cornered window appears in the screen corner where I left it last time. It stays on top while I'm in a video call.
- I see three blips close to the centre in the *Launch in the EU* sector, one red (filled dot), one amber (half dot), and a pulsing mark on the outer edge in that sector: something approaching. I hover the mark: "Gateway auth (EU Region) is blocked; reaches *Pen test* in 3 days."
- I click a blip: the main window comes forward with that task open in the pane.
- In the radar's corner a small "N"/"▲" shows whether it's in Portfolio view or My view (doc 07); clicking toggles it, and the main window's view follows (and vice versa).
- At the bottom edge: "2 overdue · 1 ping today · visibility 64%".

## Layout model (`minimap-core`, pure)
New module `crates/minimap-core/src/radar.rs`. Core decides **what** goes **where** in abstract polar units; the UI turns that into pixels.

```rust
pub struct RadarInput<'a> { today, week: &WorkWeek, view: View, scope: Option<&MyScope>,
    tasks, projects, objectives (with colour rank), schedule: &Schedule,
    overview: &PortfolioOverview /* health */, fog: &FogReport, pings: &PingsView,
    approaching: &ApproachReport, rings: [u32; 3] /* working days, default [5, 10, 60] */ }
pub struct Radar {
    pub sectors: Vec<Sector>,  // objectives in scope, + "Other" (no objective) last
    pub blips: Vec<Blip>,
    pub edge_marks: Vec<EdgeMark>,
    pub footer: Footer,        // overdue, due today, pings due, visibility, approaching count
    pub revision: i64,         // activity::latest_rowid at build time (see Refresh)
}
pub struct Sector { pub objective: Option<NodeSummary>, pub hue_rank: Option<u32>, pub start: f64, pub sweep: f64 /* radians, sum = 2π */ }
pub struct Blip { pub node: NodeSummary, pub kind: BlipKind /* Task, Project, Objective, Ping, Meeting */, pub r: f64 /* 0..=1 */, pub theta: f64, pub size: Size /* S M L from priority */, pub mark: HealthMark /* Red, Amber, Green, None */, pub fog: FogLevel, pub pinged: bool, pub label: String /* hover text */ }
pub struct EdgeMark { pub theta: f64, pub severity: Severity, pub label: String, pub source: NodeSummary }
pub fn build(input: &RadarInput) -> Radar;
```

Rules:
- **What becomes a blip** (in scope for the current view):
  - open tasks (not meetings) that are overdue, due within the outer ring, blocked, or in focus;
  - active projects whose target is within the outer ring or whose zone (doc 02) is Narrow/Outside;
  - today's and tomorrow's meetings (kind Meeting, small hollow squares; no health);
  - due pings whose target isn't already a blip (kind Ping); a blip whose item is pinged gets `pinged = true` instead.
  - Cap: at most `MAX_BLIPS = 60`; beyond that keep the most urgent (overdue first, then by due date, then priority) and add `footer.hidden_blips`.
- **Radius** `r` (0 = centre = you, 1 = outer edge): piecewise-linear in **working days until due/target** (`schedule::working_index` difference): overdue → `r` in `[0, 0.12]` (more overdue = closer to the centre, capped at 10 days); `0..=ring0` → `[0.12, 0.4]`; `ring0..=ring1` → `[0.4, 0.65]`; `ring1..=ring2` → `[0.65, 0.95]`. Blocked tasks with no due date: `r = 0.5`. In-focus tasks with no due date: `r = 0.3`.
- **Sectors**: one per in-scope, non-done objective, ordered by priority then title, plus *Other* for items serving none. Sweep ∝ `max(1, blips in sector)` but never below `MIN_SWEEP = 20°` and *Other* never above 90°. Start at 12 o'clock, clockwise.
- **Angle** `theta`: inside its sector's sweep, a deterministic position from the item id (`fnv1a(id)` mapped into the sweep minus 4° padding on each side), so a blip doesn't jump between refreshes. Then **collision nudge**: for blips whose positions (in unit coordinates) are closer than `MIN_GAP = 0.05`, nudge `theta` apart within the sector (iterate a few times; deterministic). An item serving several objectives appears **once**, in its highest-priority objective's sector.
- **Health mark**: tasks from `health::task_health` (overdue red, blocked amber), projects from the overview's project health. Size: P1 L, P2 M, others S.
- **Edge marks**: one per `Approaching` item with severity Hits/Threatens (doc 03, scope Me or the view's scope), at the target's sector angle (deterministic as above), on the rim.
- **Footer**: counts as listed; visibility from doc 01.

**Tests**: radius mapping at each boundary; overdue clamp; stable angles across runs and input order; collision nudge keeps all blips inside their sector and separated; sector sweeps sum to 2π with the minimums; MAX_BLIPS truncation order; My view vs Portfolio scope; edge marks only for Hits/Threatens. **proptest**: every blip has `0 <= r <= 1` and `theta` within its sector; `build` is independent of input order.

## Windows and capability (ADR-0021)
- **Window**: created by a Rust command, not from the webview (so the UI needs no `window:create` permission). `open_radar()` uses `tauri::WebviewWindowBuilder::new(app, "radar", WebviewUrl::App("index.html#/radar".into()))` — or the route form the app's router supports (it uses path routes; if hash routes aren't supported, load `/radar` the same way the main window loads `/`) — with `decorations(false)`, `resizable(true)`, `min_inner_size(200,200)`, `inner_size` from the saved size, `always_on_top(saved)`, `skip_taskbar(true)`, `transparent(false)`, `background_color` from the current theme kind. If the window exists, focus it; `close_radar()` closes it. `set_radar_on_top(bool)`.
- **Position/size memory**: on move/resize (window events in Rust), store `local.radar_geometry` (x, y, w, h, on_top) via `store::meta` (device-local, never synced). Clamp to a visible monitor on open.
- **Capability**: a **new** file `src-tauri/capabilities/radar.json` with `"windows": ["radar"]` and only: `allow-get-radar`, `allow-get-data-revision`, `allow-open-in-main`, `allow-close-radar`, `allow-set-radar-on-top`, `allow-get-settings` (theme), and `core:window:allow-start-dragging` (drag anywhere). The main window's `default.json` gains `allow-open-radar`, `allow-close-radar`. The radar can't call any write command.
- **Opening an item in the main window**: `open_in_main(node: NodeRef)` (Rust): shows, unminimises and focuses the `main` window, then `emit_to("main", "minimap://open", node)`. The main window listens (`window.__TAURI__.event.listen`) — this needs `core:event:allow-listen` in `default.json` **for the main window only**; record in the ADR (today the app uses no events). The listener sets `Selection` and navigates to the item's list route (`nav::list_path`).
- **View sync**: the view (doc 07) is in each window's localStorage, which is shared by same-origin windows; listen for the `storage` event in both windows so a toggle in one updates the other. No backend involved.
- **Theme**: the radar applies the same theme tokens (`theme.rs`, it reads the cached theme id from localStorage and `get_settings`).
- **Quit**: closing the main window closes the radar (Rust: on main `CloseRequested`/`Destroyed`, close `radar`). The app's existing flush-on-exit is unchanged.

## Refresh
- `get_data_revision()` → `i64` = `activity::latest_rowid` plus the sync `data_revision` (merges write activity, so the rowid already moves; keep it one number). Cheap.
- The radar polls `get_data_revision` every **5 s** and calls `get_radar(view)` only when it changed, or every **60 s** anyway (dates move at midnight; meetings advance). The main window's `MeetingClock` keeps meetings moving; the radar does not call `advance_meetings` (it can't write).

## Commands
`get_radar(view: View, utc_offset_minutes)` in a new `src-tauri/src/commands/radar.rs` (`radar_at(conn, view, today)` for tests): builds the inputs by **reusing** the `*_impl` helpers of overview, schedule, fog (01), pings (05), approaching (03), orientation (07); never duplicate their queries. Plus `open_radar`, `close_radar`, `set_radar_on_top`, `open_in_main`, `get_data_revision`. All in the 4 places (the radar's capability file lists its subset).

## UI
- **Route** `/radar` rendered **without** `Shell` (no sidebar, title bar, status bar, toasts or pane): `pages/radar.rs` mounts `RadarScreen` directly from `App` when the window label is `radar` (read `window.__TAURI__.window.getCurrentWindow().label` once at start; fall back to the path).
- **`RadarScreen`**: an SVG filling the window (square viewport, centred, letterboxed with `bg-canvas`):
  - rings: three circles, 1px `border-line`; ring labels at 12 o'clock in `faint` ("week", "2 wks", "qtr" — hints only, since the hover says the date).
  - sectors: thin radial dividers in `border-line`; each objective sector's outer rim has a 3px arc in its objective colour (`.obj-*` hue via `--obj-h`), the *Other* sector none.
  - centre: you, a small filled triangle in `accent`.
  - blips: health shapes as everywhere (`HealthMark`: filled red / half amber / empty green / dash), sized S 5px, M 7px, L 9px radius; meetings as hollow squares; pings as a pin glyph; `fog` → the blip at reduced opacity with a dashed outline (same idea as doc 01's look); `pinged` → a small pin badge.
  - edge marks: a short `warning`/`danger` arc segment plus an outward chevron on the rim; pulse **twice** on first appearance (CSS animation, 600 ms), then steady; no motion under `prefers-reduced-motion`.
  - a **sweep line** is **not** drawn by default (motion without information); behind a setting, off (see Open questions).
  - footer (24px, `bg-panel`): pure `footer_text(footer)`: "2 overdue · 1 ping today · visibility 64%"; when nothing: "Nothing in range this week".
  - top bar (20px, drag region): view toggle (N / ▲), pin-on-top toggle, close ✕. The whole background is also a drag region (`start_dragging` on mousedown on empty space).
- **Interaction**: hover a blip → a themed tooltip (`overlay`) with `label` ("Gateway auth · EU Region · blocked · due Fri"); click → `open_in_main`; the main window comes forward. Keyboard: `Tab` cycles blips in angle order, `Enter` opens, `Esc` closes the radar, `v` toggles the view.
- **Pure geometry** in `ui/src/radar_geom.rs` (tested): polar → pixels for a given size; arc paths for sectors; hit-testing for hover with a 6px tolerance; tooltip placement.
- **Main window**: a radar button in the title bar (left of the view toggle from doc 07) and the shortcut **`Ctrl/Cmd+Shift+M`** (toggle open/close; handled before the Ctrl/Cmd early return; check no conflict with undo/redo combos). Palette: "Open radar" / "Close radar". Settings → General → **Radar**: *Keep on top of other windows* (on), *Open with Minimap* (off), *Rings* (5 / 10 / 60 working days; validated increasing, 1..=260). These are device-local (`settings::LOCAL_ONLY` gains the keys, or store them in `local.*` meta; they describe this screen, not your data).

## Help wiki
- New page `docs/help/radar.md` (*The radar*): what the rings, sectors, shapes, sizes, edge marks, fog and pins mean (with a small legend table); opening, on-top, moving and resizing; clicking opens the item in Minimap; it never changes anything; Portfolio/My view; settings.
- Link from `welcome.md` (screens at a glance), `keyboard.md` (`Ctrl/Cmd+Shift+M`), `settings.md` (Radar settings, defaults quoted exactly), `views.md` (doc 07), `fog.md`, `pings.md`, `approaching.md`.

## Demo data
No schema change. Add a `radar` snapshot at 2027-03-03 (Portfolio and My view) in `src-tauri/src/commands/demo.rs`; review it: EU Region's sector should hold the red/amber blips and at least one edge mark if doc 03's demo check produced one.

## Export, undo, sync
Nothing stored except device-local window geometry and settings.

## Out of scope (later docs)
A system-tray icon with a mini radar (needs Tauri's `tray-icon` feature); notifications; editing from the radar; a sweep animation by default; showing people as blips.

## Acceptance criteria
- [ ] `core::radar` tests and proptests pass; stable angles; caps respected.
- [ ] `get_radar` command tests at a fixed date for both views; demo snapshot reviewed.
- [ ] The radar opens/closes from the title-bar button, the shortcut and the palette; remembers position, size and on-top; closes with the main window.
- [ ] The radar's capability allows only the listed commands (a test in `src-tauri` parses `capabilities/radar.json` and asserts the list); the main window alone has `core:event:allow-listen`.
- [ ] Clicking a blip focuses the main window with the item open; view toggles stay in sync between windows.
- [ ] Refresh follows changes within ~5 s; no calls while nothing changes except the 60 s refresh.
- [ ] Reduced motion respected; tokens only; shapes carry meaning without colour.
- [ ] ADR-0021, Help page and links, CHANGELOG, knowledge graph (§1 windows, §7 commands + capability, §8 route/screens/keys), `docs/progress.md`; gate passes; `cargo tauri build` still produces an installer.

## Not yet verified by hand (all three OSes if possible; Linux/WSL at least)
- Open the radar, move it to a corner, resize it, close and reopen: same place and size.
- On top: put a browser over it: the radar stays visible; toggle off: it goes behind.
- Change a task's due date in the main window: within ~5 s its blip moves.
- Click a blip: the main window comes forward with the task in the pane, even when minimised.
- Toggle My view in the radar: the main window's toggle follows.
- Quit Minimap: the radar closes too.
- macOS: `skip_taskbar` and always-on-top behave; the radar doesn't steal focus when it refreshes.

## Open questions
1. A slow sweep line (classic radar look)? **Off by default, behind a setting**, or not at all; it's motion without information.
2. Rings in working days or calendar days? **Working days**, like everything else in the schedule; the hover shows real dates.
3. Show people (overloaded reports) as blips? **No**; they aren't items with dates. Doc 06's frames cover them.
