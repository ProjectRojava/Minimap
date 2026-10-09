//! Google Drive storage and multi-device sync (spec 22): the status the status bar and Settings
//! show, signing in and out, "Sync now", and recovering from a checkpoint. The work itself is in
//! `minimap-sync`; these commands only connect it to the app. Nothing here logs a key or token.

use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use minimap_store::{meta, security::RawKey};
use minimap_sync::{
    drive::http_client,
    oauth::{self, Endpoints},
    Engine, Probe, Remote, SyncError,
};
use minimap_types::{
    AppError, CheckpointInfo, ConnectOutcome, FinishConnect, RecoverCheckpoint, RecoverResult,
    Secret, SyncStatus, UpdateSyncSettings,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::{locked, AppState},
    sync::{self, Pending},
};

/// How long the browser sign-in may take before it is given up.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(crate) fn sync_error(e: SyncError) -> AppError {
    let code = match &e {
        SyncError::Offline(_) => "offline",
        SyncError::Auth(_) => "drive_auth",
        SyncError::Drive(_) => "drive",
        SyncError::WrongKey => "wrong_key",
        SyncError::Corrupt(_) => "corrupt",
        SyncError::NewerData { .. } => "newer_data",
        SyncError::Locked => return locked(),
        SyncError::NotConnected => "not_connected",
        SyncError::Invalid(_) => "invalid",
        SyncError::Io(_) => "io",
        SyncError::Cancelled => "cancelled",
        SyncError::Store(s) => return store_error_ref(s, &e),
    };
    app_error(code, e)
}

fn store_error_ref(s: &minimap_store::StoreError, whole: &SyncError) -> AppError {
    use minimap_store::StoreError;
    let code = match s {
        StoreError::Invalid(_) => "invalid",
        StoreError::WrongKey => "wrong_key",
        StoreError::NewerData { .. } => "newer_data",
        _ => "store",
    };
    app_error(code, whole)
}

fn blocking<T, F>(f: F) -> impl std::future::Future<Output = Result<T, AppError>>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    let handle = tauri::async_runtime::spawn_blocking(f);
    async move { handle.await.map_err(|e| app_error("internal", e))? }
}

/// The status, with what only the app knows (the OAuth client, the dismissed banner).
fn status_of(state: &AppState, conn: Option<&minimap_store::Connection>) -> SyncStatus {
    let (creds, banner) = match conn {
        Some(conn) => {
            let until = meta::get(conn, meta::BANNER_UNTIL).ok().flatten();
            // Past the day it was hidden until, the banner is back.
            let today = minimap_types::timefmt::fmt_date(minimap_store::today());
            let still_hidden = until.filter(|u| u.as_str() >= today.as_str());
            (sync::credentials(conn), still_hidden)
        }
        None => (None, None),
    };
    let mut status = state.sync.engine.status(banner);
    status.client_configured = creds.is_some();
    status.client_built_in = creds.is_some_and(|(_, built_in)| built_in);
    status
}

/// What the status bar and Settings show. Works while the database is locked.
#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, AppError> {
    let app = state.inner().clone();
    state
        .run_vault(move |vault, _| Ok(status_of(&app, vault.conn.as_ref())))
        .await
}

/// Device-local Drive settings: the OAuth client, this device's name, and hiding the
/// "local only" banner for a week.
#[tauri::command]
pub async fn update_sync_settings(
    state: State<'_, AppState>,
    request: UpdateSyncSettings,
) -> Result<SyncStatus, AppError> {
    validate_settings(&request)?;
    let app = state.inner().clone();
    state
        .run(move |conn| apply_settings(&app, conn, &request))
        .await
}

/// Refuses settings that are too long or empty where a value is needed, before anything is
/// stored.
fn validate_settings(request: &UpdateSyncSettings) -> Result<(), AppError> {
    for (what, value) in [
        ("client ID", request.client_id.as_deref()),
        (
            "client secret",
            request.client_secret.as_ref().map(|s| s.0.as_str()),
        ),
    ] {
        if value.is_some_and(|v| v.len() > 500) {
            return Err(app_error("invalid", format!("That {what} is too long")));
        }
    }
    if let Some(name) = request.device_name.as_deref() {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(app_error("invalid", "A device name is 1 to 60 characters"));
        }
    }
    Ok(())
}

