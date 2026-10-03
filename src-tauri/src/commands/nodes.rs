use minimap_types::{Activity, AppError, EdgeLink, NodeRef, NodeSummary, NodeType, Uuid};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Label and archived state of any node (detail pane header, link targets).
#[tauri::command]
pub async fn get_node_summary(
    state: State<'_, AppState>,
    node: NodeRef,
) -> Result<NodeSummary, AppError> {
    state
        .run(move |conn| minimap_store::nodes::summary(conn, node).map_err(store_error))
        .await
}

/// Active edges touching a node, with the node on the other end summarised.
#[tauri::command]
pub async fn list_edges_for(
    state: State<'_, AppState>,
    node_id: Uuid,
) -> Result<Vec<EdgeLink>, AppError> {
    state
        .run(move |conn| minimap_store::edges::links_for_node(conn, node_id).map_err(store_error))
        .await
}

/// History of one node, newest first.
#[tauri::command]
pub async fn list_activity_for(
    state: State<'_, AppState>,
    node_id: Uuid,
) -> Result<Vec<Activity>, AppError> {
    state
        .run(move |conn| minimap_store::activity::list_for_node(conn, node_id).map_err(store_error))
        .await
}

/// Active nodes of one type by label, for pickers and link targets.
#[tauri::command]
pub async fn list_node_summaries(
    state: State<'_, AppState>,
    node_type: NodeType,
) -> Result<Vec<NodeSummary>, AppError> {
    state
        .run(move |conn| minimap_store::nodes::list_summaries(conn, node_type).map_err(store_error))
        .await
}
