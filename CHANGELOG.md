# Changelog

All notable changes to Minimap are documented here, organized by release. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

**Current work toward the next release.** Add entries as you complete specs and fix issues, then move them under a version number when you tag a release.

### Added
- **Look back to a past day on This week**: click a past day in the day strip and the screen shows the week as it stood then (what was overdue, due and blocked that day), with a bar and *Back to today*.
- **Changed filters stand out**: on Tasks, any filter or sort that is not at its default gets an accent ring, and a *Reset (N changed)* button puts them all back.
- **Sort the Kanban board by deadline**: a *Sort* dropdown in the Tasks filter bar orders the cards of every column at once by deadline (soonest or latest first) or priority; your choice is remembered.
- **Task types**: say what kind of work a task is (Design, Build, Decision, Review, Research, Bug, Admin to start with). Pick one in the task's panel, with `type:` in quick-add or when linking a new task; it shows as a coloured label on cards and rows and you can filter by it. Add, rename, recolour or archive types in Settings → General (Spec 32, ADR-0016).
- **Planned against actual**: a finished task says whether it was done on time or how many days late or early, and a task whose due date moved remembers the date it was first planned for, so a decision you meant to make by Friday shows when you really made it (Spec 32).
- **Kanban cards show marks**: a blue chain with a count when the task is linked to other tasks, and a green paperclip when it has files or web links.
- **Click a task on the Kanban board and its linked tasks light up across every column**, the rest fade back, and arrows join them (solid: comes first → has to wait, dashed: related).
- **Each person has their own colour** on the Kanban board's assignee circles.
- A **⋯ menu on every Kanban card**: open the task, make a new task linked to it, link an existing task to it, or archive it. *Link…* asks how the other task relates (comes first, comes after, related); a new task opens ready to fill in (Spec 31).
- A task's panel has one **Links** section, right under its fields, showing what it waits for, what it blocks, what is related and its other links, with buttons to link a new task, an existing task or something else (Spec 31).
- **Ongoing objectives** for outcomes that never end ("keep internal systems healthy"): no target date, never "done", a review rhythm instead. An overdue review turns the objective amber and is listed on This week and in a new Weekly review step; ongoing objectives have their own group in the Objectives list (Spec 30, ADR-0014).

