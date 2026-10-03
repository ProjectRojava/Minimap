use minimap_types::{
    AppError, CreatePerson, NodeRef, NodeType, Person, PersonArchivePreview, PersonDetail,
    PersonRow, UpdatePerson, Uuid,
};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// The person who is the user, or `None` before first-run setup.
#[tauri::command]
pub async fn get_self_person(state: State<'_, AppState>) -> Result<Option<Person>, AppError> {
    state
        .run(|conn| minimap_store::people::get_self(conn).map_err(store_error))
        .await
}

/// First-run setup: creates the self person (blank name becomes "Me"). Idempotent.
#[tauri::command]
pub async fn create_self_person(
    state: State<'_, AppState>,
    name: String,
) -> Result<Person, AppError> {
    state
        .run(move |conn| minimap_store::people::ensure_self(conn, &name).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn list_people(state: State<'_, AppState>) -> Result<Vec<PersonRow>, AppError> {
    state
        .run(|conn| minimap_store::views::people_rows(conn).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_person(state: State<'_, AppState>, id: Uuid) -> Result<Person, AppError> {
    state
        .run(move |conn| minimap_store::people::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_person_detail(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<PersonDetail, AppError> {
    state
        .run(move |conn| minimap_store::views::person_detail(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn create_person(
    state: State<'_, AppState>,
    input: CreatePerson,
) -> Result<Person, AppError> {
    // The self person only comes from first-run setup.
    let input = CreatePerson {
        is_self: false,
        ..input
    };
    state
        .run(move |conn| minimap_store::people::create(conn, input).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_person(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdatePerson,
) -> Result<Person, AppError> {
    state
        .run(move |conn| minimap_store::people::update(conn, id, patch).map_err(store_error))
        .await
}

/// What archiving this person would leave unassigned; show it before `archive_person`.
#[tauri::command]
pub async fn preview_archive_person(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<PersonArchivePreview, AppError> {
    state
        .run(move |conn| {
            minimap_store::views::person_archive_preview(conn, id).map_err(store_error)
        })
        .await
}

/// Archives the person and their links (so their tasks become unassigned).
#[tauri::command]
pub async fn archive_person(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::nodes::archive(conn, NodeRef::new(NodeType::Person, id))
                .map_err(store_error)
        })
        .await
}
