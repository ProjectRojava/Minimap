# 58 — Gantt, easier to read and use

Status: Implemented (all three phases) — awaiting manual check · Milestone: — · Priority: Should
Depends on: 57 (Gantt), 44 (safe zone), 50 (My view), 52 (zoom)
ADR: none expected (see *ADR check* at the end)

## Goal
The Gantt (spec 57) draws the right things, but on a real plan it is hard to read and hard to use without a steady mouse hand. In a typical first view:

- most of the screen is empty, because projects start folded and the tasks are hidden;
- names are cut short by the counts next to them ("Smal… 14 open · 3 blocked · 5 done · 5D ROOM");
- the shading for **room** is louder than the work, and an ongoing objective's line runs across the whole chart;
- flags, summary bars and edge handles are a few pixels wide;
- colour means two things at once: the objective's colour on objective rows, status on task rows;
- the axis shows a day number for every working day over months, with no month row and no way back to today;
- rows with nothing in them (an objective with no projects, a project with no tasks) give no hint about what to do;
- someone using a screen reader, only the keyboard, or a large zoom cannot use most of it.

This spec fixes that in three phases. **Each phase can ship on its own**, in order. Nothing here changes what the plan *is*: no new rule for the schedule, health, capacity or the drag (spec 57 and ADR-0022 stay as they are).

## Phase 1 — Quick wins: easier to read

**In**
- **Names on the bars.** On the *Days* scale, draw a task's name just right of its bar, in the muted text colour, clipped before the next bar on the row. A project's or objective's summary bar gets its name too when there is room. Hidden on *Weeks*, where there is no room.
- **The row lights up across both sides.** Pointing at a row (label or chart) highlights the whole row on both sides of the split, plus the column of the day under the pointer in the header. A small tag at the pointer says the day (*Thu 22 Oct*).
- **Two-row header.** Months on top (*Oct 2026*, *Nov*…), then days (Days) or week starts (Weeks). Today's column is marked in the header as well as by the line.
- **Open at today.** The chart opens scrolled so that today sits about a sixth of the way in from the left, not at the axis's first day. A **Today** button in the toolbar (and `t`) scrolls back there.
- **A *Fit* scale.** *Fit · Days · Weeks*: *Fit* picks the pixels per day so that the drawn work fits the chart's width (never wider than *Days*). Like *Weeks*, it offers no edge handles when bars are too narrow to grab.
- **Quieter room.** Draw the room between a project's work and its target as a **thin strip under the summary bar** (same zone colours, spec 44), not a full-height band. The overrun stays red and as tall as the bar. An ongoing objective's line becomes a faint line that fades out within a few weeks of its last work, not the whole chart.
- **Taller summary bars with progress.** Project and objective summary bars get the task bars' height (`BAR_H`). A project's bar is filled for the share of its tasks that are done (from `done_tasks` / `open_tasks`, already in `GanttProject`), so "5 of 19 done" shows on the bar.
- **Names win over counts.** The counts move into a narrow fixed column at the right edge of the label column, shortened to *14 · 3⛔ · 5✓* (each with a tooltip in words, and words for screen readers, see phase 2). The room chip stays (*5d room*, *2d over*) but goes in the same column. The full name is always in the row's tooltip.
- **Resizable label column.** Drag the divider between labels and chart (200 to 600 px); double-click it to fit the longest name. The width is remembered on this device.
- **One meaning for colour on bars.** Objective rows keep their colour as the **dot** next to the name only; their summary bar is drawn like a project's (neutral, with the zone strip). Colour on a bar then always means status or room, and the legend can say so in one line.
- **Rows that are empty say so.** An objective with no projects: *No projects yet* and an **Add a project** link (opens quick-add with `for:"<objective>"` filled in). A project with no open tasks: *No open tasks* and **Add a task** (quick-add with `#<slug>`). Both follow `docs/voice.md`.
- **A clearer toolbar.** Three segmented controls with visible labels (*Scale*, *Rows*, *Arrows*) and *Show finished*. The legend moves into a **?** button next to them (a popover; button only, because the `?` key opens Help everywhere), which frees a line of height.
- **Undo in the toast.** The toast after a drop says what moved and has an **Undo** button (the same as `Ctrl/Cmd+Z`).
- **Pan the chart.** `Shift+wheel` scrolls sideways; dragging on empty chart (not on a bar) pans it. The chart's own horizontal scrollbar sits right under the last row, not at the bottom of the window.

