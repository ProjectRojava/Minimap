# 05 — Pings: drop a marker on anything to come back to it

Status: Implemented as spec 48 — awaiting manual check (open questions answered as recommended) · Milestone: — · Priority: Should · Size: L
Depends on: specs 01, 12, 16, 22, 24, 25, 26 (implemented) · Read first: [README](README.md)
ADR: [ADR-0020](../decisions/ADR-0020-pings-as-a-light-table.md) *Pings: a light marker table, not a node type*

## Goal
In co-op games a **ping** marks a spot on the map in one click: "look here". An exec's version is "come back to this on Friday": the vendor contract inside a project, a person you need to talk to about something, a decision you want to revisit. Today the closest tools are focus (tasks only, every day, spec 37), waiting-ons (about someone owing you something) and creating a task (too heavy: it enters the schedule, capacity and health).

A ping is **a marker on any item with a day and an optional line of text**. On that day it shows up on This week (and in the radar, doc 10). Clearing it is one key. It never changes the item, its dates, the schedule, capacity, health or impact.

## User stories
- On the Projects board I highlight *Platform Cost Reduction* and press `m`. A small box: *When* (Today · Tomorrow · Friday · Next week · a date) and *Note* ("check the vendor contract"). Enter. A ping mark appears on the card.
- On Friday, This week has a **Pings** card: "*Platform Cost Reduction* · check the vendor contract · pinged Mon". I press `x` on it: cleared. `Ctrl/Cmd+Z` brings it back.
- In the palette: `ping #eu-region check data residency by:fri` previews and commits like any quick-add line.
- `ping @raj ask about the SOC 2 auditor by:tomorrow` pings a person.
- Pinging something that already has a ping changes that ping (one live ping per item).

## Concepts and rules
- **Targets**: objective, project, task, person, team, decision, note, waiting-on (any node type).
- **Live** = not archived, not cleared, and the target is active (not archived). A ping whose target is archived is hidden (not deleted); unarchiving the target brings it back.
- **Due** = live and `on_date <= today`. **Upcoming** = live and `on_date > today`.
- **One live ping per target**: creating a ping on a target that has a live one updates that one (note and date) instead; enforced in the store, inside the transaction.
- **Note**: optional, trimmed, ≤ 200 chars, plain text (no Markdown, never logged at info, like note bodies).
- **Date**: required; default today. A past date is refused ("Pick today or later") except when unchanged.
- **Clearing** sets `cleared_on = today`. Clearing again keeps the first date. *Ping again* on a cleared one makes a new ping.
- Pings are **private bookkeeping**: not in the weekly review's report, not in health, not in the schedule.

Pure rules go in a new `crates/minimap-core/src/pings.rs`: `validate(note, on_date, today, previous_on_date)`, `is_due(ping, today)`, `arrange(pings, today) -> PingsView { due: Vec<..> /* oldest on_date first, then created */, upcoming: Vec<..> /* soonest first */ }`, `when_text`-free (wording is UI). Quick-add grammar in `quick_add.rs` (below). Tests for each rule; proptest: `arrange` partitions live pings exactly into due ∪ upcoming with no duplicates.

## Data (ADR-0020)
**Why not a node type**: a ping has no page, no links of its own, no search entry and no detail pane; making it a node type touches search triggers, the edge matrix, `nodes::find`, the links editor and every node switch. **Why not an edge**: edges carry no dates and archiving the target would archive the ping's edge, losing it. So: a small table that points at its target, like `waiting_on.person_id` and the `attachments` table do, with its own repository that writes activity **against the ping** using a new `NodeType::Ping` variant **only in the activity log and undo**. Record this trade-off in the ADR; if the implementer finds that adding `NodeType::Ping` forces it into every node match, use the attachments precedent instead (activity row on the target with a `"ping"` diff key) and make the undo planner treat that key as undoable (document which in the ADR).

**Migration `0017_pings.sql`** (renumber if doc 01 lands first):
```sql
CREATE TABLE pings (
  id          TEXT PRIMARY KEY,           -- uuid v7
  target_type TEXT NOT NULL,              -- node type name, as in edges.from_type
  target_id   TEXT NOT NULL,
  note        TEXT NOT NULL DEFAULT '',
  on_date     TEXT NOT NULL,              -- YYYY-MM-DD
  cleared_on  TEXT NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT NULL
);
CREATE INDEX idx_pings_target ON pings(target_id) WHERE archived_at IS NULL AND cleared_on IS NULL;
CREATE INDEX idx_pings_on ON pings(on_date) WHERE archived_at IS NULL AND cleared_on IS NULL;
```
No FK (targets are polymorphic, like `edges`).

