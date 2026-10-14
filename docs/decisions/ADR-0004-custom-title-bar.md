# ADR-0004: Custom title bar and window permissions

## Context
The native title bar doesn't fit the flat, monochrome design (see `docs/design.md`), so the window is created without decorations and the title bar is drawn in Leptos.

## Decision
- `tauri.conf.json`: `decorations: false`. The Leptos `TitleBar` provides the drag region (`data-tauri-drag-region`), the app name, and minimize / maximize-restore / close buttons.
- **Permissions**: `CLAUDE.md` says to grant the frontend nothing beyond the app's own commands and dialog/fs. The title bar needs a minimal set of window permissions in `capabilities/default.json`: `core:window:allow-minimize`, `allow-toggle-maximize`, `allow-internal-toggle-maximize` (double-click on the bar), `allow-is-maximized`, `allow-close`, `allow-start-dragging`, `allow-start-resize-dragging`. Nothing else from `core:window`.
- **Resizing**: Windows resizes undecorated windows natively. On Linux the UI draws invisible resize borders that call `startResizeDragging`. macOS can't resize undecorated windows, so `tauri.macos.conf.json` keeps the native traffic lights over our bar (`titleBarStyle: Overlay`, `hiddenTitle`), and the UI omits its own buttons there and leaves room for them.
- The platform is detected from the webview user agent (`ui/src/window.rs`).

## Consequences
- The bar is part of every screen's layout (32px, `h-8`); overlays that cover the window sit below it (`top-8`).
- Window behaviour can only be checked by running the app on each OS. The macOS path is untested.
- Under WSLg the Windows host may still draw its own frame around the Linux window.
- **Amended** (Portfolio / My view, spec 50): the title bar also holds the view button (`ViewToggle`, left of the window controls) that switches between Portfolio view and My view. It is an ordinary button beside the drag region, not a window control, so it needs no window permission; the choice is per device (`localStorage`), nothing in the database.
- **Amended by [ADR-0021](ADR-0021-a-second-window-the-radar.md)** (the radar, spec 53): a second undecorated window, the radar, has its own small top bar (view, pin, close) instead of this title bar, moves with `core:window:allow-start-dragging` and resizes with `core:window:allow-start-resize-dragging` (its capability only); the main title bar gains a *Radar* button beside the view button. On Windows the radar resizes natively, on Linux and macOS it draws the same invisible edge grips as this bar's `ResizeHandles`.
