//! Undo and redo (spec 25). The stack and the logic are in `crate::undo`.

use minimap_types::{AppError, UndoOutcome};
use tauri::State;

use crate::{
    error::app_error,
    state::AppState,
    undo::{step, Direction},
};

async fn run(state: &AppState, direction: Direction) -> Result<UndoOutcome, AppError> {
    let stack = state.undo.clone();
    // Through `run_vault`, not `run`: undoing is not itself a new step to undo.
    state
        .run_vault(move |vault, _| {
            let conn = vault.parts()?.0;
            let mut stack = stack
                .lock()
                .map_err(|_| app_error("internal", "undo history lock poisoned"))?;
            step(conn, &mut stack, direction)
        })
        .await
}

/// Takes back the last change this session ("Undone: archived task X"), up to 20 steps.
/// A step that can't be taken back (a delete, an attachment, a link's details) or that things
/// have changed under says so and is skipped, so the next call goes on to the one before it.
#[tauri::command]
pub async fn undo_last(state: State<'_, AppState>) -> Result<UndoOutcome, AppError> {
    run(state.inner(), Direction::Undo).await
}

/// Puts back what the last undo took back.
#[tauri::command]
pub async fn redo_last(state: State<'_, AppState>) -> Result<UndoOutcome, AppError> {
    run(state.inner(), Direction::Redo).await
}