**Types** (`crates/minimap-types/src/pings.rs`): `Ping { id, target: NodeRef, note, on_date, cleared_on, created_at, updated_at, archived_at }`, `PingRow { ping: Ping, target: NodeSummary }`, `CreatePing { target: NodeRef, note: String, on_date: Date }`, `UpdatePing { note: Option<String>, on_date: Option<Date>, cleared_on: Patch<Date> }`, `PingsView`.

## Store (`crates/minimap-store/src/pings.rs`)
`create` (= upsert on the live ping of the target, one tx + activity), `update`, `clear(id, today)`, `reopen(id)` (clears `cleared_on`), `archive` (soft), `list_live(conn) -> Vec<PingRow>` (joins `nodes::summary`; skips archived targets), `live_for(conn, target_id) -> Option<Ping>`, `create_in_tx`/`update_in_tx` for quick-add and undo. Validation through `core::pings::validate`. **Every write logs activity.**

Wire-ups the README checklist requires:
- **Undo** (`minimap_core::undo`, `store::undo`): created → archive; updated → old values; `cleared_on` is a `Patch` → `nullable_fields`.
- **Sync** (`store::merge`, ADR-0011): add `pings` to the per-table merge (by id, newest `updated_at` wins; tombstones for hard deletes); add an `updated_at` trigger if other synced tables rely on triggers; a repair rule: if both devices created a live ping on the same target, keep the newer and clear the older (`cleared_on = its updated day`), reported in the merge summary like the other repairs.
- **Hard delete of a target** (`nodes::delete`): also hard-deletes its pings and writes tombstones.
- **Export** (spec 26): `DataExport.pings`, `store::export::collect`, `pings.json` in `commands/export.rs`, a line in `core::export_md::readme`; Markdown export: none.
- **Demo remove** (`store::demo_remove`): the `ORDER`/delete order gains `pings`.
- **Search**: none (state in the ADR).

## Commands (`src-tauri/src/commands/pings.rs`)
`list_pings()` → `PingsView` (today from `store::today()`, `list_pings_at` for tests); `ping_item(target: NodeRef, note: String, on: String)` (parses `on` with `quick_add::parse_when`, so "fri", "tomorrow", ISO all work; bad text → `invalid` naming the accepted forms; answers the `PingRow`); `update_ping(id, note?, on?)`; `clear_ping(id)`; `reopen_ping(id)`; `archive_ping(id)`; `get_ping_for(target_id)` → `Option<Ping>` (for the pane). Register each in the 4 places.

## Quick-add (`crates/minimap-core/src/quick_add.rs`, `docs/quick-add-grammar.md`)
- New keyword `ping`. Form: `ping <target> [note words…] [by:<when>]`.
- Target = the first `#project`, `@person`, or a named reference `task:"…"`, `objective:"…"`, `decision:"…"` (add these three keys; matching like `blocks:` names). Exactly one target, else the preview's problem "Say what to ping: #project, @person or task:\"…\"".
- `by:` optional, default today. Remaining words = the note.
- `@me` is refused ("You can't ping yourself; use a task").
- Preview card: "Ping *EU Region* on Fri 2027-03-05 · check data residency". Commit → `pings::create_in_tx`.
- Add the examples to `docs/quick-add-grammar.md` and to `docs/help/quick-add.md` (the core test runs every quick-add example in the help pages through `plan`, so they must parse).

