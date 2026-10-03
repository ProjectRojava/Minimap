# UI design

Flat, monochrome, IDE-like (think Zed): the work is the colour, the chrome is grey.

## Rules
- **Tokens only.** Colours come from the CSS variables in `ui/style/input.css`, used through the semantic utilities below. No raw palette classes (`zinc-*`, `emerald-*`, `red-*`), no `dark:` variants, no hex in components. Light and dark then stay in sync for free.
- **Flat.** No shadows, gradients or blur. Separate areas with a 1px `border-line` hairline or a `bg-panel` / `bg-canvas` change. Radius `rounded-sm` at most.
- **Monochrome.** Hierarchy comes from `fg` / `muted` / `faint` text and weight, not hue. The one hue is `danger`, for errors and destructive actions. Status colours (health, overload) are added only where the colour carries meaning, and sparingly.
- **Easy on the eye.** No pure black or white; low-contrast hairlines. `fg` and `muted` meet WCAG AA (≥ 4.5:1) on canvas, panel and selected rows in both themes; `faint` (~2.5:1) is only for hints and icons that are also available elsewhere (shortcut hints, ✕), never for information you need to read.
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

The theme follows the OS. `<html data-theme="light|dark">` forces one (wired up by Settings, feature 23).
