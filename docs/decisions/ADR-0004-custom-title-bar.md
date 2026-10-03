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