## UI
- **Row key `m`** (add to `nav::is_row_key`): on any list/board/This week row, opens the **Ping box** for the cursor item. (`g m` is the Teams chord; row keys and chords don't collide, as `f` already shows.) Check the weekly review's own key handler doesn't use `m`.
- **`PingBox`** (new `ui/src/components/ping.rs`): a small anchored popover (`overlay`), five buttons (Today, Tomorrow, Friday (hidden on Fri/Sat/Sun, then "Next Mon"), Next week, Pick a date… → `DateField`), a one-line note input (autofocus), Enter = save, Esc = cancel. If the item has a live ping: shows it filled, plus *Clear ping*.
- **`PingMark { ping }`**: a small glyph (new `icons.rs` entry `ping`: a map pin) next to the title on `NodeRow`, `TaskCard`, project cards, Overview rows, people rows, with title text "Pinged for Fri 2027-03-05: check the vendor contract". `accent` when due, `muted` when upcoming. The glyph shape carries the meaning; colour is second.
- **This week**: a **Pings** card after *Focus* (before *Priorities this week*): due pings, oldest first; row = pin, target type chip, target title (click opens the target in the pane), the note, "pinged Mon" / "for 3 days" (pure `ping_age_text`), buttons *Clear* (`x` row key on that card clears), *Tomorrow*, *Next week*. Hidden when empty. Upcoming pings are listed under a collapsed "Coming up (N)".
- **Pane**: every node's pane gets a *Ping* pill in the pill row when it has a live ping (`get_ping_for`), with *Clear* and *Change…*; and a *Ping…* button in the pane header actions (all node types).
- **Palette**: actions "Ping this…" (the open item) and "Show pings" (scrolls This week to the card).
- `PingsCtx` provided by `Shell` (version-keyed `list_pings`) so rows can look marks up by target id without a per-row call.

## Help wiki
- New page `docs/help/pings.md` (*Pings: come back to anything on a day*): what a ping is and isn't (not a task, no effect on plans), `m`, the box, This week's card, clearing, one per item, quick-add forms with examples, what happens when the item is archived, undo, sync/export.
- Link from `this-week.md`, `quick-add.md`, `keyboard.md` (`m`), `concepts.md`, `export.md` (pings.json), `undo.md` if it lists what's undoable.

## Demo data
Seed three pings (relative to `today`): one due today on *Platform Cost Reduction* ("check the vendor contract"), one on a person due yesterday ("ask about the auditor") and one upcoming in 4 days on a decision. Update `DemoSummary` (a `pings` count + `describe()`), the Developer card `CONTENTS` text, store tests `tests/demo.rs` and `tests/demo_remove.rs`, the snapshots (add a `pings` snapshot of `list_pings_at` at 2027-03-03), `docs/features/24-demo-data.md`.

## Out of scope
Recurring pings; pinging a person "about" another item (one target only); pings in the Markdown report; reminders/notifications outside the app.

## Acceptance criteria
- [x] Migration, repo, commands and quick-add work; one live ping per target holds even with two quick creates (store test).
- [x] Undo/redo exact for create, update, clear (store `undo` tests); `nullable_fields` test passes.
- [x] Merge: pings sync; the double-live repair works (store `merge` test with two snapshots).
- [x] Export includes `pings.json`; round-trip test of the demo export includes pings.
- [x] This week Pings card, marks on rows/cards, `m` box, pane pill and button, palette actions.
- [x] Demo data, snapshots, `demo_remove` updated; story holds.
- [x] ADR-0020, Help page and links, grammar doc, CHANGELOG, knowledge graph (§3 table, §4, §5, §6, §7, §8, §10 invariants: one live ping per target), `docs/progress.md`; gate passes.

## Implementation notes (spec 48)
- **Activity and undo** use the fallback the spec allowed: an `updated` row on the *target* with a single `ping` diff key (attachments precedent), not a `NodeType::Ping` variant (33 node matches and the `NodeType::ALL` loops would have needed it). Undo reads the key as undoable (`Inverse::Ping`). See ADR-0020.
- **`nullable_fields`**: a ping is not a node, so `store::undo::ping_nullable_fields` (test-only) lists `UpdatePing`'s `Patch` fields (`cleared_on`) and a test keeps it equal to the type.
- **The Ping box** is a small centred dialog (like *Finish decision*), not a popover anchored to the row: the keyboard path (`m` on the cursor row) has no element to anchor to on every screen.
- **The pane pill** is a strip under the pane header (*Ping due* / *Ping*, the day and note, *Clear*, *Change…*) plus a *Ping…* button in the header, for every node type.
- **This week** loads `list_pings` together with the week (no pings when looking back at a past day) so the cursor rows are known; the due pings' targets are in `ListNav` after Focus and `x` on one clears the ping. The pins elsewhere come from `PingsCtx`.
- **No duplicates with Focus**: a due ping on a task that is already a row under *Needs attention* or *Focus* shows on that row (`RowPing`) and not in the Pings card (`split_pings`). Focus stays as it is: it is for tasks seen every day, a ping for anything seen on a day.
- **Quick-add**: the keys `task:`, `objective:` and `decision:` are read only on a `ping` line; the directory now lists decisions; the target is never created.
- **Commands** return plain sentences for refusals ("Pick today or later") and `ping_item` takes `on` as natural-day text.
- **Demo data**: three pings (Platform Cost Reduction due today, Raj Patel due yesterday, "Run EU data on managed Postgres" in four days); `DemoSummary.pings`; a `the_pings_of_the_demo_data` snapshot; `demo_remove` counts the pings that go with demo items.

## Not yet verified by hand
- Press `m` on a project card, pick Friday, type a note: the pin appears; This week shows it on Friday (or set the date to today to see it now).
- `x` on the Pings card clears it; `Ctrl/Cmd+Z` restores it.
- Quick-add `ping @<someone> ask about it by:tomorrow`: preview, commit, and the person's row shows the pin.
- Archive the target: the ping disappears; unarchive: it's back.
- Two computers on Drive: ping the same project on both while offline, sync: one live ping remains, the merge note says so.

## Open questions
1. Allow several live pings per item? **No**, one; it keeps the marks readable.
2. Should a due ping also appear under *Needs attention*? **No**, its own card.
3. Ping key: `m` (mark) or `p`? **`m`** (`p` is the weekly review's previous-step key).
