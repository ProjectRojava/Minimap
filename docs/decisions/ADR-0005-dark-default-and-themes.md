# ADR-0005: Dark by default; themes as data

## Context
The UI followed the OS light/dark setting. We want dark as the default, and the option to use established palettes (Nord, Catppuccin, Gruvbox, ...).

## Decision
- **Dark is the default.** `ui/style/input.css` holds the dark tokens, the window background is dark, and `settings.theme` defaults to `minimap-dark`. "System" (follow the OS) is one of the choices.
- **A theme is data**: 11 colours + a dark/light kind (`ui/src/themes.rs`). The runtime applies it by setting CSS custom properties on `<html>` (`ui/src/theme.rs`); components keep using the semantic token utilities. Adding a theme is one array entry, checked by contrast tests.
- **Storage**: the theme id lives in the `settings` table (the backend only validates the id's shape; the UI owns the list and falls back to the default for unknown ids), cached in `localStorage` for a correct first frame on the next launch.
- **No theme JavaScript.** An inline boot script would need a CSP exception and breaks "Rust everywhere", so a non-default theme can show a brief dark frame at startup (the cache narrows it to the time WASM takes to load).

## Consequences
- The old `prefers-color-scheme` / `data-theme="light|dark"` CSS is gone; light is just another theme.
- Established palettes are adapted, not copied: some text colours are nudged for readability (documented in `docs/design.md`).
- A future "import a theme file" feature can reuse the same shape.
