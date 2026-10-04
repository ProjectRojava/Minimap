# ADR-0006: A `supersedes` edge between decisions

- **Context**: spec 10 asked how one decision replaces another: a new edge type, or `relates_to` with a note.
- **Decision**: add an edge type `supersedes`, Decision -> Decision, pointing from the newer decision to the one it replaces. It has no attributes and must stay acyclic (checked like `blocks`, with the decision titles in the error).
- **Why not `relates_to` + note**: the meaning would live in free text, so the app couldn't tell "replaced" from "related", couldn't find the replacement for the list, and couldn't keep the status in step. A typed edge costs one matrix row; `edges.edge_type` has no CHECK constraint, so no migration is needed.
- **Behaviour**: adding the link (`add_edge`) goes through `store::decisions::supersede`, which adds the edge and sets the older decision's status to `superseded` in one transaction. Removing the link does not change the status back; the user sets it by hand. A decision replaced twice shows the most recent replacement.
- **Deviation**: `CLAUDE.md` §4's matrix gains the row `supersedes | Decision -> Decision | -`. Decisions stay their own node type (not a note kind): the table, the `affects` edge and the weekly review all treat them as a distinct thing.
