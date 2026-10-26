# Changelog

All notable changes to Minimap are documented here, organized by release. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

**Current work toward the next release.** Add entries as you complete specs and fix issues, then move them under a version number when you tag a release.

### Added

- A legend along the bottom of the Gantt explains the bars, marks and arrows; **?** hides and shows it.
- The Gantt shows each task's assignee as an avatar at the right end of its row.
- The Gantt's *Show* filters have a project box next to the objective box; it lists only the projects of the chosen objective.
- A small calculator button beside a task's start date, due date and estimate works that one out from the other two.
- The Gantt remembers its toolbar (view, scale, rows, arrows, *Show* filters) between visits, and a *Reset* button puts it all back to the defaults.
- *My team* on the Order screen: everyone in Minimap side by side, and dragging a task to someone's column gives it to them at that place in their order (Spec 61).

### Changed

- Changing a task's start date or due date updates its estimate to the working days between them.
- A task with a start date and an estimate gets its due date filled in, and one with a start date and a due date gets its estimate, in working days, when you create it or type one of them.
- The Order screen is a queue: one row per task with a bar on a two-week ruler, a summary of when the last step ends and what runs late, people as tabs, the tasks not in the order on the right, estimates edited in place and more keys (`t`, `1`-`9`, `x`, `e`). Moves show at once (Spec 61).

### Fixed

- On the Order screen, a task starting beyond the first three weeks had no bar: the ruler now grows to reach the last step and the queue scrolls sideways.

---

## [0.6.0] - 2026-10-23

**The order of work, backburner, and a Gantt you can read and use without a mouse; tasks need a type; richer Dependencies; a status bar of icons.**

### Added
- **Order of work** (Spec 60, ADR-0024): a new *Order* screen (`g q`, under People) where you put a person's tasks in the order they will be done. Tasks stacked top to bottom are done one after another, and tasks side by side at the same time. The Gantt, forecasts, health and capacity follow, so one person's ten tasks no longer all start today. Drag cards, or use ↑ ↓ ✕ and `Alt+↑/↓`. Your own start dates still win, and a *blocks* link always comes first. Each task below the first step gets its **Start date** written as the next working day after the task above, and a slip moves the dates below down; a later date you set yourself stays. A task in the order with an estimate and no due date gets one: its start plus its estimate.
- **Backburner** (Spec 59): set a task aside without losing it. Drag a card onto the new **Backburner** column on the Tasks board, or press `b` (board and list), or use the button in the task's pane. It keeps its status, dates and links, and leaves This week, capacity, the Overview's health counts, your people's frames, the weekly review and the fog until you bring it back. The schedule still counts it, and the message says how many tasks still wait on it. The list hides these tasks until you tick *Show backburner*. Finishing or cancelling a task takes it off.
- **The status bar is icons**: Google Drive has a shape for each state (cloud with a check, up arrow, down arrow, slash, exclamation mark, or a laptop when your data is on this computer only), the last save point is a disk in a ring that empties as it ages, and a padlock shows while the database is encrypted and a pin with a count while pings are due. Words appear only beside an icon that needs you, or always with *Words in the status bar* in Settings → Appearance. Click Drive or the disk for a panel with the detail, *Sync now* and *Make a save point…*.

