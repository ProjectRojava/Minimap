// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod state;

use std::{fs, sync::Mutex};

use anyhow::Context;
use tauri::Manager;

use crate::state::AppState;

fn init_logging(dir: &std::path::Path) -> anyhow::Result<()> {
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("minimap.log"))
        .context("open log file")?;
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(())
}

fn main() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir().context("resolve app data dir")?;
            fs::create_dir_all(&dir).context("create app data dir")?;
            init_logging(&dir)?;
            let db_path = dir.join("minimap.db");
            let conn = minimap_store::open(&db_path)
                .map_err(|e| anyhow::anyhow!("open database {}: {e}", db_path.display()))?;
            tracing::info!(path = %db_path.display(), "database opened");
            app.manage(AppState {
                db: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::system::ping])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("error while running Minimap: {e}");
        std::process::exit(1);
    }
}
