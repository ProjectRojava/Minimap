# UI design

Flat, IDE-like (think Zed): a grey chrome with one accent colour and a few status colours, used sparingly. The work is what stands out.

## Rules
- **Tokens only.** Colours come from the CSS variables in `ui/style/input.css`, used through the semantic utilities below. No raw palette classes (`zinc-*`, `emerald-*`, `red-*`), no `dark:` variants, no hex in components. Light and dark then stay in sync for free.
- **Flat.** No shadows, gradients or blur. Separate areas with a 1px `border-line` hairline or a `bg-panel` / `bg-canvas` change. Radius `rounded-sm` at most.
- **Grey first, colour sparingly.** Hierarchy still comes from `fg` / `muted` / `faint` text and weight. Colour has four jobs and no others: **`accent`** marks where you are and what to press (current screen, primary buttons, focus, today, the critical path, mentions); **`success`** is good news (on track, absorbed, done); **`warning`** is needs-attention (amber health, stale, top priority); **`danger`** is errors, overdue and destructive actions. Never decorate with colour, and never let colour be the only signal: health is a shape (filled / half / empty dot) first and a colour second. See ADR-0008.
- **Easy on the eye.** No pure black or white; low-contrast hairlines. `fg` and `muted` meet WCAG AA (≥ 4.5:1) on canvas and panel in every theme (enforced by tests; see Themes); `faint` (~2.5:1) is only for hints and icons that are also available elsewhere (shortcut hints, ✕), never for information you need to read.
- **Dense.** 13px base text, 32px (`h-8`) list rows with a hairline between them, 48px (`h-12`) page headers, 28px sidebar rows, 10px uppercase labels for section and column titles.
- **Primary buttons** are filled with the accent (`bg-accent text-canvas`); everything else is an outlined grey button.
- **No native pop-ups.** The OS toolkit draws a webview's `<select>` list, date picker and checkbox, and they ignore page colours. Use `SelectField` (themed dropdown) and `DateField` (ISO text box + themed calendar) instead of `<select>` and `<input type="date">`; checkboxes and number/search inputs are restyled in `ui/style/input.css`. Dates are always shown and typed as `YYYY-MM-DD`, never in the locale's format.

## Window chrome
The native title bar is disabled; `TitleBar` (32px) draws the app name, a drag region and minimize / maximize / close (flat, `currentColor` line icons; close turns `danger` on hover). It sits above the sidebar, content and pane, so full-height overlays start at `top-8`. On macOS the native traffic lights overlay the bar. See ADR-0004.

