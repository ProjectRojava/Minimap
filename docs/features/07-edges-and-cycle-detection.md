# 07 — Edges, linking and cycle detection

Status: Implemented — awaiting manual check · Milestone: M1/M2 · Priority: Must
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
- [x] Every disallowed (type, from, to) combination is rejected in core, with a test. *(`matrix_matches_spec` checks all 10 × 8 × 8 combinations)*
- [x] Adding a cycle-forming edge fails and shows the path. *(command tests for `blocks`, `depends_on` and `reports_to`; the toast reads "Can't add this link: this would create a loop — Fix login → Deploy → QA sign-off → Fix login"; the toast itself is unverified by hand)*
- [x] Proptest: after any sequence of accepted inserts, the blocks/depends_on/reports_to graphs are acyclic. *(`cycles::accepted_inserts_stay_acyclic`)*

## Decisions
- **`relates_to` is in the MVP.** It links any node to any node with an optional note. It has no direction, so A–B and B–A are the same link: adding the reverse of an existing one is rejected as a duplicate, and the pane offers it once ("Related") from either end. It is not cycle-checked.
- The relations offered by "Add a link" come from the matrix itself (`core::edge_rules::link_options`), so the UI can never offer a pairing the backend would reject, and a test proves the two agree.
- Relations that a node's own panel already edits are not repeated in the generic editor (a person's teams and manager, a team's members, an objective's contributors, a project's objectives and dependencies, a task's assignee).

## Implementation notes
- **Core**: `edge_rules::attr_schema(edge_type)` is now the single definition of the attributes each link may carry (name, label, kind, error hint); `validate_attrs` is driven by it and so are the editor's inputs. `edge_rules::link_options(node_type)` lists the relations addable from a node, both directions, with the allowed node types on the other end.
- **Types**: `AttrKind`, `AttrSpec`, `LinkOption`.
- **Commands**: `list_link_options(node_type)` (new); `add_edge`, `remove_edge`, `update_edge_attrs`, `set_manager`, `list_edges_for` already existed. `add_edge` runs matrix, attribute, self-link, `relates_to` symmetry and cycle checks before writing.
- **UI**: the pane's Links section is now `LinksEditor`: links grouped by relation ("Blocks", "Blocked by", "Related", ...), each with inputs for its attributes (lag days, weight, allocation %, role, note — saved on change; empty unsets), a remove button, and "Add a link…": choose a relation, search by name across the allowed node types (already-linked nodes and the node itself are left out), click to add. Rejections (loops, duplicates) appear as toasts.
- Duplicate edges are rejected, and an archived edge with the same (type, from, to) is revived (existing store behaviour).
- `remove_edge` archives softly and writes `edge_removed` (existing).

## Not yet verified by hand
- on a task: Add a link → Blocks → search another task → added; it appears under "Blocks" with a Lag input; set a lag, clear it; remove it
- create a loop with three tasks: the toast names the tasks in order
- Add a link → Related on any node, to a node of another type; adding the same pair from the other node is refused; the note field saves
- on a decision/note/waiting-on (screens arrive with 08–10): the offered relations match the matrix (affects, mentions, about)
- a wrong pairing cannot be selected (the picker only offers allowed node types)
