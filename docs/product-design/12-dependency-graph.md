# 12 — Dependencies, richer: see why, trace a chain, edit the links

Status: Implemented as spec 47 — awaiting manual check · Milestone: — · Priority: Should · Size: L
Depends on: specs 07, 13, 14, 18, 33, 43, 44, 45 (all implemented) · Read first: [README](README.md)
ADR: none (a richer view of the same graph; the links are edited through the commands that already exist; no ADR is affected, checked against `docs/decisions/`)

## Goal
The Dependencies screen (spec 18) draws boxes and arrows, but a box says only a title, a date and a project, so you cannot tell who has it, what state it is in, how much room it has, or *why* it is red or dashed, and an arrow says nothing about what it means. It is also read-only: to change a link you go back to the task pane. Make it the place where you **untangle what blocks what** and **check whether a project will finish on time**, and where you **fix a link without leaving**:

- richer boxes (status, owner, priority, finish against due, slack, health, room), with detail that follows the zoom;
- **click a box to trace it**: what it waits on, what it holds up, in words;
- lenses (**Route**, **Risk**, **People**) so one question is answered with one set of marks, instead of every colour at once;
- links you can **draw, remove and give a lag**, with the usual loop check;
- the unlinked work in a **tray** instead of a count, so it can be linked from there;
- the same treatment for the **Projects** level (health, owner, safe zone, and the room between two projects on the arrow).

## User stories
- I open Dependencies and see, in each box, that *Select a Data provider* is in progress with Udit, P1, finishes Fri 16 and has no slack, and under the filter bar: "Biggest bottleneck: *Select a Data provider* holds up 5 · 4 links cross projects · 3 blocked".
- I click *Select a Data provider*. Everything it waits on and everything it holds up stays lit and the rest dims. A card says: "Holds up 5 tasks in 2 projects. Waits on nothing. Finishes Fri 2026-10-16, due Wed 14: 2 days late. On the critical path." with *Open* and *What if it slips?*.
- I switch to the **Risk** lens: only what is late, blocked or held up is marked, and each box says why ("late 2d · blocked"). On **People** the boxes carry their owner's colour and say how many tasks each holds up: I see one person at the head of the longest chains.
- I drag from the right edge of *Fetch Company Details* to *Automated Underwriting*: a link appears. I drag the other way round and get "Can't add: this would create a loop" with the path. I click the link, type `2` in *Lag* and it reads +2d; I press *Remove link* and Ctrl/Cmd+Z brings it back.
- "Unlinked (10)" at the bottom lists the work with no links. I click one, then click a box: the unlinked task now blocks it.
- On the Projects level each box shows a health mark, the owner, the safe zone (*10 days of room*) and the progress (3/15); each arrow says how many days of room there are between the two projects.

## Data (`minimap-types/src/graph.rs`; no migration, no setting)
All new fields are `#[serde(default)]`.

`GraphNode` gains: `task_status`, `project_status`, `priority`, `assignee: Option<NodeSummary>` (a task's first assignee; a project's owner), `project_id` (a task's project, a project's own id: for the objective colour), `finish`, `due` (a task's due date; a project's target), `slack_days`, `late_by_days`, `estimate_days`, `task_type`, `health: Option<HealthLevel>` (projects), `zone: Zone` (projects), `progress: Option<(done, total)>` (projects), and `upstream` / `downstream` (how many shown nodes it waits on / holds up, through any number of links).

`GraphEdge` gains `cross_project` (a task link between two projects, or a project and the inbox) and `room_days` (a project link: the working days between the end of the needed project and the start of the dependent one).