### Changed
- **This week is redesigned**: a headline and five counts at the top, a red *Needs attention* panel with every overdue, due-today or blocked task once (with a pill saying why and what it waits for), then *Priorities this week* numbered most important first, and day tiles that turn red for days with late work. Tasks no longer repeat across sections.
- The marks on Kanban cards are bigger and say what they mean: a blue chip "2 linked tasks" and a green chip "1 file, 2 web links" instead of tiny icons.
- On the Kanban board, clicking empty space now clears the highlighted linked tasks and arrows (the task's panel stays open); click its card again to show them.
- *Link an existing task…* lets you tick several tasks at once (for example all the children of a parent); they are linked together or not at all, in one undo step (Spec 31).
- Linking tasks now says **Parent**, **Child** or **Related** instead of *Comes first / Comes after*, with a sentence naming which task waits for which. A task's Links section lists its *Parents* and *Children*. The links themselves are unchanged: a child is a task the parent waits for (Spec 31).
- Subtasks are removed (they were added in 0.2.0). Tasks are scheduled, counted and graphed one by one again; use *blocks* links, now easier to make, to say what comes before what. Existing subtask links are archived on upgrade (ADR-0015).

### Fixed
- In the *Link a task* dialog, the dropdown list of tasks opened away from its button.
- Pressing `n` to add an item, or `d` / `a` on a task row to jump to its due date or assignee, no longer types that letter into the box it opens.
- Installed builds (AppImage, .deb, .rpm) opened to an empty window: the content security policy blocked the app from loading its own WebAssembly file. `cargo tauri dev` didn't show it because the dev server isn't subject to the policy.

---

## [0.2.0] - 2026-10-07

**Subtasks, reference links and notes on items, and a more readable detail panel.**

### Added
- Subtasks can be put in order (*Do after…*), show a **next** step and *waiting* marks, and suggest a status for the group (all done, all blocked, started); the Dependencies graph draws a group as a dashed frame around its subtasks.
- Subtasks: add a step to a task, or link an existing task as its subtask, with progress ("2 of 5 done") on the parent and on the board (Spec 29, ADR-0013).
- Open tasks with a due date take on a red tint as the deadline nears, from 14 days out, on the Tasks board and in the list.
- Tasks, projects and objectives have a **Notes and findings** section in their panel: write a note about the item and see every note that mentions it.
- Tasks have a **Reference links** section: add web pages, Google Drive files or email addresses to a task and open them in your browser with a click (Spec 28).

### Changed
- The detail panel is wider (480px, 560px on big windows). Section headings have a coloured dot (red for Archive, grey for Links and Activity), "add something…" pickers are soft-blue buttons, and activity entries are coloured by what happened.
- Right-clicking no longer shows the browser's own menu (Back, Reload, Inspect); text boxes keep theirs for cut, copy and paste.
- A task with subtasks is now a group: it is scheduled from its subtasks (its own estimate is ignored), a blocks link on it applies to every subtask, and capacity and project health count the subtasks instead of counting the group again. A group can't block its own subtask (Spec 29).
- Task board cards show the due date as a pill that gets louder as the day nears ("In 5d", "Tomorrow", "Today", "3d overdue"), and priority as a solid P1 / tinted P2 pill.
- Task, project and objective panels are easier to read: a row of coloured status, priority and date pills at the top (overdue dates in red), colour-coded status and priority dropdowns, coloured status words in their lists, and colour-coded buttons (Archive in red, What if and Attach in blue, Add link solid).

### Fixed
- The project Schedule section no longer draws a cramped, overlapping timeline when there are no tasks; "today" and "target" are small markers on the axis instead of words that landed on the dates.
- Projects list no longer lets the project title and objective chip overlap when the detail pane is open; Owner and Target hide while the list is narrow.

---

## [0.1.0] - 2026-10-07

**First stable release.** M0–M4 complete: scaffolding, core data model, graph engine, people & exec layer, review, export, security.

### Added

**M0: Scaffold**
- Tauri 2 app shell with Leptos CSR frontend (no Node/npm)
- SQLite database with SQLCipher encryption option
- Trunk for WASM build, Tailwind for styling
- App data folder on macOS/Windows/Linux

**M1: Core data & UI**
- Node types: Objectives, Projects, Tasks, People, Teams, Notes, Decisions, Waiting-ons
- 11 edge types with cycle detection (blocks, depends_on, assigns, contributes_to, etc.)
- Activity log: every write is tracked for undo/redo
- Dark theme by default; 17 built-in themes (Settings → Appearance)
- Sidebar navigation and detail pane

**M2: Graph engine**
- CPM schedule with critical path (working days Mon–Fri)
- Dependency graph view (SVG, layered L→R, pan & zoom)
- Cycle detection with readable error messages
- Impact analysis: slip propagation, absorbed vs passed-on slack

**M3: People & exec layer**
- Capacity heatmap (people × weeks)
- Health scoring: project (lateness, blocked, unestimated), objective (weighted)
- Overview: top 5 risks, overloaded people, stale waiting-ons
- This week: Monday-Sunday view with inline actions
- Waiting-on with snooze (follow-up date)
- Notes with @mentions and checklists
- Decisions with supersedes links
- Command palette (Ctrl/Cmd+K) with quick-add grammar
  - `task Fix login @priya #api-launch !2 due:fri est:3d`
  - `project Q1 EU owner:@me target:2027-03-31`
  - `wait @raj on "Security review" by:next-wed`

**M4: Review, export, security**
- Weekly review: 7-step workflow with inline reschedule/resolve
- Markdown export: status reports with user-editable template (Settings)
- Full data export: JSON per node type + edges + history + attachments + manifest
- SQLCipher encryption: random key in OS keychain or user passphrase
- Backup & restore: manual, daily (keep 14), pre-migration, pre-restore
- Google Drive sync (ADR-0011): encrypted autosave, multi-device merge, attachments
- Undo/redo: Ctrl/Cmd+Z (session-only, 20 steps, built from activity log)
- Recurring items: repeat rule on tasks and notes
- Help wiki: 28-page searchable guide (Help button, `F1`)

**Demo data**
- Developer-only seed: 2 objectives, 3 projects (one at risk), 40 tasks, 8 people, 2 teams, 3 notes, 5 decisions, 3 waiting-ons
- Demo data can be removed safely (Settings → Data & backup)

### Changed

- (none; first release)

### Fixed

- (none; first release)

---

## Release notes

**How to use this:**

1. **During development:** Add entries to `[Unreleased]` as you complete specs and fix issues.
2. **Before tagging a release:**
   - Move `[Unreleased]` entries under a new version header with today's date
   - Create a new empty `[Unreleased]` section
   - Edit [VERSIONING.md](VERSIONING.md) version number and commit together
3. **Link from releases:** When you push a git tag, GitHub shows this CHANGELOG entry on the release page

## Style guide

- **Added:** New features, screens, commands, edge types, settings
- **Changed:** Renamed or restructured features (e.g. a screen moved to a different tab)
- **Fixed:** Bug fixes, performance improvements, visual tweaks
- **Removed:** Features that went away (rare pre-1.0)

**Per entry:**
- One line per change, active voice ("Add X", "Fix Y", not "X was added")
- Group by kind (feature, data model, UI, perf) when there are many
- Link to specs: `(Spec 24)`, ADRs: `(ADR-0012)`, or the code if notable
- Keep it user-facing: what they'll notice or do, not internal refactors

### Examples

✅ Good:
```markdown
- Add objective colours: each objective gets a unique hue, inherited by its projects and tasks (ADR-0012)
- Fix contrast of objective colours on all 17 themes
- Improve schedule performance on large graphs (petgraph optimization)
```

❌ Not needed:
```markdown
- Refactor ObjectiveColours to use Memo instead of Resource
- Extract TaskCard into separate component (ui refactor)
```

---

## Automation (optional)

If you use GitHub Actions or a release script:

```bash
#!/bin/bash
# Update changelog: move [Unreleased] to [X.Y.Z] with today's date
VERSION=$1
DATE=$(date -u +%Y-%m-%d)
sed -i "s/\[Unreleased\]/[$VERSION] - $DATE/" CHANGELOG.md
sed -i "/## \[$VERSION\]/i ## [Unreleased]\n\n### Added\n- (nothing yet)\n\n### Changed\n- (nothing yet)\n\n### Fixed\n- (nothing yet)\n\n---\n" CHANGELOG.md
git add CHANGELOG.md
git commit -m "Changelog for $VERSION"
```

For now, manual edits are fine — you're not shipping that frequently yet.

---

**Last updated:** 2026-10-07