### Changed
- **A task in progress always has a start date**: moving a task with no start date to *In progress* (board, list, pane, `s` or the waiting-on offer) asks for the day, with today filled in. The start date of a task in progress can't be cleared, and a task added straight into the In progress column starts today.
- **A task you add opens on the right**: pressing `Enter` in the add box on the Tasks board or list opens the new task in the detail pane.
- **Dependencies show every link between tasks**: besides the *blocks* arrows, *related* links (dotted) and *part of* links (a thin line from the parent to its sub-task) are drawn, listed under *Also linked* in an open box and lit when you trace a task. They change no date and make nothing wait. A task that is only related to another, or only part of it, is no longer listed as unlinked.
- **Dependencies remember their filters**: the project, team and objective filters and *Show finished* / *Draw unlinked* stay as you left them, like on Tasks, and a **Reset (N changed)** button puts them back.
- **A new task needs a type**: the add boxes on the Tasks board and list, the link dialog, a note's checklist and quick-add (`type:`) all ask for one, starting on the type you filter by or used last, and nothing is created without it. Tasks you already have are left as they are.
- **Linking an existing task lists the same project first**: in the *Link a task* dialog, the tasks of the task's own project come first under the project's name, then the other tasks, each with its project at the right.
- **The Tasks filters are remembered**: the project, assignee, type, status and due range stay as you left them, across screens and restarts, until you press *Reset*.
- **Project pickers on tasks show the objective too**: the project filter and the project choices on a task list the objective after the project's name, smaller and fainter, so a project and an objective with alike names are no longer confused.
- **People have an avatar of their own**: two letters in a shape and a colour, instead of one letter in a circle, so two people whose names start the same way (Pawan and Prayrit) are easy to tell apart. It shows on your people's frames, on Tasks board cards and in Dependencies, and now also before a person's name in the assignee, owner, manager and *Waiting on* dropdowns, in lists, in search and the command palette, in links and team members, and in the person's pane.

- **The Gantt is easier to read** (Spec 58, phase 1): names are written next to the bars, the row and the day under the pointer light up, months sit above the days, the chart opens at today (and **Today** / `t` brings it back), a new **Fit** scale shows the whole plan, project bars are filled for the work done with the room as a thin strip to the flag, counts move into their own column so names have room, the names column can be resized, empty rows offer *Add a project* / *Add a task*, the legend moves behind **?**, the message after a drop has **Undo**, and you can pan by dragging empty chart or with `Shift` and the wheel.

- **The Gantt works without a mouse and with a screen reader** (Spec 58, phase 2): click a bar or press `Enter` for a date box with −1d / +1d and date fields (same rules and undo as dragging); `Alt+←/→` also moves a project's or objective's target and `Alt+Shift+←/→` a project's start; `Home`, `End`, `PageUp`, `PageDown`; rows are a tree that says in words what each bar shows, with folds and refusals announced; bigger press areas; *Comfortable* rows; narrow and closing room is hatched and dotted, not only coloured; chart marks pass a contrast check in every theme; less motion when the system asks; and a **Table** view of the same rows that sorts every task by any date.

- **The Gantt opens where the trouble is, and narrows down** (Spec 58, phase 3): projects that are late, out of room or blocked start open; **Show** filters to one person, one objective, or what needs a look; **Find** (`Ctrl/Cmd+F`) lights up matches and unfolds them; an overview strip shows the whole plan and moves the chart; `Ctrl` + wheel zooms between Weeks and Days; drag across a project's add row to start a task with those dates in quick-add; and **By person** shows each person's tasks with the weeks they are over capacity hatched.

### Fixed

- Installed builds (`cargo tauri build`, AppImage, .deb) no longer lose colours, positions and layout that only the dev build showed: the content policy now allows inline styles again.
---

## [0.5.0] - 2026-10-16

**The Gantt screen, the radar, zoom, save points, and a map of your world: fog, safe zone, pings and My view.**

