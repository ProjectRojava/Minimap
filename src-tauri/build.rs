/// Commands exposed to the frontend. Keep in sync with `invoke_handler` in main.rs.
const COMMANDS: &[&str] = &["ping"];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
