# 06 — Your people: compact frames for you and the people who report to you

Status: Implemented as spec 49 — awaiting manual check (open questions answered as recommended) · Milestone: — · Priority: Should · Size: M
Depends on: specs 03, 08, 09, 16, 17 (implemented) · Read first: [README](README.md)
Expected ADR: none

## Goal
In team games, the **party frames** in the corner show each teammate at a glance so you notice when someone needs help. An exec's equivalent: for **you and each of your direct reports**, how loaded they are this week, what you're waiting on them for, what of theirs is blocked, and how long since your last 1:1. Today these live on four screens (Capacity, Waiting on, Tasks, Notes). Put them in one compact strip.

**This is about support, never about performance.** Frames show load and neglect (yours, of them), not output. No scores, no ranking, no "health" for people, no sorting by how busy someone is. That is the reason for every wording rule below.

## User stories
- On This week, a **Your people** card shows me and my five reports as compact frames. Raj's frame has a full load bar ("112% this week") and an amber "1:1 due: 18 days". I click *New 1:1* on it and a 1:1 note with Raj opens.
- Priya's frame says "Waiting on her: 2 (1 stale)" and "1 blocked task". Clicking it opens her in the pane.
- On the People screen the same frames sit above the list.
- If I have no reports (no one `reports_to` me), the card shows just me and a hint: "Set managers in People to see your team here."

## Rules (`minimap-core`, pure)
New module `crates/minimap-core/src/people_frames.rs`.

