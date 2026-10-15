//! The Archive screen (everything that was archived, and bringing it back) and the sweep that
//! archives finished tasks that are old enough (`store::done_sweep`, Settings).

use minimap_types::{AppError, ArchivedItem, NodeRef};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Everything that has been archived, newest first.
#[tauri::command]
pub async fn list_archived(state: State<'_, AppState>) -> Result<Vec<ArchivedItem>, AppError> {
    state
        .run(|conn| minimap_store::archive::list_all(conn).map_err(store_error))
        .await
}

/// Brings an archived item back, with the links that were archived along with it.
#[tauri::command]
pub async fn restore_archived(state: State<'_, AppState>, node: NodeRef) -> Result<(), AppError> {
    state
        .run(move |conn| minimap_store::archive::restore(conn, node).map_err(store_error))
        .await
}

/// Archives the finished tasks that are old enough. Run at start and then hourly, so Done never
/// fills up, whether or not the app was open when a task came of age.
pub(crate) fn archive_old_done_tick(state: &AppState) {
    let Ok(mut vault) = state.db.lock() else {
        return;
    };
    let Ok((conn, _)) = vault.parts() else {
        return; // locked: try again later
    };
    match minimap_store::done_sweep::archive_old_done(conn, minimap_store::today()) {
        Ok(gone) if gone.is_empty() => {}
        Ok(gone) => {
            tracing::info!(archived = gone.len(), "archived finished tasks that came of age");
            // A write like any other: leave it to be saved to Drive.
            state.sync.engine.note_change(conn);
        }
        Err(e) => tracing::warn!(error = %e, "archiving old finished tasks failed"),
    }
}
