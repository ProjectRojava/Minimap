# Progress

## Current milestone: M0 (Scaffold) — in progress

- [x] Workspace, crates, toolchain file
- [x] Tauri 2 shell with `ping` command, restricted to app commands via capability
- [x] SQLite open + first migration (+ tests)
- [x] Leptos UI + Trunk + Tailwind wiring, `api.rs`
- [ ] `cargo tauri dev` shows "pong" (needs Linux system deps: webkit2gtk-4.1, libsoup3, etc.)
- [ ] `cargo tauri build` produces an installer
- [ ] README, ADRs

## M1 progress
- [x] 01 Data model and activity log (store + types; no UI/commands yet)
- [x] 02 App shell and navigation (implemented; manual click-through pending, see spec)