fn apply_settings(
    app: &AppState,
    conn: &minimap_store::Connection,
    request: &UpdateSyncSettings,
) -> Result<SyncStatus, AppError> {
    if request.client_id.is_some() || request.client_secret.is_some() {
        // Changing one half keeps the other as it was.
        let current = sync::credentials(conn).filter(|(_, built_in)| !built_in);
        let id = request.client_id.clone().unwrap_or_else(|| {
            current
                .as_ref()
                .map(|(c, _)| c.client_id.clone())
                .unwrap_or_default()
        });
        let secret = request
            .client_secret
            .as_ref()
            .map(|s| s.0.clone())
            .unwrap_or_else(|| current.map(|(c, _)| c.client_secret).unwrap_or_default());
        sync::store_client(conn, &id, &secret).map_err(store_error)?;
    }
    if let Some(name) = request.device_name.as_deref() {
        let name = name.trim();
        meta::set(conn, meta::DEVICE_NAME, name).map_err(store_error)?;
        app.sync.engine.set_device_name(name);
    }
    if request.hide_banner {
        let until = minimap_store::today() + time::Duration::days(7);
        meta::set(
            conn,
            meta::BANNER_UNTIL,
            &minimap_types::timefmt::fmt_date(until),
        )
        .map_err(store_error)?;
    }
    Ok(status_of(app, Some(conn)))
}

/// What a finished browser sign-in found on Drive.
struct SignedIn {
    remote: Arc<minimap_sync::drive::DriveRemote>,
    refresh_token: String,
    account: String,
    probe: Probe,
}

/// Opens the browser to sign in to Google and waits (up to five minutes, or until
/// [`cancel_drive_connect`]). Then:
///
/// * Drive has no Minimap data: this device starts it, and the recovery key is returned to be
///   shown once.
/// * Drive has data and this device already has the key that opens it (it was connected before):
///   joins it.
/// * Drive has data and the recovery key is needed: answers `needs_recovery_key`; the screen asks
///   for it and calls [`finish_drive_connect`].
#[tauri::command]
pub async fn connect_drive(state: State<'_, AppState>) -> Result<ConnectOutcome, AppError> {
    let hub = state.sync.clone();
    if hub.signing_in.swap(true, Ordering::SeqCst) {
        return Err(app_error(
            "state",
            "A sign-in is already open in your browser",
        ));
    }
    hub.cancel.store(false, Ordering::SeqCst);
    let result = connect_inner(&state).await;
    hub.signing_in.store(false, Ordering::SeqCst);
    result
}

async fn connect_inner(state: &AppState) -> Result<ConnectOutcome, AppError> {
    let (creds, _) = state.run(|conn| Ok(sync::credentials(conn))).await?.ok_or_else(|| {
        app_error(
            "no_client",
            "Google Drive needs an OAuth client ID and secret. Add yours under Settings → Google Drive → Advanced",
        )
    })?;
    let cancel = state.sync.cancel.clone();
    let signed = blocking(move || {
        let http = http_client().map_err(sync_error)?;
        let tokens = oauth::sign_in(
            &http,
            &Endpoints::default(),
            &creds,
            &|url| {
                open::that(url).map_err(|e| {
                    SyncError::Io(format!(
                        "Couldn't open your browser ({e}). Open the sign-in page yourself"
                    ))
                })
            },
            &cancel,
            SIGN_IN_TIMEOUT,
        )
        .map_err(sync_error)?;
        let remote = sync::make_remote(
            creds,
            &tokens.refresh_token,
            Some((tokens.access_token.clone(), tokens.expires_in)),
        )
        .map_err(sync_error)?;
        let account = remote.account().map_err(sync_error)?;
        let probe = Engine::probe(&*remote).map_err(sync_error)?;
        Ok(SignedIn {
            remote,
            refresh_token: tokens.refresh_token.clone(),
            account,
            probe,
        })
    })
    .await?;

    match signed.probe {
        Probe::Empty => {
            save_token(state, &signed.refresh_token).await?;
            let engine = state.sync.engine.clone();
            let remote: Arc<dyn Remote> = signed.remote.clone();
            let key = blocking(move || engine.start_new(remote).map_err(sync_error)).await?;
            Ok(ConnectOutcome::Started {
                recovery_key: Secret(key.to_recovery_text().to_string()),
            })
        }
        Probe::Existing { devices } => {
            // Connected before from this device? Then the key that opens Drive is already here.
            let local_key = state
                .run(|conn| meta::vault_key(conn).map_err(store_error))
                .await?;
            if let Some(key) = local_key {
                let remote = signed.remote.clone();
                let checked = {
                    let key = key.clone();
                    blocking(move || {
                        let bytes = remote.read_vault().map_err(sync_error)?;
                        Ok(bytes.is_some_and(|b| minimap_sync::vault::verify(&b, &key).is_ok()))
                    })
                    .await?
                };
                if checked {
                    save_token(state, &signed.refresh_token).await?;
                    return join(state, signed.remote, key).await;
                }
            }
            *state
                .sync
                .pending
                .lock()
                .map_err(|_| app_error("internal", "lock"))? = Some(Pending {
                remote: signed.remote,
                refresh_token: signed.refresh_token,
            });
            Ok(ConnectOutcome::NeedsRecoveryKey {
                account: signed.account,
                devices,
            })
        }
    }
}

