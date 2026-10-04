# 11 — Search

Status: Implemented — awaiting manual check · Milestone: M1 · Priority: Must
Depends on: 03–06

## Goal
Find any node by text in under 100 ms.

## Scope
**In**
- SQLite FTS5 index over titles, descriptions, names and note bodies (kept in sync via triggers or repository writes).
- Command: `search(query, types?, limit)` → ranked `NodeRef` + title + snippet.
- Search box in the sidebar (`/` to focus); results open the detail pane.
- Used by the edge picker (07), `@` picker (09) and command palette (12).

## Rules
- Prefix matching (`fix log` finds "Fix login timeout").
- Archived nodes excluded unless toggled.

## Acceptance criteria
- [x] With demo data, typing returns results in < 100 ms. *(measured on a synthetic 2.9k-node / 20k-term database, see Implementation notes; demo data itself is spec 24)*
- [x] Note body text is searchable. *(tests; the box itself is unverified by hand)*

## Decisions
- **Typo-tolerant matching is in** (your answer). Prefix matching is the base; when a query finds nothing as typed, each word that matches no indexed term is retried with indexed terms a typo away (see ADR-0007 for the rules). So `loign` finds "login", `fix loign timeot` finds "Fix login timeout", while `fox` (too short) or `zzzzzz` find nothing rather than something wrong.
- **FTS5 with triggers**, one `title` column (names, titles, project handle) and one `body` column (descriptions, notes, role/email, decision text). Title matches rank 8x body matches; ties by creation order.
- **Mentions in notes are indexed as `@Name`**, so searching a person's name also finds the notes that mention them, and ids never match.
- **Accents and case are ignored** (`cafe` finds "Café"). Typo tolerance itself compares lowercase text, so a typo'd word typed with accents isn't corrected.
- **Archived nodes are hidden** unless the box's "Archived: hidden" toggle is flipped (or `include_archived`).
- **Not switched yet**: the edge picker (07) and `@` picker (09) still filter their own lists in the UI; they work and are not on a keystroke-critical path. The `types` filter on `search` is there for them and for the palette (12), which will use it.

## Implementation notes
- Migration `0007_search.sql` (index, triggers, backfill of existing data). `store::search` (`vocabulary`, `run`), core `search` (`tokens`, `strict_expression`, `fuzzy_expression`, `similar_terms`, `distance`, `typo_budget`), command `search(query, filter_by{types, include_archived, limit})` (default 20, max 100), hits = node, label, archived, snippet.
- UI: the sidebar search box (`/` focuses it; ArrowUp/Down, Enter or click opens the node in the detail pane; Esc clears). Each result shows type, name, "archived" if so, and a snippet of its text.
- Timing (`cargo test -p minimap --release search_speed -- --ignored --nocapture`, 2,500 tasks + 400 notes x 400 words, 20.7k terms): prefix 0.5 ms, two words 0.3 ms, typo retry ~20 ms; the unoptimised debug build used by `cargo tauri dev` takes ~75 ms on the typo retry at that size, far less on a normal database.

## Not yet verified by hand
- press `/` anywhere (not in a field): the sidebar box is focused; type `fix`: results appear under it as you type
- type a typo (`loign` for a task called "login ..."): the task is listed
- results show the type, name and a snippet; ArrowDown + Enter opens the node in the pane; clicking a result does too
- a note's body text is found; a person's name finds the person and notes mentioning them
- archive a task: it disappears from results; "Archived: hidden" -> "shown" lists it greyed with "archived"
- Esc empties and closes the box; clicking elsewhere closes the list
