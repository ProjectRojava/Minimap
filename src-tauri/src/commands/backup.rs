use std::path::{Path, PathBuf};

use minimap_core::backup::{is_due, parse_name};
use minimap_store::{security::Key, Connection};
use minimap_types::{
    AppError, BackupEntry, BackupKind, BackupStatus, RestorePreview, RestoreResult, Secret,
    AUTO_BACKUPS_KEPT,
};
use tauri::State;

use crate::{
    commands::security::candidates,
    error::{app_error, store_error},
    state::AppState,
};

/// The folder backups go to: the one chosen in Settings, else `backups` in the app's data
/// folder. The flag says which.
pub(crate) fn backup_folder(
    conn: &Connection,
    data_dir: &Path,
) -> Result<(PathBuf, bool), AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    Ok(match settings.backup_folder {
        Some(f) => (PathBuf::from(f), false),
        None => (data_dir.join("backups"), true),
    })
}

/// Where backups are, whether the daily one is on, and the backups Minimap made there.
#[tauri::command]
pub async fn get_backup_status(state: State<'_, AppState>) -> Result<BackupStatus, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, _| {
            let (conn, key) = vault.parts()?;
            status_impl(conn, &data_dir, key)
        })
        .await
}

pub(crate) fn status_impl(
    conn: &Connection,
    data_dir: &Path,
    key: &Key,
) -> Result<BackupStatus, AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let (folder, is_default) = backup_folder(conn, data_dir)?;
    let backups = minimap_store::backup::list(&folder).map_err(store_error)?;
    let last_backup = backups
        .iter()
        .find(|b| matches!(b.kind, BackupKind::Manual | BackupKind::Auto))
        .cloned();
    Ok(BackupStatus {
        folder: folder.display().to_string(),
        is_default_folder: is_default,
        default_folder: data_dir.join("backups").display().to_string(),
        auto_backup: settings.auto_backup,
        database_encrypted: !key.is_none(),
        keep_auto: AUTO_BACKUPS_KEPT,
        last_backup,
        backups,
    })
}

/// Copies the database into `path` (a folder; the configured one when omitted) as
/// `minimap-YYYYMMDD-HHMMSS.db`, using SQLite's online backup, so the app keeps working. An
/// encrypted database is backed up encrypted with the same key.
#[tauri::command]
pub async fn backup_now(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<BackupEntry, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, _| {
            let (conn, key) = vault.parts()?;
            backup_now_impl(conn, &data_dir, path.as_deref(), key)
        })
        .await
}

pub(crate) fn backup_now_impl(
    conn: &mut Connection,
    data_dir: &Path,
    folder: Option<&str>,
    key: &Key,
) -> Result<BackupEntry, AppError> {
    let folder = match folder.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => {
            let f = PathBuf::from(f);
            if !f.is_absolute() {
                return Err(app_error(
                    "invalid",
                    "The backup folder must be a full path",
                ));
            }
            f
        }
        None => backup_folder(conn, data_dir)?.0,
    };
    let entry = minimap_store::backup::create(conn, &folder, BackupKind::Manual, 0, key)
        .map_err(store_error)?;
    tracing::info!(bytes = entry.bytes, "backup made");
    Ok(entry)
}

/// Checks a backup file and says what is in it, without changing anything. Refuses files that
/// are damaged, not Minimap's, or from a newer version. A backup encrypted with a different key
/// answers `backup_key_needed`; send its passphrase or recovery key as `secret`.
#[tauri::command]
pub async fn preview_restore(
    state: State<'_, AppState>,
    path: String,
    secret: Option<Secret>,
) -> Result<RestorePreview, AppError> {
    state
        .run_vault(move |vault, _| {
            let (_, key) = vault.parts()?;
            preview_impl(&path, key, secret.as_ref())
        })
        .await
}

pub(crate) fn preview_impl(
    path: &str,
    key: &Key,
    secret: Option<&Secret>,
) -> Result<RestorePreview, AppError> {
    let extra = secret.map(|s| candidates(&s.0)).unwrap_or_default();
    minimap_store::backup::inspect(
        Path::new(path.trim()),
        minimap_store::LATEST_SCHEMA,
        key,
        &extra,
    )
    .map_err(store_error)
}

/// Replaces the live data with a backup. The current data is saved as a backup first, in the
/// backup folder; a refused file changes nothing. The live database keeps its own key.
#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    path: String,
    secret: Option<Secret>,
) -> Result<RestoreResult, AppError> {
    let data_dir = state.data_dir.clone();
    let result = state
        .run_vault(move |vault, _| {
            let (conn, key) = vault.parts()?;
            restore_impl(conn, &data_dir, &path, key, secret.as_ref())
        })
        .await;
    if result.is_ok() {
        // Everything was replaced: undo steps would refer to data that is gone.
        state.forget_undo();
    }
    result
}

