# 11 — Brand and voice: the name, the mark and the words

Status: Draft · Milestone: — · Priority: Could · Size: S
Depends on: spec 40 (About), the Help wiki · Read first: [README](README.md)
Expected ADR: none (record the voice rules in a new `docs/voice.md`; `docs/design.md` links to it)

## Goal
Make the name mean something to the person using the app, quietly. Three parts:
1. **The mark**: refine the existing logo so it reads as a minimap at 16px.
2. **The voice**: a short guide for the words the app uses, so every screen, toast, empty state and Help page sounds like one product, and so docs 01–10 use the same vocabulary.
3. **The small moments**: a "Why the name" line in About and the Help welcome page, empty states rewritten in the voice, and one easter egg that already exists (`ping` → `pong`).

No game art, no mascots, no sounds (README principle 5).

## 1. The mark
The current logo (`branding/minimap-logo-{dark,light,auto}.svg`, 512×512) is already a minimap: a faint grid, a small graph of linked nodes (teal `#3FC7B8`), a **viewport rectangle** (the box game minimaps draw to show what's on screen) and a fog vignette at the edges. Keep that idea; tighten it:

- **Legibility at 16 and 32px**: today the grid, the fog gradient and the thin lines disappear and the mark becomes a blob. Make a **small-size variant** `branding/minimap-mark-small.svg` (viewBox 32×32): rounded tile, the viewport rectangle (2px stroke), three nodes (one inside the rectangle, two outside) and one link, with no grid and no gradient. Use it for the title bar (`ui/src/components/titlebar.rs` shows a 16px image) and the 32×32 app icon. Keep the large logo for 128px and up.
- **"You are here"**: the node inside the viewport rectangle becomes the accent node (slightly larger, solid); the others are the same colour at lower opacity. This is the one change to the large logo as well.
- **App icons**: regenerate `src-tauri/icons/*` from the updated SVGs with `cargo tauri icon branding/minimap-logo-dark.svg` (tauri-cli, no npm; it needs a PNG/SVG source; if it needs a 1024px PNG, export it with a Rust tool or `resvg` (`cargo install resvg`), never a Node tool). Commit the generated set. Check the `.ico`/`.icns` contain the small variant at 16/32 (if `cargo tauri icon` can't take separate sources per size, accept its downscale and note it).
- The logo SVGs are **brand assets**, not UI: they may keep their own colours and gradients (the "tokens only, no gradients" rule in `docs/design.md` applies to the app's UI). Say so in one line in `docs/design.md`.
- **Test**: none automated beyond the build; manual check below.

## 2. The voice (`docs/voice.md`, new)
Write the guide with these sections and examples. It is for whoever writes UI strings, toasts, Help pages and the CHANGELOG.

- **Who we're talking to**: a busy senior person between meetings. They glance. Every string should be readable in one look.
- **Rules**:
  1. Say what is true, then what to do. "3 tasks overdue. Reschedule or finish them." Not "Uh oh!".
  2. Plain words, short sentences, active voice. "Minimap saved a backup", not "A backup has been created".
  3. Spatial words come from the minimap vocabulary (README table: fog, clear, safe zone, approaching, route, recalculated, ping, save point, zoom, radar, Portfolio view, My view) and **only** where the feature has that name. Don't sprinkle metaphors elsewhere ("Your quest log is empty" is banned).
  4. Never judge people. Load, not performance; "over capacity", never "overworked", "slacking", "underperforming", "health" (for a person) or "score".
  5. Calm. No exclamation marks except in a genuine success that the user waited for (none today). No "Oops", "Whoops", "Uh oh", "Awesome".
  6. Numbers are digits; dates are `YYYY-MM-DD` (design rule); durations say working days when they are working days.
  7. Empty states say what the screen would show and how to get it there, in one line each (title + one sentence).
- **Word list**: preferred / avoid pairs (e.g. *archive* / delete (when it's soft), *link* / edge (UI), *task* / ticket, *person* / user, *load* / utilisation, *slip* / delay (verb), *target date* / deadline (for projects)).
- **Examples**: 10 before → after rewrites taken from the current UI.
- Link it from `docs/design.md` and `CLAUDE.md` §11 Conventions (one line: "UI strings follow `docs/voice.md`").

## 3. The small moments
- **About** (`ui/src/pages/about.rs`): add a short *Why "Minimap"* paragraph under the version (constant `WHY_NAME`, tested like `DEVELOPER`):
  > In a game, the minimap is the small map in the corner. You don't play on it; you glance at it and get back to the game. Minimap is meant to be used the same way: a quick look that tells you where things stand, what's coming, and what you haven't looked at lately.
- **Help → Welcome** (`docs/help/welcome.md`): a short *Why the name* section with the same idea in two sentences, and (once the docs exist) a line linking to the radar, fog and My view pages.
- **Empty states**: rewrite the `EmptyState` titles and lines that don't follow the voice. Keep the ones that already do ("Nothing needs attention", "Inbox zero" (keep: it's a known idiom), "Nothing slipped", "Nobody is overloaded"). Candidates to rewrite: generic "Nothing to show yet" (say what would show), "No X match" (add "Clear the filters to see all"). Each changed string: grep the Help pages for it and update them in the same change (the help tests may quote them).
- **Easter egg**: the M0 `ping` command answers "pong" and is still shown on Overview (`api::ping -> ui:Overview`). Decide its fate: **keep it, hidden**: remove it from the Overview UI (if it's still visible there), and make the palette answer the exact text `ping` with a single row "pong · Minimap <version> · schema v<N>" (a quick health check that the backend answers, using the existing `ping` command). It must not appear in the palette for any other input. Document it nowhere except the FAQ's troubleshooting ("Type `ping` in the palette to check Minimap's backend answers").
- **Status bar / title bar**: no change (doc 08 and doc 07 add their own items).

## Help wiki
- `welcome.md`: *Why the name*.
- `faq.md`: the `ping` check.
- Any page quoting an empty-state string that changes.

## Demo data, export, undo, sync
Nothing.

## Out of scope
A website, marketing copy, a new colour palette (themes are data, ADR-0005), renaming the product, sounds.

## Acceptance criteria
- [ ] Small mark SVG added; title bar uses it; large logos updated with the "you" node; icons regenerated with tauri-cli; installer builds.
- [ ] `docs/voice.md` written with rules, word list and 10 rewrites; linked from `docs/design.md` and `CLAUDE.md` §11.
- [ ] About shows *Why "Minimap"* (unit test on the constant); Help welcome has *Why the name*.
- [ ] Empty states reviewed against the voice; changed strings updated in Help; help tests pass.
- [ ] `ping` in the palette answers pong with version and schema; nowhere else in the UI (palette unit test on `matching`/`build_rows`).
- [ ] CHANGELOG (*Changed*: new app icon; *Added*: Why the name), knowledge graph (§8 About, palette), `docs/progress.md`; gate passes.

## Not yet verified by hand
- Title bar at 100% and 200% scaling: the mark reads as a framed map with a dot, not a smudge.
- Taskbar/dock icon on Linux (and Windows/macOS when available) at small sizes.
- About shows the paragraph; Help → Welcome shows the section.
- Palette: type `ping` → one row "pong …"; type `pin` → no pong row.

## Open questions
1. Keep the teal accent in the logo even though the app's accent depends on the theme? **Yes**: the logo is a brand asset; the app keeps its tokens.
2. Should the voice guide also govern commit messages and the CHANGELOG? **CHANGELOG yes** (it's user-facing), commits no.