## Pages
Every list screen is built from the same pieces (`ui/src/components/page.rs`), so they look and behave alike:
- **`PageHeader`** (48px, `bg-panel`): a 28px icon tile (the screen's sidebar icon), the title (14px semibold), a one-line description (hidden below ~1024px), then the page's own buttons and toggles as children. **`Hints`** renders the keyboard shortcuts as key chips at the right (hidden below ~1280px).
- **`FILTER_BAR`** under the header (search, dropdowns, date ranges); **`FORM_BAR`** (`bg-panel`) for the inline "new" forms.
- **Column headings** (`column_head`): 28px, 10px uppercase, `bg-panel`, using the list's own grid so they line up with the rows; **`GroupLabel`** for group headings with a count chip.
- **`NodeRow`**: 32px, hairline below, hover `bg-hover`, keyboard cursor `bg-hover` + a 2px muted left bar, open item `bg-active` + a 2px `fg` left bar (the same marker the sidebar uses).
- **`CHIP`** for statuses and kinds; **`CHIP_STRONG`** (stronger border, `fg` text) for what needs attention. No colour: weight and border carry it.
- **`EmptyState`** (icon tile, title, hint) instead of a bare sentence; **`Card`** (title, description, body) for Settings.
- The Overview adds four **stat tiles** (red / amber / green / not scored) above its lists.

## Sidebar
`Sidebar` (208px, `bg-panel`): the search box in its own hairline-separated header; the screens in **groups** with 10px uppercase headings (none for the first group, then Plan, People, Log); a footer with Settings and a "Command palette" button showing the shortcut as key chips. Each entry is a 28px row with a 16px **line icon** (`currentColor`, 1.4px round stroke, path data in `ui/src/icons.rs`), the label, and a quiet `g x` key chip that appears on hover. The current screen has `bg-active`, medium weight, `text-fg` and a 2px `bg-fg` bar on its left edge; everything else is `text-muted` and turns `text-fg` on hover. Groups and icons come from the `group` and `icon` fields of `nav::NAV`, so a new screen is one table entry.

## Tokens
| Utility | Use |
|---|---|
| `bg-canvas` | main content area, inputs |
| `bg-panel` | sidebar, detail pane, headers, toasts, dialogs |
| `bg-hover` | hover; keyboard cursor row |
| `bg-active` | selected row, current nav item |
| `border-line` / `border-line-strong` | hairlines (default border colour) / focused inputs |
| `text-fg` / `text-muted` / `text-faint` | primary / secondary / tertiary text |
| `text-accent`, `bg-accent`, `border-accent` (and `/10`, `/30` tints) | current screen, primary action, focus ring, today, critical path, mentions |
| `text-success` | on track, absorbed, done |
| `text-warning`, `border-warning` | needs attention: amber health, stale, `CHIP_STRONG` |
| `text-danger`, `border-l-danger` | errors, overdue, destructive actions |
| `bg-scrim` | dimming behind overlays |

## Themes
**Dark is the default.** The user picks a theme in Settings → Appearance; "System" follows the OS (Minimap Dark / Minimap Light). The choice is stored in the database (`settings.theme`) and cached in `localStorage` so the next launch is correct immediately.

A theme is **14 colours** (the tokens above plus `line`, `hover`, `active`, `fg`, ... exactly as listed in `ui/src/themes.rs::Tokens`) and a kind (dark or light). At runtime `ui/src/theme.rs` writes them as CSS custom properties on `<html>` (and sets `color-scheme` and `data-theme="<id>"`); components never know which theme is active. `ui/style/input.css` holds the default (dark) values so the first paint is already right, and a test keeps it in sync with the default theme.

Built-in themes (adapted from the originals): Minimap Dark/Light, One Dark/Light, Dracula, Nord, Solarized Dark/Light, Gruvbox Dark/Light, Catppuccin Mocha/Latte, Tokyo Night, GitHub Dark, Rosé Pine/Dawn, Monokai.

### Adding a theme
1. Append an entry to `ALL` in `ui/src/themes.rs` (bump the array length): id (lowercase, digits, hyphens), name, dark/light, 14 colours.
2. Map the palette like this: `canvas` = the editor background; `panel` = the sidebar/darker or lighter surface next to it; `hover` and `active` = the palette's line-highlight and selection backgrounds; `line` / `line_strong` = its border and stronger border; `fg` = the main foreground; `muted` = a secondary text colour; `faint` = the comment grey; `danger` = the palette's red; `accent` = its blue (or purple/cyan, whatever it uses for links and selection); `success` = its green; `warning` = its yellow or orange; `scrim` = a translucent black.
3. Run `cargo test -p minimap-ui themes`. The tests require unique valid ids, hex colours, dark/light kind matching the colours, `fg` ≥ 4.5:1 on canvas, panel and active, `muted` ≥ 4.5:1 on canvas and panel (≥ 3.5:1 on active), and `danger`, `accent`, `success` and `warning` ≥ 4.5:1 on canvas and panel (≥ 3.5:1 on the selected row), because they are used as text and as the fill behind canvas-coloured button labels. Palettes whose own colours miss these (common for comment greys and reds) get nudged toward white or black until they pass; today that is `fg`, `muted` or `danger` of One Dark, Nord, Solarized Dark/Light, Gruvbox Dark, Catppuccin Latte, Rosé Pine, Rosé Pine Dawn and Monokai, each by a few percent.
No backend or CSS change is needed. User-supplied themes (importing a palette file) would use the same 11-colour shape and are a possible later addition.

The window itself starts with the default dark background colour (`backgroundColor` in `tauri.conf.json`) to avoid a white flash; a light theme can show a brief dark frame before the app applies it.
