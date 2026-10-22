use std::path::{Path, PathBuf};

use minimap_core::{
    backup::{is_due, parse_name},
    save_points::{age_minutes, merge, TimelineInput},
};
use minimap_store::{security::Key, Connection};
use minimap_types::{
    AppError, BackupEntry, BackupKind, BackupStatus, CheckpointInfo, RestorePreview, RestoreResult,
    SaveSummary, SaveTimeline, Secret, AUTO_BACKUPS_KEPT,
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
/// encrypted database is backed up encrypted with the same key. With a `label` (spec 51) it is a
/// named save point, `minimap-YYYYMMDD-HHMMSS--before-q2-replan.db`; a blank label means none and
/// one that is too long is refused.
#[tauri::command]
pub async fn backup_now(
    state: State<'_, AppState>,
    path: Option<String>,
    label: Option<String>,
) -> Result<BackupEntry, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, _| {
            let (conn, key) = vault.parts()?;
            backup_now_labelled(conn, &data_dir, path.as_deref(), key, label.as_deref())
        })
        .await
}

pub(crate) fn backup_now_impl(
    conn: &mut Connection,
    data_dir: &Path,
    folder: Option<&str>,
    key: &Key,
) -> Result<BackupEntry, AppError> {
    backup_now_labelled(conn, data_dir, folder, key, None)
}

pub(crate) fn backup_now_labelled(
    conn: &mut Connection,
    data_dir: &Path,
    folder: Option<&str>,
    key: &Key,
    label: Option<&str>,
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
    // A blank name is no name.
    let label = label.map(str::trim).filter(|l| !l.is_empty());
    let entry =
        minimap_store::backup::create_with_label(conn, &folder, BackupKind::Manual, 0, key, label)
            .map_err(|e| match e {
                // A name the person typed is refused with its sentence, not "invalid value: ...".
                minimap_store::StoreError::Invalid(message) => app_error("invalid", message),
                other => store_error(other),
            })?;
    tracing::info!(bytes = entry.bytes, "backup made");
    Ok(entry)
}

// ------------------------------------------------------------ save points (spec 51)

/// How long Google Drive gets to answer before the timeline shows this computer's points alone.
const DRIVE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// What Drive said, as the timeline needs it.
pub(crate) struct DriveListing {
    pub connected: bool,
    pub device_id: String,
    pub checkpoints: Vec<CheckpointInfo>,
    /// Said when Drive is connected but could not be listed.
    pub note: Option<String>,
}

impl DriveListing {
    pub(crate) fn not_connected() -> Self {
        Self {
            connected: false,
            device_id: String::new(),
            checkpoints: Vec::new(),
            note: None,
        }
    }
}

/// The note shown when Drive's checkpoints could not be listed.
pub(crate) const DRIVE_NOTE: &str =
    "Couldn't reach Google Drive; showing this computer's save points";

/// Drive's checkpoints. `fetch` asks Drive (with a short wait); otherwise only what the engine
/// remembers is used, with no network at all.
fn drive_listing(
    engine: &std::sync::Arc<minimap_sync::engine::Engine>,
    fetch: bool,
) -> DriveListing {
    let status = engine.status(None);
    if !status.connected {
        return DriveListing::not_connected();
    }
    if !fetch {
        return DriveListing {
            connected: true,
            device_id: status.device_id,
            checkpoints: status.checkpoints,
            note: None,
        };
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let asking = engine.clone();
    std::thread::spawn(move || {
        let _ = tx.send(
            asking
                .list_checkpoints()
                .map(|_| asking.status(None).checkpoints),
        );
    });
    match rx.recv_timeout(DRIVE_WAIT) {
        Ok(Ok(checkpoints)) => DriveListing {
            connected: true,
            device_id: status.device_id,
            checkpoints,
            note: None,
        },
        _ => DriveListing {
            connected: true,
            device_id: status.device_id,
            checkpoints: Vec::new(),
            note: Some(DRIVE_NOTE.to_owned()),
        },
    }
}

/// The backups in the folder and whether the daily one is on.
fn local_points(conn: &Connection, data_dir: &Path) -> Result<(Vec<BackupEntry>, bool), AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let (folder, _) = backup_folder(conn, data_dir)?;
    let backups = minimap_store::backup::list(&folder).map_err(store_error)?;
    Ok((backups, settings.auto_backup))
}

/// The timeline from what was listed. Pure, so the tests give it any combination.
pub(crate) fn timeline_at(
    local: &[BackupEntry],
    auto_backup: bool,
    drive: &DriveListing,
    now: time::OffsetDateTime,
    utc_offset_minutes: i32,
) -> SaveTimeline {
    let mut timeline = merge(&TimelineInput {
        local,
        drive: &drive.checkpoints,
        this_device_id: &drive.device_id,
        now,
        utc_offset_minutes,
        keep_auto: AUTO_BACKUPS_KEPT as usize,
        auto_backup,
        drive_connected: drive.connected,
    });
    timeline.drive_note = drive.note.clone();
    timeline
}

/// Every restorable copy on one timeline, newest first, grouped by the person's own days
/// (`utc_offset_minutes` is the offset of their clock): the backups in the backup folder and,
/// when Google Drive is connected, its checkpoints. Drive gets a few seconds to answer; if it
/// can't, the list is this computer's alone with a note, never an error.
#[tauri::command]
pub async fn list_save_points(
    state: State<'_, AppState>,
    utc_offset_minutes: i32,
) -> Result<SaveTimeline, AppError> {
    let data_dir = state.data_dir.clone();
    let (local, auto) = state
        .run_vault(move |vault, _| {
            let (conn, _) = vault.parts()?;
            local_points(conn, &data_dir)
        })
        .await?;
    let engine = state.sync.engine.clone();
    let drive = tauri::async_runtime::spawn_blocking(move || drive_listing(&engine, true))
        .await
        .map_err(|e| app_error("internal", e))?;
    Ok(timeline_at(
        &local,
        auto,
        &drive,
        minimap_store::now(),
        utc_offset_minutes,
    ))
}

