# Minimap

A local-first, private desktop command center for CTOs, CXOs and project managers: objectives, projects, tasks, people and teams as a typed graph. Rust everywhere (Tauri 2 + Leptos/WASM + SQLite). No Node/npm.

See [CLAUDE.md](CLAUDE.md) for the full spec, [docs/progress.md](docs/progress.md) for status, and [CHANGELOG.md](CHANGELOG.md) for release notes. For versioning and releases, see [VERSIONING.md](VERSIONING.md).

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

### Google Drive (optional)

Minimap works fully offline without it. To store and sync your data through your own Google Drive
(Settings → Google Drive), Google needs an OAuth client for the app:

1. In the [Google Cloud Console](https://console.cloud.google.com/) create a project and turn on the **Google Drive API**.
2. Configure the OAuth consent screen, then create an **OAuth client ID** of type **Desktop app**.
3. Copy `.env.example` to `.env` (git-ignored) and put the client ID and secret in it (or set
   `MINIMAP_GOOGLE_CLIENT_ID` / `MINIMAP_GOOGLE_CLIENT_SECRET` in CI). The build bakes them into
   the app, so users just click **Sign in with Google** and never see credentials. Rebuild after
   changing them. (A build without them shows an Advanced box in Settings for testing.)

Minimap asks only for the `drive.file` permission (it sees just the files it made). Google treats a
desktop app's secret as non-confidential. Public distribution needs Google's consent-screen verification.

## Run

```bash
cargo tauri dev     # app with hot reload (Trunk serves the UI on :1420)
cargo tauri build   # production installer
cargo test --workspace
```

Trunk downloads the standalone Tailwind binary on first build; no npm is involved.
