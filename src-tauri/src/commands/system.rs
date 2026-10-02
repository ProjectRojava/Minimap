use minimap_types::{AppError, PingResponse};
use tauri::State;

use crate::{error::app_error, state::AppState};

/// Round-trips through core and the database; returns "pong".
#[tauri::command]
pub async fn ping(state: State<'_, AppState>) -> Result<PingResponse, AppError> {
    let version = {
        let conn = state
            .db
            .lock()
            .map_err(|_| app_error("internal", "database lock poisoned"))?;
        minimap_store::schema_version(&conn).map_err(|e| app_error("store", e))?
    };
    Ok(minimap_core::pong(version))
}
