//! This computer's own settings (what closing the window does, the tray, starting with the
//! computer, reminders, locking) and the commands that act on the window: minimise, quit and the
//! answer to "keep running in the tray?". The settings are a file in the data folder, not the
//! database (`shell.rs`), so another computer can have other answers and the tray can read them
//! before the database is open.

use minimap_types::{AppError, CloseChoice, DeviceSettings, DeviceView, OnClose};
use tauri::{AppHandle, Manager, State};

use crate::{
    error::app_error,
    shell::Shell,
    state::{AppState, Vault},
    tray,
};

/// Starting with the computer is only registered by installed builds: a development build would
/// put its own, temporary, program in the list.
const REGISTERS_AUTOSTART: bool = !cfg!(debug_assertions);

/// Makes the computer's start-up list say what the setting says.
pub fn apply_autostart(app: &AppHandle, want: bool) -> Result<(), String> {
    if !REGISTERS_AUTOSTART {
        return Ok(());
    }
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if want {
        manager.enable()
    } else if manager.is_enabled().unwrap_or(false) {
        manager.disable()
    } else {
        Ok(())
    };
    result.map_err(|e| e.to_string())
}

fn view(shell: &Shell, vault_can_lock: bool) -> DeviceView {
    DeviceView {
        settings: shell.settings(),
        tray_available: shell.tray_available(),
        can_lock: vault_can_lock,
    }
}

async fn can_lock(state: &AppState) -> Result<bool, AppError> {
    state
        .run_vault(|vault: &mut Vault, _| Ok(crate::commands::security::lockable(vault)))
        .await
}

#[tauri::command]
pub async fn get_device_settings(
    shell: State<'_, Shell>,
    state: State<'_, AppState>,
) -> Result<DeviceView, AppError> {
    Ok(view(&shell, can_lock(&state).await?))
}

#[tauri::command]
pub async fn update_device_settings(
    app: AppHandle,
    shell: State<'_, Shell>,
    state: State<'_, AppState>,
    settings: DeviceSettings,
) -> Result<DeviceView, AppError> {
    settings
        .check()
        .map_err(|message| app_error("invalid", message))?;
    // The computer's list first: if it refuses, nothing is saved that would then be untrue.
    if settings.start_at_login != shell.settings().start_at_login {
        apply_autostart(&app, settings.start_at_login).map_err(|e| {
            app_error(
                "io",
                format!("Couldn't change whether Minimap starts with the computer: {e}"),
            )
        })?;
    }
    shell
        .set_settings(settings)
        .map_err(|e| app_error("io", format!("Couldn't keep the settings: {e}")))?;
    Ok(view(&shell, can_lock(&state).await?))
}

/// The answer from the "keep running in the tray?" box. With `remember`, closing does this from
/// now on without asking.
#[tauri::command]
pub async fn answer_close(
    app: AppHandle,
    shell: State<'_, Shell>,
    choice: CloseChoice,
    remember: bool,
) -> Result<(), AppError> {
    if remember {
        let mut settings = shell.settings();
        settings.on_close = match choice {
            CloseChoice::Tray => OnClose::Tray,
            CloseChoice::Quit => OnClose::Quit,
        };
        shell
            .set_settings(settings)
            .map_err(|e| app_error("io", format!("Couldn't keep your answer: {e}")))?;
    }
    match choice {
        CloseChoice::Tray if shell.tray_available() => tray::hide_main(&app),
        _ => tray::quit(&app),
    }
    Ok(())
}

/// The title bar's minimise button: to the tray when Settings says so, else to the task bar.
#[tauri::command]
pub async fn minimize_main(app: AppHandle, shell: State<'_, Shell>) -> Result<(), AppError> {
    if shell.settings().minimise_to_tray && shell.tray_available() {
        tray::hide_main(&app);
        return Ok(());
    }
    if let Some(main) = app.get_webview_window("main") {
        main.minimize()
            .map_err(|e| app_error("window", format!("Couldn't minimize: {e}")))?;
    }
    Ok(())
}

/// Closes Minimap for good, whatever closing the window does.
#[tauri::command]
pub async fn quit_app(app: AppHandle) -> Result<(), AppError> {
    tray::quit(&app);
    Ok(())
}