async fn save_token(state: &AppState, token: &str) -> Result<(), AppError> {
    let keys = state.keys.clone();
    let token = token.to_owned();
    state
        .run(move |conn| sync::save_token(conn, keys.as_ref(), &token).map_err(store_error))
        .await
}

async fn join(
    state: &AppState,
    remote: Arc<minimap_sync::drive::DriveRemote>,
    key: RawKey,
) -> Result<ConnectOutcome, AppError> {
    let engine = state.sync.engine.clone();
    let remote: Arc<dyn Remote> = remote;
    let outcome = blocking(move || engine.join_existing(remote, key).map_err(sync_error)).await;
    if outcome.is_ok() {
        // This computer adopted, or merged in, another's data: earlier undo steps no longer
        // describe what is here.
        state.forget_undo();
    }
    outcome
}

/// Gives up on a sign-in that is waiting for the browser (or forgets one that is waiting for the
/// recovery key).
#[tauri::command]
pub async fn cancel_drive_connect(state: State<'_, AppState>) -> Result<(), AppError> {
    state.sync.cancel.store(true, Ordering::SeqCst);
    *state
        .sync
        .pending
        .lock()
        .map_err(|_| app_error("internal", "lock"))? = None;
    Ok(())
}

/// The recovery key of the Drive this computer is connected to, for setting up another computer.
/// Only while connected; the key is this computer's own copy of the vault key.
#[tauri::command]
pub async fn get_recovery_key(state: State<'_, AppState>) -> Result<Secret, AppError> {
    let key = state
        .run(|conn| meta::vault_key(conn).map_err(store_error))
        .await?;
    match key {
        Some(key) => Ok(Secret(key.to_recovery_text().to_string())),
        None => Err(app_error(
            "not_connected",
            "This computer has no Drive key. Connect Google Drive first",
        )),
    }
}

/// The second step of connecting to a Drive that already holds data: the recovery key shown when
/// it was started. A wrong key is refused (after a short wait) and nothing changes.
#[tauri::command]
pub async fn finish_drive_connect(
    state: State<'_, AppState>,
    request: FinishConnect,
) -> Result<ConnectOutcome, AppError> {
    let key = RawKey::from_hex(&request.recovery_key.0).ok_or_else(|| {
        app_error(
            "invalid",
            "That doesn't look like a recovery key (64 letters and digits, usually in groups of four)",
        )
    })?;
    let (remote, token) = {
        let guard = state
            .sync
            .pending
            .lock()
            .map_err(|_| app_error("internal", "lock"))?;
        let pending = guard.as_ref().ok_or_else(|| {
            app_error(
                "state",
                "There is no sign-in waiting for a recovery key. Connect again",
            )
        })?;
        (pending.remote.clone(), pending.refresh_token.clone())
    };
    // The token is kept first so a failure half way leaves the device connected, not lost.
    let remote_dyn: Arc<dyn Remote> = remote.clone();
    let checked = {
        let key = key.clone();
        blocking(move || {
            let bytes = remote_dyn
                .read_vault()
                .map_err(sync_error)?
                .ok_or_else(|| {
                    app_error("state", "There is no Minimap data on this Google Drive")
                })?;
            minimap_sync::vault::verify(&bytes, &key).map_err(sync_error)
        })
        .await
    };
    if let Err(e) = checked {
        if e.code == "wrong_key" {
            tokio_free_delay(Duration::from_millis(750)).await;
        }
        return Err(e);
    }
    save_token(&state, &token).await?;
    let outcome = join(&state, remote, key).await?;
    *state
        .sync
        .pending
        .lock()
        .map_err(|_| app_error("internal", "lock"))? = None;
    Ok(outcome)
}