`DependencyGraph` gains `unlinked: Vec<GraphUnlinked { node, label, subtitle }>`: the nodes left out because they have no links (`hidden_unlinked` stays as their count). `GraphInput` gains `people`, `project_health` (from the Overview's health) and `zone_narrow_days`.

## Rules (`minimap-core/src/dependency_graph.rs`, pure)
- The facts above come from the schedule that is already computed for the critical path, and from the Overview's health for projects (the command passes it in; the core stays pure). The safe zone is `zone::of` on the project's forecast.
- `upstream` / `downstream` are transitive counts over the drawn links (no node counts itself).
- `room_days` = `floor(max(0, earliest start of the dependent project's open tasks − finish of the needed project))`.
- Node boxes are 54px high (they were 38) so three lines fit.
- **Tests**: each fact on a hand-built graph; transitive counts on a diamond; cross-project and inbox links; room between projects; unlinked list sorted and complete; project health/zone/owner/progress; filters and layout unchanged (existing tests keep passing, with the new height).

## Commands
No new command (as built the command has a `graph_at(conn, filter, today)` for tests, and `overview_at` now takes `&Connection`). `get_dependency_graph` returns the richer graph (the command adds the people, the health and the narrow setting to its input). Editing uses `add_edge` (rules and loop check), `remove_edge` and `update_edge_attrs`, which are logged and undoable like every other link change.

## UI (`ui/src/pages/graph.rs`, `ui/src/graph_view.rs` pure helpers)
- **Boxes**: line 1 title; line 2 status · owner · finish; line 3 the lens line. An owner badge (the person colour of the Tasks board) and a P1/P2 pill at the right, the objective colour as a 3px left bar, a fog chip (`≋ 19d`) when the task has one. **Detail follows the zoom**: below 50% only the title and the marks, below 30% only the shape.
- **Lenses** (buttons in the header, *Route* the default): Route marks the critical path and says slack and estimate (`slack 2d · est 3d`); Risk marks late, blocked and held-up work and says why; People tints by owner and says what each holds up. The legend under the graph follows the lens. Colour is never the only signal: every mark has words on the box.
- **Trace**: clicking a box selects it and lights what it waits on and holds up (the rest dims); clicking the background or Escape clears it. A double-click opens the detail pane (a single click no longer does).
- **The open box** (second round; replaces the floating inspector, which sat awkwardly beside the detail pane): a ⌄ icon on every box (hidden at the lowest zoom) grows it in place. The backend lays the graph out around it (`GraphFilter.expanded`; the box is `EXPANDED_W` × `EXPANDED_TASK_H` / `EXPANDED_PROJECT_H` in the layout's sizes, so nothing overlaps), and the page keeps the box where it was on screen by adjusting the pan once the new layout arrives. **Only one box is open at a time**; opening another closes the first; ⌃, Escape or a filter change closes it. A task's card: full title, pills, the sentence above and why it is marked, what it waits on / holds up by name (click to go there, ✕ to remove the link), the start of the description, a status (the four working statuses, through the same update as the pane so the decision gate applies), *Add blocker…* / *Add task it blocks…* (then click the other box), *Open*, *What if it slips?*, *Still accurate*. A project's card is simpler: health, zone and progress in words, needs / needed by, *Needs…* / *Needed by…*, *Open*, *What if it slips?*.
- **A picked link** has a small card at its middle (the sentence, the lag field for task links, *Remove link*).
- **Links**: a box shows a round handle on its right edge on hover; dragging from it to another box adds a `blocks` link (Projects level: the dragged-from project is needed by the one it is dropped on, `depends_on`). A refused link says why in a toast (including the loop path). A link is clicked to select it; `Delete` or *Remove link* removes it (a toast says Ctrl/Cmd+Z undoes it).
- **Unlinked tray** at the bottom (`Unlinked (N)`, collapsed): chips for the nodes with no links; clicking one starts "pick the box it should block" (a banner says so, Escape cancels).
- **Insights** in a row under the filter bar (the count stays at its right): the biggest bottleneck (clickable: selects it), cross-project links, blocked and late counts.
- **Projects level**: the same, with health mark, owner, safe zone text, progress, and the room on the arrow.
- Pure helpers (tested): `trace`, `detail_level`, `node_lines` per lens, `inspector_lines`, `edge_sentence`, `new_link` (which link a drag makes), `insights`.

## Help wiki
(As built the *Include unlinked* tick is renamed *Draw unlinked*, because the tray now lists them.) `dependencies.md` is rewritten: what a box says, the lenses, tracing, drawing and removing links, the lag, the tray, the Projects level, and the keys. Links to it from `approaching.md` and `what-if.md` (which already link here).

## Demo data
No schema change and no seed change. The snapshot `the_dependency_graph_of_the_demo_data_says_who_what_and_how_much_room` (at 2027-03-03) reads right: 18 boxes and 20 links with the 4 cross-project links, *Provision EU network and clusters* (Tomás) the biggest bottleneck holding up 9, EU Region amber and 2 working days past its target, Security and Compliance with 14 days of room, and no room between the two projects.

## Export, undo, sync
Nothing stored. Links drawn here are ordinary links: logged, undoable, exported and synced like any other.

## Out of scope
A timeline layout, swimlanes by project or person, collapsing a project into one box when zoomed out, an overview map, SVG export, search-to-jump, and the what-if heat lens. (All good later; see the brainstorm in the review of this screen.)

## Acceptance criteria
- [x] The core builds every new fact with tests; layout tests updated for the box height.
- [x] Boxes show the richer lines with the zoom levels; lenses change the marks and the legend; trace dims the rest; inspector for a box and a link.
- [x] A box opens in place into a card (one at a time) with the layout making room; double-click opens the pane; quick actions (status, add blocker / blocked task, remove a link, Open, What if) work.
- [x] A link can be drawn, refused with the loop message, selected, given a lag and removed (and undone).
- [x] The unlinked tray lists the hidden nodes and can start a link; Projects level shows health, owner, zone, progress and room.
- [x] Help, CHANGELOG, knowledge graph (§4, §5 `mod:dependency_graph`, §8), `docs/progress.md`, `docs/design.md`; gate passes.

## Not yet verified by hand
- Demo data: Dependencies shows owners, finish dates and the bottleneck chip; clicking a box dims the rest and says what it holds up.
- Draw a link between two boxes; draw one that would loop; remove it with Delete; undo.
- Change the lag on a link and see +Nd on it.
- Open a box with ⌄: the graph moves aside and the box stays where it was; open another and the first closes; change its status; "Add blocker…" then click a box; double-click opens the pane.
- Click an unlinked chip, then a box.
- Projects level: health marks and room on the arrows.

## Open questions
1. Should dragging from a box's *left* handle make that box *blocked by* the target (the reverse link)? **Not now**: one handle on the right keeps it learnable; the reverse is a drag the other way.
2. Should the inspector be a floating card or part of the detail pane? **Neither, in the end**: a floating card at the top right sat awkwardly beside the open pane, so the details moved into the box itself (it opens in place, one at a time), and the pane opens on double-click.
3. One open box or several? **One**: the layout makes room for a single box and the graph stays readable.

## As built, third round (a click opens a box; the unlinked list moves to the right)
This replaces what the UI section above says about the ⌄ icon, the double-click and the tray.
- **A click anywhere on a box traces it and opens it in place** (when zoomed in enough to read it; at the lowest zoom a click only traces). There is no chevron any more. The click waits 260 ms (`CLICK_DELAY`) to see whether a second one follows: a **double-click opens the item in the detail pane on the right** and does not expand the box. In an open box, a click on an empty part of the card (not on a button, a name or a field) closes it; **⌃** and `Esc` still do. Hints: the box's tooltip (`NODE_HINT`), the line along the bottom of the graph (`LEGEND_HINT`), a line at the foot of the open card (`CARD_HINT`).
- **Unlinked work is a list on the right.** Closed to start with, a small **Unlinked (N)** button at the top right of the graph opens it; **✕** closes it. Each item is drawn **like the boxes in the graph** (title, status · owner · finish, the lens line, a P pill, an owner badge, the objective stripe, dashed amber when blocked, red when late), built from `GraphUnlinked.look` (a `GraphNode` the core fills with the same facts as a drawn box: `to_node` / `unlisted`). The list can be **searched** (every word in the name, project line, owner or status) and **narrowed by status, project and owner** (the choices offered are only those in the list; `UnlinkedFilter`, tested in `graph_view.rs`). Dragging an item onto a box links them (the box lights up as a drop target; the item blocks the box, or on the Projects level the box needs it), as does clicking an item and then the box (`tray_pair` decides the direction for both); a drop on empty graph links nothing and says so; the link is an ordinary `add_edge`, so it is saved, undoable, on the Tasks board card, and the item is gone from the list on the next graph (test `an_unlinked_task_dropped_onto_a_box_is_linked_everywhere`); double-clicking an item opens it in the pane. `MAX_UNLINKED_LISTED` is now 500 so a search can find any of them.
- Tests: core (`an_unlinked_entry_carries_the_facts_of_the_box_it_would_be`, `many_unlinked_nodes_are_all_listed_so_a_search_can_find_them`), `graph_view` (filter, choices, header, hints).

## As built, fourth round (the first dropped task waits; the next one links to it)
Dropping onto "the box it should block" fails when that box is not in the graph (and on an empty graph there is nothing to drop on), so a drop on **empty graph** now works like this. This replaces the "Nothing was linked" notice.
- **The first unlinked item dropped waits on the graph** (`GraphFilter.waiting`, `GraphNode.waiting`): the core draws it although nothing links to it, as a dashed box with *Not linked yet · drop another task beside it*, centred **below** the graph (`WAITING_GAP`) so the rest does not move; it leaves the unlinked list and its count. The page pans to it when it lands off screen.
- **The next one dropped on empty space (or on the waiting box) is linked to it**, by where it lands against the waiting box's centre (`drop_pair`): to the **right**, the waiting item blocks it; to the **left**, it blocks the waiting item. This is the graph's own convention (what is on the left blocks what is on the right; on the Projects level, is needed by it), the same order `new_link` draws. A banner while dragging says so (`drop_beside_banner`). After the link is saved the waiting state is cleared, so only one item ever waits.
- **It is cleared** by **✕** on its box, `Esc`, a switch between Tasks and Projects, leaving the screen (the state is local to the page), or by it being linked another way (the graph shows an edge on it). Changing a filter or lens keeps it.
- Dropping on a drawn box other than the waiting one is still the direct shortcut (the item blocks that box); clicking an item and then a box still works.
- Tests: core (`a_dropped_unlinked_task_is_drawn_below_the_graph_and_not_listed`, `a_dropped_task_on_an_empty_graph_is_the_only_box`, `once_the_dropped_task_is_linked_it_is_an_ordinary_box`, `a_dropped_item_the_graph_would_not_draw_is_ignored`, `a_dropped_project_waits_below_the_graph_too`), `graph_view` (`dropping_beside_the_waiting_box_follows_the_left_to_right_convention`).