## Phase 2 — Accessibility

**In**
- **The rows are a tree for assistive tech.** The label column is a `role="treegrid"`: every row has `aria-level` (1 objective, 2 project, 3 task, 4 sub-task), `aria-expanded` where it folds, `aria-selected` on the open row, and the focused row is `aria-activedescendant` of the scroller. Each row's accessible name says what the chart shows: *Harden underwriting module, project, 2 open tasks, finishes Thu 22 Oct, 5 days of room* / *EU pricing, task, blocked, critical, Wed 3 Mar to Fri 5 Mar, due Fri 5 Mar, 2 days late*. The SVG is `aria-hidden` (the rows carry everything it says); the bars' tooltips stay for the mouse.
- **Changes are spoken.** A polite `aria-live` region says the outcome of every drop, keyboard move, refused move and fold: *EU pricing moved to Fri 5 Mar. 2 due dates pushed later.* / *Can't start before Provision EU network is done (Wed 31 Mar).*
- **Every drag has a keyboard and a click way (WCAG 2.5.7).**
  - On a **project** row: `Alt+←/→` moves its **target** a working day, `Alt+Shift+←/→` its **start date**. On an **objective** row: `Alt+←/→` moves its target (an ongoing objective says it has none). Same rules and undo as the drags.
  - **Click a bar** (a short press, as today) also opens a small **date popover** next to it: start, due (or target), estimate, with −1d / +1d buttons and the same limits and messages as the drag (*can't start before …*). `Enter` on a row opens it from the keyboard. It writes through the existing commands (`apply_task_drag` for tasks, `update_project` / `update_objective` for bands), one undo step each.
- **Targets big enough to hit (WCAG 2.5.8).** Every bar, flag, start mark and edge handle gets an invisible hit area at least 24 px tall and 24 px wide (handles grow outwards, never covering the next bar's handle). What is drawn stays the same size.
- **Comfortable rows.** A *Comfortable* row height (about 36 px rows, 16 px bars) next to today's compact one, for touchpads, tablets and low vision. Remembered on this device.
- **Not colour alone, everywhere.** Status already has fill, outline and stripes. Add the same for room: *wide* plain, *narrow* hatched, *none* dotted, *over* solid red with the *2d over* chip. The legend shows the patterns.
- **Contrast checked.** Every chart mark (zone strip, slack line, due tick, grid lines, the today line, muted labels) meets 3:1 against the chart background, and text 4.5:1, in every built-in theme (`ui/src/themes.rs`). Theme tokens only, no raw colours (`docs/design.md`).
- **Visible focus.** The focused row has a focus ring on both sides of the split and its bar is outlined, so the eye knows where the keyboard is.
- **More keys.** `Home` / `End` first and last row, `PageUp` / `PageDown` a screen of rows, `t` today, `Enter` the date popover. All checked against the global shortcuts (`/`, `v`, `?`, `g`-chords and `Alt+↑/↓` are global and stay so).
- **Reduced motion.** With `prefers-reduced-motion`, scrolling to today and to a found row jumps instead of gliding, and lit arrows don't fade.
- **A table view.** A *Chart / Table* switch shows the same tree as a plain HTML table: name, kind, status, start, finish, due, room or late, critical. Same folding, same keyboard, same detail pane. Columns sort when the tree is flat (*Projects only* off and a sort chosen). It reads well to a screen reader, prints, and is the quickest way to scan dates.

## Phase 3 — Bigger ideas

**In** (each is a separate decision; strike what we don't want before setting this spec to Ready)
- **Find on the chart.** `Ctrl/Cmd+F` on the Gantt opens a small find box: typing a name unfolds what is needed, scrolls to the first match and highlights every match; `Enter` / `Shift+Enter` move between them. (`/` stays the global search.)
- **Filters.** *Person* (tasks assigned to them, and the projects and objectives they sit under), *Objective*, and **Only what needs a look** (late, no room, or blocked). Needs the assignees on `GanttTask` (a types change, no schema change).
- **A smarter first view.** Projects that are **late, have no room, or have blocked tasks** start open; the rest stay folded. Clicks are still kept as the set that differs from this (as `is_open` does today). The rule reads `zone`, `late_by_days` and `blocked_tasks`, all already in `GanttProject`; to keep business rules out of the UI, `core::gantt::build` sets a `GanttProject.needs_a_look` flag and the UI only reads it.
- **An overview strip.** A thin strip above the header with the whole axis in miniature (summary bars only, today, targets) and a box for what is on screen; drag the box or click the strip to move there. A minimap in Minimap.
- **`Ctrl+wheel` zoom** around the pointer, between *Weeks* and *Days* in steps, the scale buttons following.
- **Create a task by dragging on an empty row.** On an open project, dragging across the empty space under its tasks draws a new bar; letting go opens quick-add with `#<slug>` and the dates (`start:` / `due:` / `est:`) filled in, to type the name. Nothing is created until it is committed. (Spec 57 left this out; it is listed here to decide.)
- **Group by person.** A *Rows: by person* option: one row per person, their open tasks as bars, weeks over 100% capacity shaded (from `get_capacity`, spec 17). Read-only at first.

**Out** (all phases)
- Any change to how the schedule, the drag or the push of due dates works (spec 57, ADR-0022).
- Baselines (planned against now), waiting-on, ping and fog marks on bars (still out, as in spec 57).
- Printing or exporting the chart as an image or PDF (PDF export is *Later* in `CLAUDE.md`).

## Acceptance criteria

### Phase 1
- [x] Task names drawn next to their bars on *Days*, clipped before the next bar; not on *Weeks* (pure layout test in `ui/src/gantt.rs`).
- [x] Hovering a row highlights it in both panes and the day column in the header.
- [x] Month row in the header; *Today* button and `t` scroll to today; the screen opens with today about a sixth in.
- [x] *Fit* scale fits the drawn work to the width, never wider than *Days* (pure test for the pixels per day).
- [x] Room drawn as a strip under the summary bar; the ongoing line fades within a few weeks of its last work (pure test on `marks`).
- [x] Summary bars at `BAR_H` with a done fill (pure test for the fill width).
- [x] Counts and room in their own column; names cut only by the label width; the full name in the tooltip.
- [x] Label column resizable 200 to 600 px, double-click fits, width remembered on the device.
- [x] Objective colour only on the dot; the legend says what colour means on a bar in one line.
- [x] Empty objective and empty project rows say so and offer quick-add, filled in.
- [x] Toolbar in three labelled groups; legend in a popover; the toast after a drop has *Undo*.
- [x] `Shift+wheel` and dragging empty chart pan it; the horizontal scrollbar sits under the last row.

### Phase 2
- [x] `treegrid` with `aria-level`, `aria-expanded`, `aria-selected`, `aria-activedescendant`; each row's accessible name built by a pure function with tests for objective, project, task, blocked, critical and late.
- [x] A live region says the outcome of drops, keyboard moves, refusals and folds.
- [x] `Alt+←/→` / `Alt+Shift+←/→` on project rows move target / start; on objective rows the target; one undo step each; refused moves say why.
- [x] The date popover sets start, due / target and estimate with the drag's limits; `Enter` opens it.
- [x] Hit areas at least 24 × 24 px for bars, flags, marks and handles, never overlapping a neighbour's handle (pure geometry test).
- [x] *Comfortable* rows, remembered on the device.
- [x] Room levels have patterns as well as colours; the legend shows them.
- [x] Contrast of chart marks ≥ 3:1 and text ≥ 4.5:1 in every built-in theme (a test over `themes.rs` tokens).
- [x] Focus ring on the focused row and bar; `Home`, `End`, `PageUp`, `PageDown`, `t`, `Enter`; no clash with global shortcuts (the keyboard-page test).
- [x] Reduced motion respected.
- [x] *Chart / Table* switch; the table has the same rows, folding and keyboard, and sorts when flat.

### Phase 3 (for the ideas kept)
- [x] Find box: unfolds, scrolls, highlights, steps through matches.
- [x] Person, objective and *needs a look* filters; assignees on `GanttTask`; My view still applies on top.
- [x] `needs_a_look` set in `core::gantt::build` (core test) and used as the first view's open set.
- [x] Overview strip with a draggable viewport box.
- [x] `Ctrl+wheel` zoom around the pointer.
- [x] Drag on an empty row opens quick-add with the project and dates filled in; nothing is created without commit.
- [x] *By person* rows with over-capacity weeks shaded.

### Every phase
- [x] Help page `gantt`, the keyboard page and any page the change touches say what is now true (the page-truth tests pass). *(Done for every phase.)*
- [x] Changelog line under `[Unreleased]`. *(Done for every phase.)*
- [x] `docs/knowledge-graph.md` §8 (and §2 / §7 if types or commands change) updated. *(Done for every phase.)*
- [x] ADR check done (below). *(An amendment line on ADR-0022 for each phase.)*

## As built (phase 1)
- Open questions 2 and 5 were answered as recommended: the label width is kept in `localStorage`; counts are short (*14 · 3⊘ · 5✓*) with words in the tooltip and the accessible name.
- Only **project** rows keep the fixed counts column (that is where the counts line up); objective and task rows give their few chips only the room they need, so their names get the rest. Below 280 px the column is dropped and the counts are in the tooltip. The label column starts at 340 px.
- *Fit* fits the whole axis (the work, today, targets and the 25 days of room after) into the chart's width.
- *Add a project* / *Add a task* sit in the counts column; the chart row says *No projects yet* / *No open tasks* next to today.
- The toast's *Undo* is the same as `Ctrl/Cmd+Z`: it undoes the last change, whatever it was.

## As built (phase 2)
- Open question 3 was answered as recommended: the **date box** is built. It opens beside the bar (under the row in the table) on a click or `Enter`. A task has *Move*, *Start* and *End* (steppers; date fields for start and end), a project *Start*, *Target* and *Both*, an objective *Target*. A typed date that falls on a weekend means the next working day. A step the drag would stop at its limit is refused with the drag's reason and writes nothing (`task_step_refusal`, `band_step`).
- **Spoken changes**: folds and *Every project opened* go to the Gantt's own polite live region; drops and refusals are toasts, and the toast host is already a polite live region, so they are said once, not twice.
- **Press areas**: the edges of a task bar are each 24 × 24, reaching out past the bar and at most a quarter of it in; the middle keeps at least half the bar, so on a bar shorter than two days (*Days* scale) the middle is narrower than 24 px. The date box and `Alt+arrows` are the way to move such a bar without aiming. Flags, start marks and the move grip of a project are at least 24 × 24; a start mark and a flag closer than that split the space between them.
- **Contrast**: the test reads `input.css` and checks the colour of every meaningful chart mark (bars, critical line, late tail, slack, due tick, summary bar and its fill, room strips and patterns, overrun, flag, today, focus) at 3:1 and every kind of chart text at 4.5:1 against the canvas of every theme, with no transparency. Grid lines, done bars, quiet arrows and an ongoing objective's fading line are faint on purpose and left out: they decorate or are said in words elsewhere (WCAG 1.4.11 asks 3:1 only of what is needed to understand the content). The slack line and the empty-row text moved from `--faint` to `--muted`; the room, overrun, late tail and due tick lost their transparency.
- **Comfortable** rows are 36 px with 16 px bars; the choice is kept in `localStorage` (open question 2).
- **Reduced motion**: scrolling to today glides unless the system asks for less motion; arrows fade in 150 ms, not at all with less motion.
- **The table** sorts by name, status, start, finish, due or lateness; a sorted table lists every task flat with its project's name, and a third click on the column goes back to the tree.

## As built (phase 3)
- Open questions 1 and 4 were answered by building every idea: the *needs a look* first view, and creating by dragging and rows by person in this spec.
- **Types and core**: `GanttTask.assignees`, `GanttProject.needs_a_look` and `GanttData.people` (all `serde(default)`); `core::gantt::needs_a_look` and `BuildInput.people`; `get_gantt` reads the people. No schema change and no new command. The demo snapshot now says which project needs a look (EU Region) and how many people are on the plan.
- **Filters** are done on the screen from what `get_gantt` sends (`filtered`): a person keeps their tasks and recounts the project's counts; a sub-task whose parent is not theirs stands alone; links stay only between what is left. My view still applies first, in the backend.
- **Find** matches every word of the text in objective, project and task names, in plan order, and unfolds the objective and project of each match (it never folds anything).
- **The overview strip** stacks the projects' work in four lanes, marks targets and today, and moves the chart on a click or a drag; it is for the pointer (`t`, the scale and the keys cover the keyboard).
- **Zoom** steps are 8, 11, 15, 18 and 22 px a day; the scale buttons show *Weeks* or *Days* at the ends and neither in between.
- **Creating by dragging** happens on an *add row* under an open project's tasks (dragging empty chart elsewhere still pans). The new task can't start before today; quick-add opens with `task #slug start: due: est:` and nothing is made until it is committed. Arrow keys pass over add rows; their **+ Add a task** button is in the tab order.
- **By person** puts a task under its first assignee by name (so a task appears once) and those with nobody under *Not assigned*; it reads the Capacity numbers (`get_capacity` over the chart's days, in the current view) and hatches the weeks over 100 %. It only reads: no drags, no date box, no arrows, `Alt+arrows` say so. The table keeps the plan's tree.

## Open questions
1. **First view.** Phase 3's *needs a look* rule, or keep "projects folded" and only fix the empty space in phase 1 (names, Fit)? Recommendation: ship the rule in phase 3; phase 1 alone already fills the screen better.
2. **Where device-local settings live** (label width, comfortable rows, last scale): `localStorage` like the radar's view, or the settings table. Recommendation: `localStorage`; they are about this screen on this device, not the data.
3. **The date popover vs. the detail pane.** The pane already edits dates. Is a popover worth it? Recommendation: yes, because it applies the drag's limits and pushes, which the pane's date fields do not show.
4. **Creating by dragging** and **by person** rows: in this spec, or a later one? Recommendation: decide when phase 2 ships; strike them here if not wanted.
5. **Counts as icons.** *3⛔ · 5✓* or words (*3 blocked*) at a smaller size? Recommendation: icons from `ui/src/icons.rs` with words in the tooltip and the accessible name.

## ADR check
None of this changes a rule ADR-0022 states (the drag, the push, the start rule), so no ADR is expected. If phase 3's *needs a look* rule ships, add an amendment line to ADR-0022 (the first view is no longer "projects folded"). If *create by dragging* ships, amend ADR-0022 too (spec 57 left it out on purpose).

## Not yet verified by hand
Phase 3 was checked in a browser against the demo plan (and its real capacity numbers) with a stand-in backend: EU Region opens on its own with its add row; the person filter (Tomás: 7 open tasks, recounted) and *Needs a look* (only EU Region); find (`Ctrl+F`, typing, *1 of 2*, `Enter` to a task inside a folded project, which opens, `Esc`); *By person* (eight people, Tomás *over 4 wk* with four hatched weeks, no arrows, no handles); `Ctrl+wheel` (22 → 18 → 15 → 18 px a day); a click on the strip scrolling the chart; drawing on the add row (a ghost bar with its dates, then quick-add filled in). Still to check in the app: that `Ctrl+wheel` doesn't also zoom the window on each OS, that the strip's box follows scrolling, and that a task drawn on the add row is created with those dates.

Phase 2 was checked in a browser against the demo plan with a stand-in backend: the treegrid attributes and every row's spoken name, ↓ / → / `End` / `Enter`, the fold announcement, the focus ring, the date box (focus inside, a refused −1d with its reason, a +1d that calls `apply_task_drag` and shows *Undo*, `Esc` back to the chart), `Alt+→` / `Alt+Shift+←` on a project calling `update_project` with the right dates, *Comfortable* rows remembered, the hatched and dotted room, and the table (tree, sort by due, `aria-sort`, keyboard, date box under the row). Still to check in the app, with a screen reader (Orca, NVDA or VoiceOver): that the tree is read as a tree, that the live region and toasts are spoken once, and that `Alt+arrows` don't clash with the window manager.

Phase 1 was checked in a browser against the demo plan with a stand-in backend (names on bars, the row and day highlights, months header, opening at today and `t`, *Fit*, the legend popover and `Esc`, resizing and double-click to fit, panning, *Add a project* filling quick-add). Still to check in the app:
- A drop's toast shows *Undo*, and it puts the bar back.
- *Add a task* on a project with no open tasks commits a task into that project.
- `Shift` + wheel scrolls sideways on Linux, Windows and macOS.
- *Fit* follows the detail pane opening and closing, and the window being resized.