pub(crate) fn restore_impl(
    conn: &mut Connection,
    data_dir: &Path,
    path: &str,
    key: &Key,
    secret: Option<&Secret>,
) -> Result<RestoreResult, AppError> {
    let (folder, _) = backup_folder(conn, data_dir)?;
    let extra = secret.map(|s| candidates(&s.0)).unwrap_or_default();
    let result = minimap_store::backup::restore(conn, Path::new(path.trim()), &folder, key, &extra)
        .map_err(store_error)?;
    tracing::info!("data restored from a backup");
    Ok(result)
}

/// The daily backup: when it is on and the newest manual or automatic backup is over 24 hours
/// old (or there is none), makes one and deletes automatic backups beyond the newest 14.
pub(crate) fn auto_backup_impl(
    conn: &mut Connection,
    data_dir: &Path,
    key: &Key,
) -> Result<Option<BackupEntry>, AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    if !settings.auto_backup {
        return Ok(None);
    }
    let (folder, _) = backup_folder(conn, data_dir)?;
    let newest = minimap_store::backup::list(&folder)
        .map_err(store_error)?
        .into_iter()
        .filter(|b| matches!(b.kind, BackupKind::Manual | BackupKind::Auto))
        .filter_map(|b| parse_name(&b.file_name).map(|(_, at)| at))
        .max();
    if !is_due(newest, minimap_store::now()) {
        return Ok(None);
    }
    let entry = minimap_store::backup::create(conn, &folder, BackupKind::Auto, 0, key)
        .map_err(store_error)?;
    let removed = minimap_store::backup::prune_auto(&folder, AUTO_BACKUPS_KEPT as usize)
        .map_err(store_error)?;
    tracing::info!(bytes = entry.bytes, removed, "automatic backup made");
    Ok(Some(entry))
}

