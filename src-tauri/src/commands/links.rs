//! Reference links (spec 28): opening one in the system's browser or mail program. The links
//! themselves are a field of the task (`update_task`); which addresses are accepted is
//! `minimap_core::links`.

use minimap_types::AppError;

use crate::error::app_error;

/// Opens a web or email address with the program the system uses for it. Anything else is
/// refused, so a link can never open a file or run something.
#[tauri::command]
pub async fn open_link(url: String) -> Result<(), AppError> {
    let url = minimap_core::links::normalize(&url).map_err(|e| app_error("invalid", e))?;
    open::that_detached(&url).map_err(|e| app_error("io", format!("Couldn't open the link: {e}")))
}
