use minimap_core::objectives::arrange;
use minimap_types::{
    AppError, CreateObjective, NodeRef, NodeType, Objective, ObjectiveDetail, ObjectiveGroup,
    ObjectiveGrouping, UpdateObjective, Uuid,
};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Active objectives, sorted by priority (1 = highest) then target date, optionally
/// grouped by the calendar quarter of the target date.
#[tauri::command]
pub async fn list_objectives(
    state: State<'_, AppState>,
    grouping: ObjectiveGrouping,
) -> Result<Vec<ObjectiveGroup>, AppError> {
    state
        .run(move |conn| {
            let rows = minimap_store::views::objective_rows(conn).map_err(store_error)?;
            Ok(arrange(rows, grouping))
        })
        .await
}

#[tauri::command]
pub async fn get_objective(state: State<'_, AppState>, id: Uuid) -> Result<Objective, AppError> {
    state
        .run(move |conn| minimap_store::objectives::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_objective_detail(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<ObjectiveDetail, AppError> {
    state
        .run(move |conn| minimap_store::views::objective_detail(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn create_objective(
    state: State<'_, AppState>,
    input: CreateObjective,
) -> Result<Objective, AppError> {
    state
        .run(move |conn| minimap_store::objectives::create(conn, input).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_objective(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateObjective,
) -> Result<Objective, AppError> {
    state
        .run(move |conn| minimap_store::objectives::update(conn, id, patch).map_err(store_error))
        .await
}

/// Archives the objective and its contribution links.
#[tauri::command]
pub async fn archive_objective(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::nodes::archive(conn, NodeRef::new(NodeType::Objective, id))
                .map_err(store_error)
        })
        .await
}