/// One round of the automatic backup, for the timer thread in `main`. Does nothing while the
/// database is locked.
pub(crate) fn auto_backup_tick(state: &AppState) {
    let Ok(mut vault) = state.db.lock() else {
        return;
    };
    let Ok((conn, key)) = vault.parts() else {
        return;
    };
    if let Err(e) = auto_backup_impl(conn, &state.data_dir, key) {
        tracing::warn!(error = %e, "automatic backup failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{AssigneeChoice, CreateTask, UpdateSettings};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "minimap-cmd-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn task(conn: &mut Connection, title: &str) {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: None,
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
    }

    fn set(conn: &mut Connection, patch: UpdateSettings) {
        minimap_store::settings::update(conn, patch).unwrap();
    }

    #[test]
    fn backups_go_to_a_default_folder_until_another_is_chosen() {
        let data = temp_dir("folders");
        let mut conn = minimap_store::open(&data.join("minimap.db")).unwrap();
        task(&mut conn, "One");
        let status = status_impl(&conn, &data, &Key::None).unwrap();
        assert!(status.is_default_folder && status.auto_backup);
        assert_eq!(status.folder, data.join("backups").display().to_string());
        assert_eq!(status.folder, status.default_folder);
        assert!(status.backups.is_empty() && status.last_backup.is_none());
        assert_eq!(status.keep_auto, 14);

        let made = backup_now_impl(&mut conn, &data, None, &Key::None).unwrap();
        assert!(made.path.starts_with(&status.folder));
        assert_eq!(made.kind, BackupKind::Manual);
        let status = status_impl(&conn, &data, &Key::None).unwrap();
        assert_eq!(
            status.last_backup.as_ref().map(|b| &b.file_name),
            Some(&made.file_name)
        );

        // The user changes the folder in Settings: new backups go there.
        let elsewhere = temp_dir("elsewhere");
        set(
            &mut conn,
            UpdateSettings {
                backup_folder: Some(elsewhere.display().to_string()),
                ..Default::default()
            },
        );
        let made = backup_now_impl(&mut conn, &data, None, &Key::None).unwrap();
        assert!(made.path.starts_with(&elsewhere.display().to_string()));
        let status = status_impl(&conn, &data, &Key::None).unwrap();
        assert!(!status.is_default_folder);
        assert_eq!(
            status.backups.len(),
            1,
            "the list follows the chosen folder"
        );
        // An empty folder goes back to the default.
        set(
            &mut conn,
            UpdateSettings {
                backup_folder: Some(String::new()),
                ..Default::default()
            },
        );
        assert!(
            status_impl(&conn, &data, &Key::None)
                .unwrap()
                .is_default_folder
        );
        // A folder given to backup_now directly wins for that backup only.
        let one_off = temp_dir("oneoff");
        let made = backup_now_impl(
            &mut conn,
            &data,
            Some(one_off.to_str().unwrap()),
            &Key::None,
        )
        .unwrap();
        assert!(made.path.starts_with(&one_off.display().to_string()));
        assert_eq!(
            backup_now_impl(&mut conn, &data, Some("relative/folder"), &Key::None)
                .unwrap_err()
                .code,
            "invalid"
        );
        for d in [data, elsewhere, one_off] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_relative_backup_folder_is_refused_in_settings() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let e = minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                backup_folder: Some("backups".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("full path"), "{e}");
    }

    #[test]
    fn the_daily_backup_runs_when_due_and_keeps_fourteen() {
        let data = temp_dir("auto");
        let mut conn = minimap_store::open(&data.join("minimap.db")).unwrap();
        task(&mut conn, "One");
        // First start: nothing yet, so one is made.
        let first = auto_backup_impl(&mut conn, &data, &Key::None)
            .unwrap()
            .expect("a backup is due");
        assert_eq!(first.kind, BackupKind::Auto);
        // Straight away it is not due again.
        assert!(auto_backup_impl(&mut conn, &data, &Key::None)
            .unwrap()
            .is_none());
        // A manual backup counts as a recent backup too.
        let folder = data.join("backups");
        // Sixteen old automatic backups (over a day old) make one more due, then pruning.
        for day in 2..18 {
            let name = format!("minimap-auto-202601{day:02}-090000.db");
            std::fs::write(folder.join(name), "x").unwrap();
        }
        std::fs::remove_file(&first.path).unwrap();
        let again = auto_backup_impl(&mut conn, &data, &Key::None)
            .unwrap()
            .expect("old ones are over a day old");
        let autos: Vec<BackupEntry> = status_impl(&conn, &data, &Key::None)
            .unwrap()
            .backups
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto)
            .collect();
        assert_eq!(autos.len(), 14, "only 14 automatic backups are kept");
        assert!(
            autos.iter().any(|b| b.file_name == again.file_name),
            "the new one is among them"
        );
        assert!(
            autos
                .iter()
                .all(|b| b.file_name != "minimap-auto-20260102-090000.db"),
            "oldest went first"
        );

        // Turned off: nothing happens, however old the last one is.
        set(
            &mut conn,
            UpdateSettings {
                auto_backup: Some(false),
                ..Default::default()
            },
        );
        std::fs::remove_file(&again.path).unwrap();
        assert!(auto_backup_impl(&mut conn, &data, &Key::None)
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn a_manual_backup_postpones_the_daily_one() {
        let data = temp_dir("manual");
        let mut conn = minimap_store::open(&data.join("minimap.db")).unwrap();
        backup_now_impl(&mut conn, &data, None, &Key::None).unwrap();
        assert!(auto_backup_impl(&mut conn, &data, &Key::None)
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn restoring_through_the_commands_round_trips_and_refuses_bad_files() {
        let data = temp_dir("restore");
        let mut conn = minimap_store::open(&data.join("minimap.db")).unwrap();
        task(&mut conn, "Keep me");
        let made = backup_now_impl(&mut conn, &data, None, &Key::None).unwrap();
        task(&mut conn, "Added later");
        let tasks = |conn: &Connection| minimap_store::tasks::list(conn, false).unwrap().len();
        assert_eq!(tasks(&conn), 2);

        let preview = preview_impl(&made.path, &Key::None, None).unwrap();
        assert_eq!(
            preview
                .counts
                .iter()
                .find(|c| c.label == "Tasks")
                .unwrap()
                .count,
            1
        );
        assert_eq!(tasks(&conn), 2, "previewing changes nothing");

        let result = restore_impl(&mut conn, &data, &made.path, &Key::None, None).unwrap();
        assert_eq!(tasks(&conn), 1);
        assert_eq!(result.saved_current_as.kind, BackupKind::PreRestore);
        assert!(result
            .saved_current_as
            .path
            .starts_with(&data.join("backups").display().to_string()));
        // The saved copy can be restored to get the later work back.
        restore_impl(
            &mut conn,
            &data,
            &result.saved_current_as.path,
            &Key::None,
            None,
        )
        .unwrap();
        assert_eq!(tasks(&conn), 2);

        let junk = data.join("junk.db");
        std::fs::write(
            &junk,
            "not a database, but long enough to be looked at by sqlite",
        )
        .unwrap();
        // Random bytes look like an encrypted file, so Minimap asks for a key.
        let code = |r: Result<RestorePreview, AppError>| r.unwrap_err().code;
        assert_eq!(
            code(preview_impl(junk.to_str().unwrap(), &Key::None, None)),
            "backup_key_needed"
        );
        assert_eq!(
            restore_impl(&mut conn, &data, junk.to_str().unwrap(), &Key::None, None)
                .unwrap_err()
                .code,
            "backup_key_needed"
        );
        for bad in ["/no/such/file.db", data.to_str().unwrap()] {
            assert_eq!(
                code(preview_impl(bad, &Key::None, None)),
                "invalid",
                "{bad}"
            );
            assert_eq!(
                restore_impl(&mut conn, &data, bad, &Key::None, None)
                    .unwrap_err()
                    .code,
                "invalid",
                "{bad}"
            );
        }
        assert_eq!(tasks(&conn), 2, "refused restores change nothing");
        std::fs::remove_dir_all(data).unwrap();
    }
}