- **Who**: the self person, then everyone with an active `reports_to` edge **to** the self person (direct reports only), ordered **by name**, never by load. Optional (setting, default off): also people who report to my direct reports (one level more), shown indented under their manager.
- **Per person** (`Frame`):
  - `load_pct: Option<u32>` — this week's load from `capacity::compute` for the current Monday-week (the same number the Capacity heatmap shows for that cell); `None` when the person has no capacity window (e.g. 0 hours). `over: bool` = above 100%. `many_tasks: bool` from the open-task-count flag (`capacity_task_limit`).
  - `open_tasks: u32`, `blocked_tasks: u32` (assigned, status blocked), `overdue_tasks: u32`.
  - `waiting_on_them: u32` and `waiting_stale: u32` (open waiting-ons where `person_id` = them; stale per `waiting_on::is_stale`).
  - `last_one_on_one: Option<Date>` — the newest note of kind `one_on_one` that mentions them (by `note_date`). For the self person: `None` and not shown.
  - `one_on_one_due: Option<u32>` — days overdue when `today - last > one_on_one_every_days` (or never had one and they've existed for longer than that); `None` otherwise.
  - `next_meeting: Option<(Date, minute)>` — the next open meeting (spec 38) assigned to them **and** to me? Meetings are assigned to one person; use: the next meeting that mentions them in its title is too fuzzy, so **leave this out** (see Open questions).
- `flags(frame) -> Vec<Flag>` in a fixed order: `Over` (load > 100%), `ManyTasks`, `OneOnOneDue(days)`, `WaitingStale(n)`, `Blocked(n)`, `Overdue(n)`. These drive the UI pills; they are facts, not judgements.
- `build(input, today, settings) -> PeopleFrames { me: Frame, reports: Vec<Frame>, second_level: Vec<(Uuid /*manager*/, Vec<Frame>)>, hint: Option<Hint> }`.

**Tests**: who is included (archived people and edges ignored, reports-of-reports only with the setting); name order regardless of load; load equals the capacity module's number for the week; 1:1 cadence boundary; never-had-1:1 rule with creation date; waiting counts; flags order. **proptest**: the frame list is a permutation-independent function of the input (shuffle people/edges → same output).

## Data
- No migration.
- **Settings** (synced): `one_on_one_every_days: u32` default **14** (1..=180); `frames_second_level: bool` default false. Add to `Settings`/`UpdateSettings`, validate in `store::settings`.
- **Types** (`crates/minimap-types/src/people_frames.rs`): `Frame { person: NodeSummary, is_self, load_pct, over, many_tasks, open_tasks, blocked_tasks, overdue_tasks, waiting_on_them, waiting_stale, last_one_on_one, one_on_one_due, flags }`, `Flag`, `PeopleFrames`, `Hint`.

## Commands
`get_people_frames()` in a new `src-tauri/src/commands/people_frames.rs` (`frames_at(conn, today)`): settings, people, active edges (`reports_to`, `assigned_to`), `task_rows`, `waiting_on_items`, `note_items` (1:1s), capacity input as `get_capacity` builds it (refactor a shared helper in `commands/capacity.rs` rather than copy it). Register in the 4 places.

## UI
New component `ui/src/components/people_frames.rs`:
- **`PeopleFrames { place: Place::ThisWeek | Place::People }`**: a horizontal, wrapping row of `FrameCard`s (on narrow widths, a vertical list).
- **`FrameCard`** (about 200×64px, `bg-panel`, hairline border, `rounded-sm`):
  - line 1: avatar circle (the existing per-person hue used on board cards: `.avatar` + `--obj-h` from `objective_colour::assign` over people) with initials, name, role (muted, truncated); the self frame says "You".
  - line 2: a **load bar** (4px track, filled to `min(load,100)%`; above 100% the bar is full and a `warning` end segment shows the excess; no `danger` for people) with the text "72% this week" / "112% this week" / "no capacity set".
  - line 3: flag pills from `flags`, at most three then "+N": "1:1 due · 18 days" (warning), "Waiting on them · 2 (1 stale)" (warning only when stale > 0), "1 blocked" (warning), "2 overdue" (danger is allowed here because it's about tasks, not the person; use the same pill as task lists). Pure `flag_text(flag, person_is_self)`; for "you" phrase as "Waiting on you" never appears (waiting-ons are what *you* wait on).
  - hover/focus actions: *New 1:1* (creates a `one_on_one` note mentioning them, as the People page's *New 1:1* does today, and opens it), *Tasks* (Tasks screen filtered to them), *Capacity* (Capacity screen scrolled to them).
  - click elsewhere on the card: opens the person in the pane.
- **This week**: a *Your people* card in the right column (or after the 1:1s card in a single-column layout).
- **People screen**: frames above the list, under the filter bar.
- Keyboard: the frames join `ListNav` on the People screen (j/k reach them first); on This week they are reachable with Tab.
- **Words** (put in `docs/voice` section of doc 11 if it lands first): never "health", "score", "performance", "productivity", "under-utilised". Load is "load", over 100% is "over capacity".
- Pure helpers (`flag_text`, `load_text`, `bar_fill`) with tests.

## Help wiki
- `people-teams.md`: a *Your people* section: who is shown, each line, the 1:1 rhythm setting, that frames never rank people and don't judge performance.
- `this-week.md`: the card.
- `capacity.md`: one line: the frame shows this week's number from the heatmap.
- `settings.md`: the two settings with defaults quoted exactly (`14`).

## Demo data
No schema change; the demo already has "me", reporting lines (7) and 1:1 notes. Check at 2027-03-03: at least one report shows `OneOnOneDue` and the overloaded person shows `Over`. If no report has an overdue 1:1, backdate one existing 1:1 note in the seed (keep three notes total) and update snapshots. Add a `people_frames` snapshot. Update `docs/features/24-demo-data.md` if the seed changed.

## Export, undo, sync
Settings only (synced, not undoable). Nothing else stored.

## Out of scope
Next shared meeting per person (needs attendees; meetings have one assignee); teams' frames; frames for people outside your reporting line (use the People screen); any trend over time.

## Acceptance criteria
- [x] `core::people_frames` tests and proptest pass; load matches the capacity module exactly (test compares both on the same input).
- [x] `get_people_frames` command test at a fixed date; demo snapshot reviewed.
- [x] Frames on This week and People; actions work; ordered by name; no wording from the banned list (a ui test greps the module's strings for the banned words).
- [x] Settings validated and persisted.
- [x] Help, CHANGELOG, knowledge graph (§4, §5, §7, §8), `docs/progress.md`; gate passes.

## Implementation notes (spec 49)
- **`one_on_one_due` is days since the last 1:1** (or since the person was added, when there has never been one), not days past the rhythm: the pill reads "1:1 due · 18 days" as in the user story.
- **Which 1:1 is "with" someone** is refined from "mentions them": a 1:1 note often mentions people it only talks about (the demo's 1:1 with Priya mentions Tomás, Elena and Raj). A 1:1 that mentions one person is with them; one that mentions several is with those whose name is in its title; with no name in the title it is with everyone mentioned. A 1:1 dated in the future has not happened yet and does not count.
- **Waiting-ons** are counted open and not snoozed (the same ones the Waiting on screen lists), and are always 0 for you.
- **Pills**: besides the flags, a plain grey "Waiting on them · N" when you wait on them but nothing is stale, so a count is never hidden.
- **`build` returns `None` without a "me"** (before first run); the command answers `Option<PeopleFrames>`. `Flag` is `FrameFlag` (`week::Flag` exists), and `Frame` also carries `role_title`.
- **Where the card sits**: This week is a single column, so it is the card after *1:1s this week*. It is not shown when looking back at a past day. On People the frames sit under the header and the cursor (`j`/`k`) reaches them first; on This week they are reachable with Tab.
- **The actions**: *New 1:1* is the same function as the pane's button (`people_panel::new_one_on_one`); *Tasks* opens `/tasks?person=<id>` (the Tasks screen reads it into its assignee filter) and *Capacity* opens `/capacity?person=<id>` (that row, this week's cell chosen, scrolled into view).
- **Capacity** has a shared `capacity_at(conn, today, …)` that `get_capacity` and `get_people_frames` both call, so the figure is the heatmap's own.
- **Demo data**: no schema change, but Tomás Alvarez (the overloaded person) now reports to you directly, so his frame is on your card with *Over capacity*; the 1:1 with Priya now mentions Priya (it did not), which is one more `mentions` link (119 links). Raj, Sam and Tomás have never had a 1:1, so their frames say *1:1 due*; Priya's is clear.

## Not yet verified by hand
- Demo: This week shows you and your reports; the overloaded person's bar has the amber end.
- *New 1:1* on a frame creates and opens the note; the frame's 1:1 pill disappears after a reload.
- Remove a manager link in People: that person leaves the card.
- Settings: set the 1:1 rhythm to 7 days: more frames show "1:1 due".

## Open questions
1. Per-person 1:1 rhythm (stored on the person)? **Later**; a single setting first.
2. Show reports-of-reports by default? **No** (setting, off).
3. Next meeting with the person? **Not until meetings have attendees.**