### Added
- **Gantt: the plan on a calendar**: a new *Gantt* screen (`g j`, under Plan) draws objectives, their projects and their tasks as bars, and every row opens and folds. A bar's look says where the task stands (to do, in progress, **blocked** in amber stripes, done faded); the critical chain is underlined, work past what the target allows is red, the due date is a tick, and a project shows the **room** between its work and its target flag. Arrows show which task waits for which: quiet for all of them, lit for the whole chain of the task you point at (or *All*, *Critical*, *Off*). **Drag a bar** to change its dates: the middle moves the task, its left or right edge changes its start or end (and its estimate); a task stops where what blocks it, its project or today stops it, and **the tasks that follow get their due dates pushed later** by exactly as much as they moved, in one undo step. Drag a project's or objective's **flag** to give its work more room (or less). `Alt+←/→` moves the open task from the keyboard. Spec 57, ADR-0022.
- **Keep Minimap running in the system tray**: closing the window can now hide Minimap to the tray instead of quitting (it asks the first time, and remembers if you ask it to). The tray icon opens the window on a click and has a menu: *Open Minimap*, *Quick add…*, *Show or hide the radar*, *Sync now* and *Quit Minimap*. Its tooltip and a red dot say what is waiting (*3 overdue · 2 pings waiting*), the radar stays on screen while the window is hidden, and starting Minimap a second time brings the first one forward instead of opening a second copy. *Settings → General → Window and tray* (this computer only) also has: minimising to the tray, starting Minimap with the computer (in the tray, with no window or start screen), a notification a few minutes before a meeting, one notification a day when pings are waiting, and locking a passphrase-protected database after the window has been hidden for a time you choose (Spec 56).
- **A start screen**: Minimap now shows its mark, *Starting up*, its version and who made it while it opens, instead of an empty dark window. It is **a small window in the middle of the screen** (not the whole screen), and the main window appears when the app is ready. It stays for five seconds, or until the app is ready if that takes longer (it says so if the start is slow, and it holds still if your system is set to reduce motion). The radar window only shows it briefly.
- **Why "Minimap"**: About and the Help welcome page now say where the name comes from: a small map you glance at, then get back to what you were doing.
- **A quick check that Minimap answers**: type `ping` in the command palette (`Ctrl/Cmd+K`) for one row that says *pong* with the version of Minimap and of its database. See the FAQ.
- **The radar: a small window you glance at**: press the *Radar* button in the title bar or `Ctrl/Cmd+Shift+M` for a small frameless window that can stay on top of your other apps. You are in the middle; rings are this week, the next two weeks and this quarter; each open objective is a wedge in its colour; blips are the work that matters, by when it is due (a filled, half or empty circle for overdue, blocked or on track; bigger for higher priority; faded and dashed when it has gone quiet; a pin for a ping), and marks on the rim show trouble approaching your work. Hover for what something is, click to open it in Minimap. It follows Portfolio view and My view, remembers where you left it, and changes nothing. Settings → General → Radar (Spec 53, ADR-0021).
- **Zoom: from the whole portfolio to one task**: `Alt+↑` zooms out one level (a task to its project, the project to its objective, the objective to the Overview) and `Alt+↓` brings you back to exactly where you were, the same screen, open item and highlighted row. A line under the title of the Overview, Objectives, a project-filtered Tasks screen and every detail pane shows the path (*World › Launch in the EU › EU Region*); click a step to go there, and ▾ lists the other objectives a project serves. Also *Zoom out* / *Zoom in* in the command palette. On Dependencies, zooming out past the smallest size of the Tasks level becomes the Projects level around the project you were looking at, and zooming in on a project past the largest size shows its tasks; `+` and `-` now zoom the graph too (Spec 52).
- **Archive from the People list**: point at a person's row and press *Archive* (asks first, lists the tasks that lose their assignee, `Ctrl/Cmd+Z` undoes it).
- **Save points: every restorable copy on one timeline**: press `Ctrl/Cmd+S` anywhere to make a save point, with a name if you like (*before the Q2 replan*); it is a backup that is never deleted for you. Settings → Data & backup → *Save points* lists your named and daily backups, the safety copies and Google Drive's checkpoints together, grouped by day, each with *Restore…*, and the status bar says *Last save point 2 h ago*, turning amber with the reason when there has been none for a day. Also in the command palette (Spec 51).
- **Portfolio view and My view**: a button in the title bar (or `v`) turns the maps around you. In My view the Overview, Objectives, Projects, Tasks, Dependencies and Capacity show only what touches you: what you own or are assigned, what blocks it (up to four links back) and what it blocks, the objectives it serves, and your reports. Each item says why it is there, the Overview's top five risks are chosen among your own, and *Show everything* goes back. Remembered per computer (Spec 50).
- **Your people: frames for you and the people who report to you**: a card on This week and a strip on People show you and each direct report by name, with this week's load (the Capacity heatmap's own number, with the part over capacity in amber), what you are waiting on them for, their blocked and overdue tasks, and *1:1 due · 18 days* when it has been longer than your rhythm. *New 1:1*, *Tasks* and *Capacity* are on each frame. Never a ranking and never a verdict on a person. Everyone below your reports is shown too, however many levels, each group under its manager. Settings → Thresholds sets the 1:1 rhythm (14 days) and can turn the deeper levels off (Spec 49).
- **Pings: come back to anything on a day**: press `m` on any row or card (or *Ping…* in the pane, or `ping #eu-region check data residency by:fri` in the palette) to leave a marker on it with a day and an optional note. A pin shows next to the item; on the day it appears on a new *Pings* card on This week, where `x` clears it and *Tomorrow* / *Next week* put it off. One ping per item, never part of the schedule, health or capacity; undoable, synced, exported (`pings.json`) (Spec 48, ADR-0020).
- **Dependencies: see why, trace a chain, edit the links**: boxes now say who has the work, its status, when it finishes against its due date, its slack, and an urgent pill; three lenses (*Route*, *Risk*, *People*) show one question's marks at a time; clicking a box traces everything it waits on and holds up, and its ⌄ icon opens the box in place into a card with the details, a status, *Add blocker…* / *Add task it blocks…*, *Open* and *What if it slips?* (one at a time; the graph makes room for it; double-click opens the detail pane); links can be drawn by dragging a box's handle, given a lag, and removed with Delete (Ctrl/Cmd+Z undoes each); work with no links waits in an *Unlinked* bar you can link from; arrows say their lag or, between projects, the room; and the Projects level shows health, owner, safe zone and progress (Spec 47).
- **What if shows the recalculated route**: the What-if screen now opens on a *Route* view. For each project a slip changes it draws the old route to the finish dashed and the new one in colour, says when the project now arrives (*arrives 2027-04-14 instead of 2027-04-07, +5 working days, 3 past target*), and lists the steps: where the slip enters, where it is absorbed, where it passes on and where it arrives. The tables are one click away under *Details* (Spec 46).
- **Approaching: warned when trouble elsewhere will reach you**: a strip on a project's pane, an objective's pane, This week and the filtered Tasks screen says when blocked, overdue or late work in another project will reach what you are looking at (*2 approaching from EU Region · first in 4 days*). Click it for the chain, why each task is in trouble, how soon it arrives and how much slack is left, with *Open* and *What if it slips?*. Nothing shows when all is well (Spec 45).
- **Safe zone: the room left before a project's target**: a thin bar with words (*6 days of room*, *No room left*, *3 days past target*, *No target*) on project cards, in the Projects list's new *Room* column, on the Overview, in the project's pane and at the top of its Schedule section. Counted in working days from the same forecast as health; amber at 3 days of room or fewer (Settings → Thresholds), *All tasks done* when nothing is left open, nothing for paused, done or cancelled projects (Spec 44).
- **Fog of war: see how old what you know is**: work that is in play but that nobody has changed, written about or confirmed for a while fades and shows a `≋ 19d` chip on lists, cards, the Overview, This week and the detail pane. *Still accurate* (or `c`) clears it without changing anything and `Ctrl/Cmd+Z` brings it back. The Overview gets a *Visibility* tile, This week a *Gone quiet* section for fogged tasks that are nowhere else on the page, the weekly review a *Fog* step, and Settings → Thresholds the six numbers (Spec 43, ADR-0019).
- **Waiting on, from the task**: a task that is stuck on someone can say so. *Add to this task → Waiting on* in its pane takes who, what and (optionally) when you expect it, and the task then shows the person on its board card, list row and This week row (amber when stale). Minimap offers to mark the task blocked, and to move it on when the last one is resolved, but never changes the status by itself. Marking the task done asks *Resolve them too?*; archiving it leaves the waiting-ons open on the Waiting on screen. The Markdown project files say who a task waits on. An expected-by date does not move the schedule (Spec 56).
- **Decision tasks have a record in the Decisions log**: making a task of type *Decision* creates a linked, proposed decision you write from the task's pane (*Decision record*). Finishing or archiving the task leaves the decision in the log (Spec 41).

