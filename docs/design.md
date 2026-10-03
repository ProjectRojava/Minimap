# UI design

Flat, monochrome, IDE-like (think Zed): the work is the colour, the chrome is grey.

## Rules
- **Tokens only.** Colours come from the CSS variables in `ui/style/input.css`, used through the semantic utilities below. No raw palette classes (`zinc-*`, `emerald-*`, `red-*`), no `dark:` variants, no hex in components. Light and dark then stay in sync for free.
- **Flat.** No shadows, gradients or blur. Separate areas with a 1px `border-line` hairline or a `bg-panel` / `bg-canvas` change. Radius `rounded-sm` at most.
- **Monochrome.** Hierarchy comes from `fg` / `muted` / `faint` text and weight, not hue. The one hue is `danger`, for errors and destructive actions. Status colours (health, overload) are added only where the colour carries meaning, and sparingly.
- **Easy on the eye.** No pure black or white; low-contrast hairlines. `fg` and `muted` meet WCAG AA (≥ 4.5:1) on canvas and panel in every theme (enforced by tests; see Themes); `faint` (~2.5:1) is only for hints and icons that are also available elsewhere (shortcut hints, ✕), never for information you need to read.
- **Dense.** 13px base text, 28px (`h-7`) rows, 40px (`h-10`) headers, 11px uppercase labels for section titles.
- **Primary buttons** are inverted (`bg-fg text-canvas`), not coloured.

## Window chrome
The native title bar is disabled; `TitleBar` (32px) draws the app name, a drag region and minimize / maximize / close (flat, `currentColor` line icons; close turns `danger` on hover). It sits above the sidebar, content and pane, so full-height overlays start at `top-8`. On macOS the native traffic lights overlay the bar. See ADR-0004.

## Tokens
| Utility | Use |
|---|---|
| `bg-canvas` | main content area, inputs |
| `bg-panel` | sidebar, detail pane, headers, toasts, dialogs |
| `bg-hover` | hover; keyboard cursor row |
| `bg-active` | selected row, current nav item |
| `border-line` / `border-line-strong` | hairlines (default border colour) / focused inputs |
| `text-fg` / `text-muted` / `text-faint` | primary / secondary / tertiary text |
| `text-danger`, `border-l-danger` | errors, destructive actions |
| `bg-scrim` | dimming behind overlays |

## Themes
**Dark is the default.** The user picks a theme in Settings → Appearance; "System" follows the OS (Minimap Dark / Minimap Light). The choice is stored in the database (`settings.theme`) and cached in `localStorage` so the next launch is correct immediately.

A theme is **11 colours** (the tokens above plus `line`, `hover`, `active`, `fg`, ... exactly as listed in `ui/src/themes.rs::Tokens`) and a kind (dark or light). At runtime `ui/src/theme.rs` writes them as CSS custom properties on `<html>` (and sets `color-scheme` and `data-theme="<id>"`); components never know which theme is active. `ui/style/input.css` holds the default (dark) values so the first paint is already right, and a test keeps it in sync with the default theme.

Built-in themes (adapted from the originals): Minimap Dark/Light, One Dark/Light, Dracula, Nord, Solarized Dark/Light, Gruvbox Dark/Light, Catppuccin Mocha/Latte, Tokyo Night, GitHub Dark, Rosé Pine/Dawn, Monokai.

### Adding a theme
1. Append an entry to `ALL` in `ui/src/themes.rs` (bump the array length): id (lowercase, digits, hyphens), name, dark/light, 11 colours.
2. Map the palette like this: `canvas` = the editor background; `panel` = the sidebar/darker or lighter surface next to it; `hover` and `active` = the palette's line-highlight and selection backgrounds; `line` / `line_strong` = its border and stronger border; `fg` = the main foreground; `muted` = a secondary text colour; `faint` = the comment grey; `danger` = the palette's red; `scrim` = a translucent black.
3. Run `cargo test -p minimap-ui themes`. The tests require unique valid ids, hex colours, dark/light kind matching the colours, `fg` ≥ 4.5:1 on canvas, panel and active, `muted` ≥ 4.5:1 on canvas and panel (≥ 3.5:1 on active), and `danger` ≥ 4.5:1 on canvas and panel. Palettes whose own colours miss these (common for comment greys and reds) get nudged toward white or black until they pass; today that is `fg`, `muted` or `danger` of One Dark, Nord, Solarized Dark/Light, Gruvbox Dark, Catppuccin Latte, Rosé Pine, Rosé Pine Dawn and Monokai, each by a few percent.
No backend or CSS change is needed. User-supplied themes (importing a palette file) would use the same 11-colour shape and are a possible later addition.

The window itself starts with the default dark background colour (`backgroundColor` in `tauri.conf.json`) to avoid a white flash; a light theme can show a brief dark frame before the app applies it.
