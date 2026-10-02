# ADR-0001: Stable versions and command allow-list

- **Tauri 2.12 / Leptos 0.8**: crates.io also carries Tauri 3 alpha and Leptos 0.9 beta; the spec asks for Tauri 2.x and Leptos stable, so we pin the latest stable releases.
- **Command allow-list**: `src-tauri/build.rs` declares an app manifest listing every command, and `capabilities/default.json` grants only `allow-<command>` permissions. Adding a command means updating both.
- **Keyring** is not a dependency yet; it arrives in M4.
