# 07 — Edges, linking and cycle detection

Status: Draft · Milestone: M1/M2 · Priority: Must
Depends on: 03–06

## Goal
Link any two nodes with a typed edge, validated against the edge matrix, with cycles rejected and explained.

## Scope
**In**
- Edge matrix from `CLAUDE.md` §4.2 enforced in `minimap-core` (type, allowed from/to node types, attrs schema and ranges).
- Commands: `add_edge`, `update_edge_attrs`, `remove_edge`, `list_edges_for(node_id)`.
- Detail pane "Links" section: edges grouped by type and direction ("Blocks", "Blocked by", "Assigned to"…); add via a type picker + node search; edit attrs; remove.
- Cycle detection (petgraph) before inserting `blocks`, `depends_on`, `reports_to`, and team nesting. Error contains the cycle path as node titles.
- No self-edges; duplicate edges rejected (or the archived one revived).

## Rules
- Attr validation: `lag_days` ≥ 0 integer, `weight` 0–1, `allocation_pct` 1–100, `role` ∈ {lead, member}.
- `remove_edge` archives (soft) and writes `edge_removed` activity.

## UI
- Cycle error reads like: "Can't add: this would create a loop — Deploy → QA sign-off → Fix login → Deploy."

## Acceptance criteria
- [ ] Every disallowed (type, from, to) combination is rejected in core, with a test.
- [ ] Adding a cycle-forming edge fails and shows the path.
- [ ] Proptest: after any sequence of accepted inserts, the blocks/depends_on/reports_to graphs are acyclic.

## Open questions
- Should `relates_to` be in the MVP?
