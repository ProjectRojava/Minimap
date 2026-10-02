use minimap_types::{AppError, PingResponse};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Round-trips through core and the database; returns "pong".
#[tauri::command]
pub async fn ping(state: State<'_, AppState>) -> Result<PingResponse, AppError> {
    let version = state
        .run(|conn| minimap_store::schema_version(conn).map_err(store_error))
        .await?;
    Ok(minimap_core::pong(version))
}
