use minimap_types::{AppError, Settings, UpdateSettings};
use tauri::State;

use crate::{error::store_error, state::AppState};

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