/// A pause that doesn't hold a runtime thread.
async fn tokio_free_delay(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// Signs out of Google Drive: Google is told to forget the sign-in, the token is deleted, and
/// syncing stops. Everything on this device stays; so does what is on Drive.
#[tauri::command]
pub async fn disconnect_drive(state: State<'_, AppState>) -> Result<SyncStatus, AppError> {
    let engine = state.sync.engine.clone();
    blocking(move || {
        engine.disconnect();
        Ok(())
    })
    .await?;
    let keys = state.keys.clone();
    let app = state.inner().clone();
    state
        .run(move |conn| {
            sync::forget_token(conn, keys.as_ref());
            Ok(status_of(&app, Some(conn)))
        })
        .await
}

/// Pulls other devices' changes and saves this device's right now.
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<SyncStatus, AppError> {
    let engine = state.sync.engine.clone();
    let outcome = blocking(move || engine.sync_now().map_err(sync_error)).await;
    let app = state.inner().clone();
    let status = state
        .run_vault(move |vault, _| Ok(status_of(&app, vault.conn.as_ref())))
        .await?;
    outcome.map(|()| status)
}

/// The checkpoints on Drive, newest first (read from Drive, not remembered).
#[tauri::command]
pub async fn list_drive_checkpoints(
    state: State<'_, AppState>,
) -> Result<Vec<CheckpointInfo>, AppError> {
    let engine = state.sync.engine.clone();
    blocking(move || {
        engine.list_checkpoints().map_err(sync_error)?;
        Ok(engine.status(None).checkpoints)
    })
    .await
}

/// Brings back items from a checkpoint (see spec 22): a safety backup is taken first.
#[tauri::command]
pub async fn recover_checkpoint(
    state: State<'_, AppState>,
    request: RecoverCheckpoint,
) -> Result<RecoverResult, AppError> {
    let engine = state.sync.engine.clone();
    let result = blocking(move || engine.recover(&request.name).map_err(sync_error)).await;
    if result.is_ok() {
        // Old copies came back as new edits: earlier undo steps no longer describe the data.
        state.forget_undo();
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{atomic::AtomicU32, Arc};

    use minimap_store::security::Key;

    use super::*;
    use crate::{keystore::memory::MemoryKeyStore, state::Vault};

    static N: AtomicU32 = AtomicU32::new(0);

    pub(crate) fn app(name: &str) -> (AppState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "minimap-cmd-{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = minimap_store::open(&dir.join("minimap.db")).unwrap();
        let state = AppState::new(
            Vault::open(conn, Key::None, None),
            dir.clone(),
            Arc::new(MemoryKeyStore::new()),
        );
        (state, dir)
    }

    fn with_conn<T>(state: &AppState, f: impl FnOnce(&minimap_store::Connection) -> T) -> T {
        let guard = state.db.lock().unwrap();
        f(guard.conn.as_ref().unwrap())
    }

    #[test]
    fn every_kind_of_sync_problem_has_its_own_code() {
        let code = |e: SyncError| sync_error(e).code;
        assert_eq!(code(SyncError::Offline("x".into())), "offline");
        assert_eq!(code(SyncError::Auth("x".into())), "drive_auth");
        assert_eq!(code(SyncError::Drive("x".into())), "drive");
        assert_eq!(code(SyncError::WrongKey), "wrong_key");
        assert_eq!(code(SyncError::Corrupt("x".into())), "corrupt");
        assert_eq!(
            code(SyncError::NewerData {
                device: "d".into(),
                found: 9,
                supported: 8
            }),
            "newer_data"
        );
        assert_eq!(code(SyncError::Locked), "locked");
        assert_eq!(code(SyncError::NotConnected), "not_connected");
        assert_eq!(code(SyncError::Cancelled), "cancelled");
    }

    #[test]
    fn a_new_install_is_local_only_and_says_what_is_missing() {
        let (state, dir) = app("status");
        let status = with_conn(&state, |c| status_of(&state, Some(c)));
        assert_eq!(status.state, minimap_types::SyncState::LocalOnly);
        assert!(!status.connected && !status.client_configured);
        assert!(
            status.summary.contains("not backed up"),
            "{}",
            status.summary
        );
        assert!(status.banner_hidden_until.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_keep_the_client_the_device_name_and_the_banner_choice() {
        let (state, dir) = app("settings");
        let set = |request: UpdateSyncSettings| {
            with_conn(&state, |c| apply_settings(&state, c, &request).unwrap())
        };
        let status = set(UpdateSyncSettings {
            client_id: Some("  my-id.apps.googleusercontent.com ".into()),
            client_secret: Some(Secret("my-secret".into())),
            device_name: Some(" Work laptop ".into()),
            hide_banner: true,
        });
        assert!(status.client_configured && !status.client_built_in);
        assert_eq!(status.device_name, "Work laptop");
        let until = status.banner_hidden_until.clone().unwrap();
        let expect =
            minimap_types::timefmt::fmt_date(minimap_store::today() + time::Duration::days(7));
        assert_eq!(until, expect);
        with_conn(&state, |c| {
            let (creds, built_in) = sync::credentials(c).unwrap();
            assert_eq!(creds.client_id, "my-id.apps.googleusercontent.com");
            assert_eq!(creds.client_secret, "my-secret");
            assert!(!built_in);
        });
        // Changing one half leaves the other.
        set(UpdateSyncSettings {
            client_id: Some("other-id".into()),
            ..Default::default()
        });
        with_conn(&state, |c| {
            let (creds, _) = sync::credentials(c).unwrap();
            assert_eq!(
                (creds.client_id.as_str(), creds.client_secret.as_str()),
                ("other-id", "my-secret")
            );
        });
        // Empty values clear it.
        let cleared = set(UpdateSyncSettings {
            client_id: Some(String::new()),
            client_secret: Some(Secret(String::new())),
            ..Default::default()
        });
        assert!(!cleared.client_configured);
        // A banner hidden until a day that has passed is back.
        with_conn(&state, |c| {
            meta::set(c, meta::BANNER_UNTIL, "2020-01-01").unwrap()
        });
        assert!(with_conn(&state, |c| status_of(&state, Some(c)))
            .banner_hidden_until
            .is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_google_token_lives_in_the_keychain_or_failing_that_the_database() {
        let (state, dir) = app("token");
        with_conn(&state, |c| {
            let keys = MemoryKeyStore::new();
            assert!(sync::load_token(c, &keys).is_none());
            sync::save_token(c, &keys, "refresh-A").unwrap();
            assert_eq!(sync::load_token(c, &keys).as_deref(), Some("refresh-A"));
            assert!(
                meta::get(c, meta::DRIVE_TOKEN).unwrap().is_none(),
                "not in the database"
            );
            sync::forget_token(c, &keys);
            assert!(sync::load_token(c, &keys).is_none());

            // No keychain on this computer: the (possibly encrypted) database holds it.
            let none = MemoryKeyStore::unavailable();
            sync::save_token(c, &none, "refresh-B").unwrap();
            assert_eq!(sync::load_token(c, &none).as_deref(), Some("refresh-B"));
            assert_eq!(
                meta::get(c, meta::DRIVE_TOKEN).unwrap().as_deref(),
                Some("refresh-B")
            );
            sync::forget_token(c, &none);
            assert!(sync::load_token(c, &none).is_none());
        });
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_settings_are_refused_before_anything_is_stored() {
        let too_long = UpdateSyncSettings {
            client_id: Some("x".repeat(501)),
            ..Default::default()
        };
        assert_eq!(validate_settings(&too_long).unwrap_err().code, "invalid");
        let secret = UpdateSyncSettings {
            client_secret: Some(Secret("y".repeat(501))),
            ..Default::default()
        };
        assert!(validate_settings(&secret).is_err());
        for name in ["", "   ", &"n".repeat(61)] {
            let r = UpdateSyncSettings {
                device_name: Some(name.into()),
                ..Default::default()
            };
            assert!(validate_settings(&r).is_err(), "{name:?}");
        }
        let ok = UpdateSyncSettings {
            device_name: Some("Laptop".into()),
            ..Default::default()
        };
        assert!(validate_settings(&ok).is_ok());
        assert!(validate_settings(&UpdateSyncSettings::default()).is_ok());
    }

    #[test]
    fn a_command_that_writes_leaves_changes_to_save_and_a_read_does_not() {
        use minimap_store::tasks;
        use minimap_sync::folder::FolderRemote;
        let (state, dir) = app("tracking");
        let engine = state.sync.engine.clone();
        engine.load().unwrap();
        let remote = Arc::new(FolderRemote::new(dir.join("drive")));
        engine.start_new(remote).unwrap();
        assert!(
            !engine.status(None).unsaved_changes,
            "the first save went up"
        );

        // A read changes nothing.
        let n = tauri::async_runtime::block_on(
            state.run(|conn| Ok(tasks::list(conn, false).map_err(store_error)?.len())),
        )
        .unwrap();
        assert_eq!(n, 0);
        assert!(!engine.status(None).unsaved_changes);

        // A write, however small, does.
        tauri::async_runtime::block_on(state.run(|conn| {
            minimap_store::people::ensure_self(conn, "Me").map_err(store_error)?;
            Ok(())
        }))
        .unwrap();
        assert!(engine.status(None).unsaved_changes);
        // And with Drive disconnected nothing is tracked.
        engine.disconnect();
        assert!(!engine.status(None).unsaved_changes);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
