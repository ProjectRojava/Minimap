# 12 — Command palette and quick-add

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Must
Depends on: 06, 07, 11

## Goal
`Ctrl/Cmd+K` to go anywhere, run any action and capture work in one line. This is the main way data gets in.

## Scope
**In**
- Palette: navigate to screens, search nodes (11), run actions ("New project", "Resolve waiting-on…", "What if this slips?").
- Quick-add grammar per `CLAUDE.md` §7, documented in `docs/quick-add-grammar.md`:
  ```
  task Fix login timeout @priya #api-launch !2 due:fri est:3d blocks:"Release 1.2"
  project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU"
  wait @raj on "Security review sign-off" by:next-wed
  note 1:1 @priya
  decision "Postgres over Mongo" affects:#api-launch
  ```
- Parser in `minimap-core`: pure, returns a structured preview with resolved/ambiguous/unresolved references.
- Date parser: `today`, `tomorrow`, weekday names (`fri` = next Friday, or today if Friday? decide), `next-wed`, `+3d`, `+2w`, ISO dates.
- Ambiguous `@name` / `#project` → pick from candidates inline. Unresolved → offer create, or send to inbox.
- Commands: `parse_quick_add(text)` → preview; `commit_quick_add(text, resolutions)`.
- Preview always shown before commit; `Enter` commits.

## Acceptance criteria
- [x] Every example above parses and commits correctly. *(core tests assert the whole parsed plan for each example; command tests commit each into a database and check the nodes and links. Plain assertions instead of `insta` snapshot files.)*
- [x] Ambiguous person asks to pick; `@me` resolves to self. *(tests; the picking UI is unverified by hand)*
- [ ] Palette opens in < 50 ms and is fully keyboard driven. *(keyboard driven by construction; the timing needs a look in the running app. It opens without any backend call, so it should be instant.)*

## Decisions
- **`fri` on a Friday is today.** A bare weekday is the next such day, counting today. **`next-fri` is that weekday in the next calendar week** (Monday to Sunday), so `next-wed` on a Monday is a week and two days away, not this Wednesday. Rationale: "by Friday" said on a Friday almost always means today, and "next Wednesday" almost always means the one after this week; the preview shows the weekday so a surprise is visible. Full table in `docs/quick-add-grammar.md`.
- **No keyword means a task.** The palette shows the preview first, so a stray line can't silently become the wrong thing. A task whose title starts with a keyword needs the `task` keyword.
- **The preview is always shown, and Enter commits** only when nothing is unclear. Unclear names are answered in order (arrows + Enter or click): pick a candidate, create it, or skip it.
- **Typos in names are offered, never taken**: `@priyaa` asks "did you mean Priya Shah?". Exact, whole-word and prefix matches are used directly when unique; several candidates ask.
- **One transaction**: the line, any people/projects/objectives/tasks created for it, and all its links commit together or not at all. To allow this the store's `create` functions gained `create_in_tx` variants.
- **`parse_quick_add(text, choices)`** takes the answers so far (an addition to the spec's `parse_quick_add(text)`), so the preview can show the effect of each choice; `commit_quick_add(text, choices)` re-parses and refuses anything not ready.
- **Palette**: `Ctrl/Cmd+K` anywhere (also from a text field). Empty box lists actions and screens; typing filters them and shows matching nodes (search, 11); the last row is always "Add task “…”" (the way into quick-add for text without a keyword). A keyword + space switches to quick-add. "Resolve waiting-on…" lists open waiting-ons and resolves the chosen one. **"What if this slips?" is left out until impact analysis exists (spec 14).**
- Extra grammar beyond the spec's examples (all documented): `about:`, `kind:`, `status:`, `date:`, `start:`, a `#project` on a wait/note/decision, `waiting` as a keyword.

## Implementation notes
- Core `quick_add::plan(text, context)` -> preview + plan (pure; `parse_when` date rules; name matching; the plan is only built when the preview is ready). Types in `minimap-types::quick_add` (`QuickPreview`, `QuickRef`/`RefState`, `QuickChoice`/`QuickPick`, `QuickPlan`/`QuickMain`, `DirectoryEntry`). Store `quick_add::directory` (active people, projects with handles, objectives, open tasks) and `quick_add::commit(plan)`. Commands `parse_quick_add`, `commit_quick_add`.
- UI: `components/palette.rs` (`PaletteHost`, mounted in the shell; pure helpers tested), `PaletteOpen` context, `Ctrl/Cmd+K` in `keyboard.rs`, a hint at the bottom of the sidebar.
- A property test found and fixed a panic on non-ASCII input in the `+Nd` date form.

## Not yet verified by hand
- `Ctrl+K` (and `⌘K`): the palette opens instantly with the cursor in the box; Esc closes; it also opens while typing in a field
- empty box: actions and screens listed; arrows + Enter go to a screen; typing `proj` filters; a node name lists the node (Enter opens it in the pane)
- `task Fix login timeout @priya #api-launch !2 due:fri est:3d` (with real names): the preview shows title, Due with weekday, Estimate, Priority, Assignee ✓, Project ✓; Enter adds it, a toast appears and the task opens
- `task Review @pri` with two matching people: the preview asks "which one?"; arrows + Enter pick; the preview then becomes ready; Enter adds
- `task x @zzz #nope`: asks about each in turn; "Create" makes them (the toast says "also created …"); "Skip" shows what happens instead
- `wait @raj on "Security review sign-off" by:next-wed`, `note 1:1 @priya` (opens the note, titled "1:1 with …", mentioning them), `decision "Postgres over Mongo" affects:#api-launch`, `project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU"`
- a mistake (`due:someday`, `est:lots`) is shown in red and Enter does nothing
- "New task" in the palette fills in `task ` and shows the preview prompt; plain text with no keyword ends in an "Add task" row
- "Resolve waiting-on…" lists open ones; Enter resolves the highlighted one; Esc goes back, Esc again closes