### Changed
- **A project's start date now holds its tasks back**: no task starts before the project it belongs to. A start date in the past changes nothing, since nothing starts before today anyway; a start date in the future moves the project's forecast, health, capacity and What if with it. See Schedule in Help.
- **Dependencies: click a box to expand it, unlinked work on the right**: clicking anywhere on a box now traces it and opens it in place (the small chevron is gone), and double-clicking opens the task or project in the detail pane on the right; hints along the bottom, on every box and inside an open box say so. The unlinked work is no longer a bar at the bottom: a small *Unlinked (N)* button at the top right opens a list on the right that shows each item like the boxes in the graph and can be searched and narrowed by status, project and owner. Drag an item from the list onto the graph and it waits there as a dashed box; drag a second one to its right (it comes after) or left (it comes before) to link the two, following the graph's left-to-right order. Dropping an item on a box already in the graph links it to that box (or click an item, then the box). Links are saved like any other, show on the task's card on the Tasks board, and the item leaves the list.
- **A new app icon**: Minimap's mark is now a framed map with you as the solid dot, with a light version for light themes. The title bar, the browser tab icon and the app icons use a simplified version that stays readable at 16 and 32 pixels.
- **Clearer empty screens**: lists with no match now say how to see everything again (*Clear the filters to see all tasks*), the Overview's empty state says it is waiting for health (*No health to show yet*), and the sort menu says *due date* instead of *deadline* (new `docs/voice.md` for how Minimap's words are chosen).
- **Minimap opens maximized**: the window fills the screen every time it starts instead of opening at 1280×800. The maximize button in the title bar still restores the smaller size.
- **This week reads at a glance**: Meetings, Focus and Priorities no longer look alike. Meetings are an agenda with a heading for each day and the time in its own column; Focus is a pinned card with a blue edge and taller rows (title, then project and dates in words); Priorities are a ranked list with rank, priority, status and day in aligned columns. Each card has its own icon, and cards now sit on the page colour with a grey title bar. P1 is a solid amber pill and P2 a tinted one on every row, as on the board's cards.
- **A Decision task can't be done until the decision is written**: moving it to Done opens *Finish decision* (what was decided, why); saving finishes the task and marks the record decided with today's date, as one undo step. The status and date Minimap fills in are tagged AUTO, what you set is tagged YOU, and your own values are never overwritten (Spec 42).
- **Archiving always asks first**: every archive (tasks, notes, objectives, people, teams, projects, decisions, waiting-ons, the card menu) opens an "Are you sure?" dialog with Cancel selected (Spec 42).
- **Archive decisions from the list**: an *Archive* button appears on a decision's row when you point at it, and the pane's Archive section is now always open. Ctrl/Cmd+Z undoes it.

