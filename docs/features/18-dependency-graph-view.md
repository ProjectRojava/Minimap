# 18 — Dependency graph view

Status: Draft · Milestone: M2 · Priority: Later
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
- [ ] Demo data renders without overlapping nodes; critical path highlighted.
- [ ] Layout of 100 nodes in < 100 ms.

## Open questions
- Worth it for MVP given the critical-path list (13) and impact analysis (14) cover most of the value?
