# Minimap

A local-first, private desktop command center for CTOs, CXOs and project managers: objectives, projects, tasks, people and teams as a typed graph. Rust everywhere (Tauri 2 + Leptos/WASM + SQLite). No Node/npm.

See [CLAUDE.md](CLAUDE.md) for the full spec and [docs/progress.md](docs/progress.md) for status.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install tauri-cli --version "^2" --locked
cargo install trunk --locked
```

**Linux** also needs the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/#linux). On Debian/Ubuntu:

```bash
sudo apt install pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev perl make
```

macOS needs Xcode command line tools; Windows needs the MSVC build tools and WebView2.

## Run

```bash
cargo tauri dev     # app with hot reload (Trunk serves the UI on :1420)
cargo tauri build   # production installer
cargo test --workspace
```

Trunk downloads the standalone Tailwind binary on first build; no npm is involved.