### Fixed

- Long task names in a project's Schedule chart no longer run over the bars: they are cut off at the end of the label column (the full name is in the tooltip).
- Archiving a task from the "⋯" menu on a Tasks board card now works: the Archive button in the confirmation did nothing.

---

## [0.4.0] - 2026-10-09

**Meetings, task focus, task-type borders, and a clearer detail pane.**

### Added
- **Show the Drive recovery key again**: Settings → Google Drive → *Add another computer* shows (and copies) the recovery key from a computer that is already connected, so a second computer can join the same Drive data.
- **Held-up work on Dependencies**: tasks further down the chain from a *blocked* task (A → B blocked → C → D) are now tinted red, so you can see everything a stuck task is holding up.
- **About**: a new entry in the sidebar footer (`g u`) showing who made Minimap (Uditt Lamba, with a clickable email, uditt.lamba@pm.me) and the version you are running (Spec 40).
- **Task card borders**: on the Tasks board a card's border is now in its objective's colour (the coloured strip on the left is gone) and in a thick line style that goes with its type, which you can tell without any colour (double for decisions, dashed for design, bugs and meetings, dotted for research and admin, heavy for reviews). Make every border a plain line under Settings → Appearance (Spec 39).
- **Meetings**: a built-in kind of task that happens at a time. A meeting always has a date, a start time and a length (an hour unless you say), starts itself (*in progress*) at its start and ends itself (*done*) when it ends, even catching up on ones that ended while Minimap was closed. *New meeting* on the Tasks screen or in the command palette; picking *Meeting* as a task's type asks when. Meetings have their own *Meetings* card on This week (with *NOW* and "in 25 min"), stay out of the schedule, capacity, health and the weekly review, and are never late. **Follow-ups**: *Schedule follow-up…* makes a linked meeting a week later; a meeting can have several, and the next meeting of a repeating series links itself to the last. (Spec 38, ADR-0018).
- **Focus: keep a task on This week every day, whatever its deadline**: star a task (on its board card or list row, with `f`, or with the new *Focus* box in its pane) to give it a *Focus* section on This week, even if it is due months away or has no date. Choose *until I take it out*, *today only* or *until a date*. A focused task that is late or blocked stays under *Needs attention*; more than five in focus, or one untouched for 14 days, gets a gentle nudge. Focus changes no date, schedule or health, and can be undone (Spec 37).
- **This week shows each task's latest three notes**: under every task in *Needs attention* and *Priorities this week*, the three newest notes that mention it, with their date and the start of their text; click one to open it, and a *+N earlier* line says when there are more. Looking back to a past day leaves out notes dated after it (Spec 36).
- **A GitHub-style text box everywhere you write**: descriptions (task, project, objective, team), the context, decision and rationale of a decision, notes on a person, notes and the note box on an item all share one box with *Write* and *Preview* tabs and a toolbar (heading, bold, italic, quote, code, link, bulleted, numbered and task lists, @ mention, attach a file). `Ctrl/Cmd+B` and `Ctrl/Cmd+I` work, `Enter` continues a list, and `Ctrl/Cmd+Z` steps back through toolbar changes (Spec 34).
- **Descriptions read as formatted text** with a pencil to edit, *Save* and *Cancel* (`Ctrl/Cmd+Enter` saves; `Esc` cancels when nothing was typed) (Spec 34).
- **Notes and findings are a thread**: each note on a task, project or objective shows as formatted text with its date, oldest first, with a pencil to edit it in place and a ⋯ menu (open in the pane, archive); the box for the next one is at the bottom (Spec 34).
- **Sub-tasks, separate from blocking**: a task can now be *part of* another task (one parent, one level deep) without that meaning anything waits. The task panel has a *Part of* section with the sub-tasks and how many are done, the Kanban card shows *2/5 sub-tasks* (or the parent's name on a sub-task), and the card menu has *Add a sub-task…*. Part of moves no date and never touches the schedule (Spec 33, ADR-0017).

### Changed
- **Board columns are tinted**: each Tasks board column now has its status colour across the whole column instead of a line along its top.
- **This week shows each task's notes as formatted text**: the notes under a task now read as they do in the note (links underlined and clickable, bold, lists, mentions), about four lines of each, instead of a plain-text excerpt with raw `[text](url)` characters (Spec 36).
- **The detail pane is easier to read**: every section sits under a header strip with its title, a count and its own add buttons at the right, so the sections are clearly set apart. Click a header to fold the section (Minimap remembers); the fields stay open, *Activity* and *Archive* start folded, and thin lines group the fields (Spec 35).
- **Line breaks are kept** in formatted text: a single newline shows as a line break, so what you type in the box reads the same when shown. Links in formatted text now open in your browser or mail program when clicked (web and email addresses only).
- **Notes open as formatted text**: a note with text now opens as it reads, with a pencil to edit it (and *Done* to go back); an empty new note still opens for typing. This covers the notes on tasks, projects and objectives too, since they are ordinary notes.
- **Blocking is worded as blocking again**: the link dialog and task panel say *Blocks this*, *Blocked by this*, *Blocked by* and *Blocks* instead of parent and child. Your existing links are the same links, just worded correctly; *Parent* and *Sub-task* now mean *part of* (Spec 33).

### Fixed
- **Mentions showed two @ signs** (`@@Name`) in a note's formatted view.

---

## [0.3.0] - 2026-10-08

**Task types, parent/child linking, a redesigned This week, and a more useful Kanban board.**

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
