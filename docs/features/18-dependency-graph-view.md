# 18 — Dependency graph view

Status: Implemented — awaiting manual check · Milestone: M2 · Priority: Later
Depends on: 13

## Goal
An interactive picture of how work depends on other work.

## Scope
**In**
- Layered left-to-right layout (Sugiyama-style: layer assignment, crossing reduction, coordinate assignment) in `minimap-core`, or a suitable Rust layout crate.
- SVG rendered from Rust in Leptos; pan and zoom.
- Filters: project, team, objective. Critical path highlighted.
- Click node → detail pane.
- Command: `get_dependency_graph(scope)`.

## Acceptance criteria
- [x] Demo data renders without overlapping nodes; critical path highlighted. *(a property test over random DAGs checks that boxes never overlap, every link points right and routes join the boxes' edges; a 100-node plan with cross-project links lays out cleanly; the critical path is marked from the schedule. Demo data itself is spec 24.)*
- [x] Layout of 100 nodes in < 100 ms. *(1.5 ms release, 8 ms debug for 100 nodes / 304 edges; 400 nodes 6 ms / 34 ms; `cargo test -p minimap-core --release layout_speed -- --ignored --nocapture`)*

## Decisions
- **Yes, ship it for the MVP** (your answer). The critical-path list and impact analysis answer "what drives the date" and "what if", but only a picture shows the shape: where work fans in, which project is waiting on another, and what is tangled across projects.
- **Two levels** (toggle): **Tasks** joined by `blocks` (with lag shown as `+Nd` on the link), and **Projects** joined by `depends_on`. Links are drawn in time order, so what must happen first is on the left (a project that "depends on" another is drawn to its right).
- **Filters** (project, team, objective; they combine with AND): project = that project's tasks; team = work assigned to members of the team **or its sub-teams**; objective = tasks of projects that contribute to it plus tasks that contribute directly. At the project level, "project" means *focus on this project* and the objective filter applies; team is not used there. **What the selected work is linked to is shown dimmed (one hop) as context** so no link dangles; links between two pieces of context aren't drawn.
- **Unlinked work is hidden by default** (a portfolio of hundreds of isolated tasks would bury the graph); "Include unlinked" shows it and the toolbar says how many are hidden. **Finished work is hidden** unless "Show finished" is ticked; cancelled and archived work never appears. A graph over **400 nodes** is refused with a message to narrow the filters.
- **Critical path** comes from the schedule (13): critical tasks get an accent outline, and a link is highlighted when both ends are critical and nothing slack separates them (it really drives the finish). Late tasks (negative slack) are outlined in the danger colour, blocked ones dashed in the warning colour. (The project level marks late projects, not a critical path.)
- **Layout** is a Sugiyama-style layered layout in `minimap-core` (no external crate): longest-path layers with sources pulled next to their successors, virtual lanes so long links route between boxes, barycentre crossing reduction keeping the best arrangement, and least-squares vertical placement that keeps every gap (pool-adjacent-violators). A loop is reported as an error instead of hanging.
- **The drawing** is SVG from Rust data in Leptos: drag to pan, wheel or ± to zoom around the cursor, Fit (also automatic when the filter changes; edits keep your view), click a box to open it in the detail pane.

## Implementation notes
- Core `layout::layout` (generic: sizes + edges -> positions and routes, `crossings_of`), `dependency_graph::build(input, filter)`; types in `minimap-types::graph` (`GraphFilter`, `GraphLevel`, `GraphNode`, `GraphEdge`, `DependencyGraph`); command `get_dependency_graph(filter_by)`; UI `pages/graph.rs` (route `/graph`, sidebar Plan group, `g l`) with tested pan/zoom, path and text helpers and `.dep` styles in `input.css`.

## Not yet verified by hand
- Dependencies opens from the sidebar (or `g l`); with some `blocks` links, boxes appear in columns left to right, the critical chain outlined in the accent colour
- drag pans, the wheel zooms about the cursor, ± and Fit work; changing a filter re-fits; clicking a box opens the task (or project)
- project / team / objective filters narrow the picture and dim the linked context; "Include unlinked" and "Show finished" add nodes
- a long link between distant columns is routed around the boxes in between
- the Projects toggle shows `depends_on` with status and projected finish, and late projects outlined in red
- a task with a lag on its `blocks` link shows `+Nd` on the link
