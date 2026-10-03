use minimap_core::projects::{by_objective, by_status, filter};
use minimap_types::{
    AppError, CreateProject, Project, ProjectArchivePreview, ProjectDetail, ProjectFilter,
    ProjectGroup, ProjectLayout, TaskDisposition, UpdateProject, Uuid,
};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Active projects, filtered, then arranged as list sections (by objective) or board
/// columns (by status).
#[tauri::command]
pub async fn list_projects(
    state: State<'_, AppState>,
    filter_by: ProjectFilter,
    layout: ProjectLayout,
) -> Result<Vec<ProjectGroup>, AppError> {
    state
        .run(move |conn| {
            let rows = minimap_store::views::project_rows(conn).map_err(store_error)?;
            let rows = filter(rows, &filter_by);
            Ok(match layout {
                ProjectLayout::List => by_objective(rows),
                ProjectLayout::Board => by_status(rows),
            })
        })
        .await
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: Uuid) -> Result<Project, AppError> {
    state
        .run(move |conn| minimap_store::projects::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_project_detail(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<ProjectDetail, AppError> {
    state
        .run(move |conn| minimap_store::views::project_detail(conn, id).map_err(store_error))
        .await
}

/// The handle is generated from the title unless one is given.
#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    input: CreateProject,
) -> Result<Project, AppError> {
    state
        .run(move |conn| minimap_store::projects::create(conn, input).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateProject,
) -> Result<Project, AppError> {
    state
        .run(move |conn| minimap_store::projects::update(conn, id, patch).map_err(store_error))
        .await
}

/// The active tasks archiving the project would affect; show before `archive_project`.
#[tauri::command]
pub async fn preview_archive_project(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<ProjectArchivePreview, AppError> {
    state
        .run(move |conn| {
            minimap_store::views::project_archive_preview(conn, id).map_err(store_error)
        })
        .await
}

/// Archives the project and its links; its tasks are archived too or moved to the inbox.
#[tauri::command]
pub async fn archive_project(
    state: State<'_, AppState>,
    id: Uuid,
    tasks: TaskDisposition,
) -> Result<(), AppError> {
    state
        .run(move |conn| minimap_store::projects::archive(conn, id, tasks).map_err(store_error))
        .await
}