/// The newest save point and whether it is too old, for the status bar. Cheap: the backup
/// folder and what the sync engine already knows of Drive's checkpoints, no network.
#[tauri::command]
pub async fn get_last_save_point(state: State<'_, AppState>) -> Result<SaveSummary, AppError> {
    let data_dir = state.data_dir.clone();
    let (local, auto) = state
        .run_vault(move |vault, _| {
            let (conn, _) = vault.parts()?;
            local_points(conn, &data_dir)
        })
        .await?;
    let drive = drive_listing(&state.sync.engine, false);
    Ok(summary_at(&local, auto, &drive, minimap_store::now()))
}

pub(crate) fn summary_at(
    local: &[BackupEntry],
    auto_backup: bool,
    drive: &DriveListing,
    now: time::OffsetDateTime,
) -> SaveSummary {
    let timeline = timeline_at(local, auto_backup, drive, now, 0);
    SaveSummary {
        age_minutes: timeline.last.as_ref().map(|p| age_minutes(p.at, now)),
        last: timeline.last,
        stale: timeline.stale,
    }
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
        let __p = minimap_store::fixtures::project(conn);
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: Some("build".into()),
                focus: None,
                backburner_since: None,
                start_minute: None,
                length_minutes: None,
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: Some(__p),
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
    #[test]
    fn a_named_save_point_is_made_listed_and_a_bad_name_refused() {
        let data = temp_dir("named");
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Anything");
        let made = backup_now_labelled(
            &mut conn,
            &data,
            Some(data.join("b").to_str().unwrap()),
            &Key::None,
            Some("Before Q2 replan"),
        )
        .unwrap();
        assert!(
            made.file_name.ends_with("--before-q2-replan.db"),
            "{}",
            made.file_name
        );
        assert_eq!(made.label.as_deref(), Some("before-q2-replan"));
        // A blank name is no name; a name that is too long says so in a sentence.
        let blank = backup_now_labelled(
            &mut conn,
            &data,
            Some(data.join("b").to_str().unwrap()),
            &Key::None,
            Some("  "),
        )
        .unwrap();
        assert_eq!(blank.label, None);
        let long = backup_now_labelled(
            &mut conn,
            &data,
            Some(data.join("b").to_str().unwrap()),
            &Key::None,
            Some(&"x".repeat(200)),
        )
        .unwrap_err();
        assert_eq!(long.code, "invalid");
        assert!(
            long.message.contains("at most 80 characters"),
            "{}",
            long.message
        );
        assert!(!long.message.contains("invalid value"));
    }

    #[test]
    fn the_timeline_without_drive_is_the_local_backups_and_with_drive_offline_it_says_so() {
        let data = temp_dir("timeline");
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Anything");
        let folder = data.join("b");
        backup_now_labelled(
            &mut conn,
            &data,
            Some(folder.to_str().unwrap()),
            &Key::None,
            Some("Before Q2 replan"),
        )
        .unwrap();
        let listed = minimap_store::backup::list(&folder).unwrap();
        assert_eq!(listed.len(), 1);
        let now = minimap_store::now();
        let alone = timeline_at(&listed, true, &DriveListing::not_connected(), now, 0);
        assert_eq!(alone.days.iter().map(|d| d.points.len()).sum::<usize>(), 1);
        assert_eq!(alone.drive_note, None);
        assert_eq!(alone.stale, None);
        // Drive connected but unreachable: this computer's points and a note, no failure.
        let offline = DriveListing {
            connected: true,
            device_id: "device-aaaa".into(),
            checkpoints: Vec::new(),
            note: Some(DRIVE_NOTE.to_owned()),
        };
        let t = timeline_at(&listed, true, &offline, now, 0);
        assert_eq!(t.days.len(), 1);
        assert_eq!(
            t.drive_note.as_deref(),
            Some("Couldn't reach Google Drive; showing this computer's save points")
        );
        // With Drive's checkpoints the two kinds interleave.
        let checkpoint = CheckpointInfo {
            name: minimap_core::sync::checkpoint_name("device-aaaa", now),
            device_name: "Laptop".into(),
            created: "now".into(),
            bytes: 10,
        };
        let online = DriveListing {
            connected: true,
            device_id: "device-aaaa".into(),
            checkpoints: vec![checkpoint],
            note: None,
        };
        let t = timeline_at(&listed, true, &online, now, 0);
        assert_eq!(t.days.iter().map(|d| d.points.len()).sum::<usize>(), 2);
        // Nothing for two days: the status bar line turns stale, with a reason.
        let later = now + time::Duration::days(2);
        let summary = summary_at(&listed, false, &DriveListing::not_connected(), later);
        assert_eq!(summary.stale, Some(minimap_types::StaleReason::AutoOff));
        assert!(summary.age_minutes.unwrap() >= 2 * 24 * 60);
        assert!(summary.last.is_some());
        let empty = summary_at(&[], true, &DriveListing::not_connected(), now);
        assert_eq!(empty.stale, Some(minimap_types::StaleReason::AutoFailing));
        assert_eq!(empty.last, None);
    }
}
