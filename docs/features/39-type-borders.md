# 39 — Card borders: the type's line in the objective's colour

Status: Implemented — awaiting manual check · Milestone: — · Priority: Could
Depends on: 06, 32

## Goal
Tell a task's type on the board at a glance, and not by colour alone: each card is framed in its type's colour with a line style that goes with the type.

## Scope
**In** (built)
- **Line** (board cards only): `TypeBorder::of(type_id)` in `minimap-types` (fixed, not editable): *Decision* double (4px), *Design*, *Bug*, *Meeting* dashed (3px), *Research*, *Admin* dotted (3px), *Review* solid 3px, *Build*, untyped cards and any type of the user's own solid 2px; `css()` / `width_px()` / `name()`. Thick on purpose: they must read at a glance.
- **Colour**: the border is the colour of the card's **objective** (inline `--frame` from `task_type::frame_colour`, through the same theme-aware `--obj-s` / `--obj-l` the chips use); grey (`--line-strong`) when the project serves none. It replaces the old 3px objective left edge (`obj-bar`), which cards no longer have. CSS `article.card-frame` (`ui/style/input.css`); the line comes from inline `--tb-style` / `--tb-width` (`task_type::frame_style`).
- **States**: every card shows open / linked / cursor as a background tint and a ring (`ring-accent`, `ring-line-strong`), never as a border colour.
- **Setting** `type_borders` (default on; Settings → General → Appearance, *Frame task cards by type*; off = a plain 2px line on every card, the objective's colour still the border): per device like the theme (`settings::LOCAL_ONLY`), `Settings.type_borders` / `UpdateSettings.type_borders`, read by the `TaskTypes` context.
- Docs: Help (tasks, settings), `docs/design.md`.

**Out**
- Borders on list rows, This week, the dependency graph or the Gantt bars; a border style per type edited in Settings; a card tint; an icon per type.

## Acceptance criteria
- [x] Every default type has a frame, other types a plain one, all four CSS styles are used and a double line is wide enough to show (`types::task_type` test).
- [x] The border's inline style follows the type and the setting, and its colour is the objective's, none without one (`ui::components::task_type` test).
- [x] The setting is on by default, persists, is left alone by other updates and is never synced (`store tests/task_types.rs`).
- [ ] Click-through (see below).

## Not yet verified by hand
- Tasks board: a Decision card has a thick double border, Design / Bug / Meeting thick dashed, Research / Admin thick dotted, Review heavier solid, Build, untyped and own types a plain 2px line
- Every card's border is its objective's colour (the same as its chip); a card whose project serves no objective has a grey border; there is no coloured strip on the left any more
- Opening a card, selecting one with `j`/`k` and linking cards show a tint and a ring, not a new border
- Settings → Appearance: switch it off and every border is a plain line (still in the objective's colour); it stays off after a restart
- A light theme and a dark theme both read the borders; the deadline tint and the dragged card (faded) still look right
