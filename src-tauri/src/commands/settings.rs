use std::path::Path;

use minimap_store::{security::Key, Connection};
use minimap_types::{AppError, DataInfo, Settings, UpdateSettings};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    state
        .run(|conn| minimap_store::settings::get(conn).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    patch: UpdateSettings,
) -> Result<Settings, AppError> {
    state
        .run(move |conn| minimap_store::settings::update(conn, patch).map_err(store_error))
        .await
}

/// Where the data lives, for Settings -> Data: the database file and its folder, its size, the
/// schema version, whether it is encrypted, and the log file. Read-only.
#[tauri::command]
pub async fn get_data_info(state: State<'_, AppState>) -> Result<DataInfo, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, _| {
            let (conn, key) = vault.parts()?;
            data_info_impl(conn, &data_dir, key)
        })
        .await
}

pub(crate) fn data_info_impl(
    conn: &Connection,
    data_dir: &Path,
    key: &Key,
) -> Result<DataInfo, AppError> {
    let database = data_dir.join("minimap.db");
    Ok(DataInfo {
        database_path: database.display().to_string(),
        folder: data_dir.display().to_string(),
        database_bytes: std::fs::metadata(&database).map_or(0, |m| m.len()),
        schema_version: minimap_store::schema_version(conn).map_err(store_error)?,
        encrypted: !key.is_none(),
        log_path: data_dir.join("minimap.log").display().to_string(),
    })
}

/// Opens the data folder in the system's file manager. Takes no path: it can only ever open the
/// app's own data folder.
#[tauri::command]
pub async fn show_data_folder(state: State<'_, AppState>) -> Result<(), AppError> {
    let folder = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        open::that_detached(&folder)
            .map_err(|e| app_error("io", format!("Couldn't open the folder: {e}")))
    })
    .await
    .map_err(|e| app_error("internal", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_info_names_the_database_and_its_folder() {
        let dir = std::env::temp_dir().join(format!("minimap-data-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("minimap.db");
        let conn = minimap_store::open(&path).unwrap();
        let info = data_info_impl(&conn, &dir, &Key::None).unwrap();
        assert_eq!(info.database_path, path.display().to_string());
        assert_eq!(info.folder, dir.display().to_string());
        assert!(info.database_bytes > 0, "the file exists and has a size");
        assert_eq!(info.schema_version, minimap_store::LATEST_SCHEMA);
        assert!(!info.encrypted);
        assert!(info.log_path.ends_with("minimap.log"));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
