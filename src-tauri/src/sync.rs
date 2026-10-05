//! Wiring for Google Drive sync (spec 22, ADR-0011): what the sync engine sees of the app
//! ([`TauriHost`]), the sign-in the user may be in the middle of, the Google OAuth client and
//! refresh token, and the background thread that keeps the engine ticking.
//!
//! Nothing here logs a token, a key, a file name or note text.

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use minimap_store::{meta, security::Key, Connection};
use minimap_sync::{
    drive::{http_client, ApiBase, DriveRemote},
    oauth::{ClientCredentials, Endpoints, TokenSource},
    Engine, Host, SyncError,
};
use serde::{Deserialize, Serialize};

use crate::{
    keystore::KeyStore,
    state::{AppState, Vault},
};

/// The keychain entry the Google refresh token lives in.
const TOKEN_ACCOUNT: &str = "drive-refresh-token";

/// The engine's window onto the app: the database (with its lock) and the data folder.
pub struct TauriHost {
    pub db: Arc<Mutex<Vault>>,
    pub data_dir: PathBuf,
}

impl Host for TauriHost {
    fn with_db(&self, f: &mut dyn FnMut(&mut Connection, &Key)) -> Result<(), SyncError> {
        let mut guard = self.db.lock().map_err(|_| SyncError::Locked)?;
        let vault: &mut Vault = &mut guard;
        match vault.conn.as_mut() {
            Some(conn) => {
                f(conn, &vault.key);
                Ok(())
            }
            None => Err(SyncError::Locked),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }
}

/// Signed in to Google, but Drive already holds Minimap data that needs the recovery key
/// before anything is merged.
pub struct Pending {
    pub remote: Arc<DriveRemote>,
    pub refresh_token: String,
}

pub struct SyncHub {
    pub engine: Arc<Engine>,
    /// The background thread has loaded the engine's state (the database was unlocked).
    ready: Mutex<bool>,
    pub pending: Mutex<Option<Pending>>,
    /// Set to give up on a sign-in that is waiting for the browser.
    pub cancel: Arc<AtomicBool>,
    pub signing_in: AtomicBool,
}

impl SyncHub {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            ready: Mutex::new(false),
            pending: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
            signing_in: AtomicBool::new(false),
        }
    }
}

// ------------------------------------------------------------------ credentials

#[derive(Serialize, Deserialize)]
struct StoredClient {
    client_id: String,
    client_secret: String,
}

fn built_in_client() -> Option<ClientCredentials> {
    let id = option_env!("MINIMAP_GOOGLE_CLIENT_ID").filter(|s| !s.is_empty())?;
    let secret = option_env!("MINIMAP_GOOGLE_CLIENT_SECRET").filter(|s| !s.is_empty())?;
    Some(ClientCredentials {
        client_id: id.to_owned(),
        client_secret: secret.to_owned(),
    })
}

/// The OAuth client to sign in with: the one the user entered in Settings, else the one built
/// into this version. The flag says it is the built-in one.
pub fn credentials(conn: &Connection) -> Option<(ClientCredentials, bool)> {
    let entered = meta::get(conn, meta::DRIVE_CLIENT)
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str::<StoredClient>(&t).ok())
        .filter(|c| !c.client_id.is_empty() && !c.client_secret.is_empty())
        .map(|c| ClientCredentials {
            client_id: c.client_id,
            client_secret: c.client_secret,
        });
    match entered {
        Some(c) => Some((c, false)),
        None => built_in_client().map(|c| (c, true)),
    }
}

/// Saves (or, with empty values, clears) the OAuth client entered in Settings.
pub fn store_client(
    conn: &Connection,
    id: &str,
    secret: &str,
) -> Result<(), minimap_store::StoreError> {
    if id.trim().is_empty() && secret.trim().is_empty() {
        return meta::remove(conn, meta::DRIVE_CLIENT);
    }
    let text = serde_json::to_string(&StoredClient {
        client_id: id.trim().to_owned(),
        client_secret: secret.trim().to_owned(),
    })?;
    meta::set(conn, meta::DRIVE_CLIENT, &text)
}

// ------------------------------------------------------------------ the refresh token

/// The refresh token: from the OS keychain, or where there is none from the local database
/// (which is encrypted when encryption is on).
pub fn load_token(conn: &Connection, keys: &dyn KeyStore) -> Option<String> {
    if let Ok(Some(t)) = keys.get_secret(TOKEN_ACCOUNT) {
        return Some(t);
    }
    meta::get(conn, meta::DRIVE_TOKEN).ok().flatten()
}

pub fn save_token(
    conn: &Connection,
    keys: &dyn KeyStore,
    token: &str,
) -> Result<(), minimap_store::StoreError> {
    if keys.set_secret(TOKEN_ACCOUNT, token).is_ok() {
        // One place only.
        let _ = meta::remove(conn, meta::DRIVE_TOKEN);
        return Ok(());
    }
    meta::set(conn, meta::DRIVE_TOKEN, token)
}

pub fn forget_token(conn: &Connection, keys: &dyn KeyStore) {
    let _ = keys.delete_secret(TOKEN_ACCOUNT);
    let _ = meta::remove(conn, meta::DRIVE_TOKEN);
}

/// A Drive remote that signs in with `refresh_token`.
pub fn make_remote(
    creds: ClientCredentials,
    refresh_token: &str,
    access: Option<(String, Duration)>,
) -> Result<Arc<DriveRemote>, SyncError> {
    let http = http_client()?;
    let mut tokens = TokenSource::new(
        http.clone(),
        Endpoints::default(),
        creds,
        refresh_token.to_owned(),
    );
    if let Some((token, valid_for)) = access {
        tokens = tokens.with_access(token, valid_for);
    }
    Ok(Arc::new(DriveRemote::new(
        http,
        Arc::new(tokens),
        ApiBase::default(),
    )))
}

// ------------------------------------------------------------------ the background thread

/// A name for this computer in the device list, when the user hasn't chosen one: the host name
/// (GUI apps often don't inherit `HOSTNAME`, so the `hostname` program is asked too).
fn default_device_name() -> String {
    let clean = |s: String| {
        let t = s.trim().to_owned();
        (!t.is_empty() && t.chars().count() <= 60).then_some(t)
    };
    ["HOSTNAME", "COMPUTERNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().and_then(clean))
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| clean(String::from_utf8_lossy(&o.stdout).into_owned()))
        })
        .unwrap_or_else(|| "This computer".to_owned())
}

/// Gets the engine ready once the database can be read: names this device, loads what was saved
/// last time, and reconnects to Drive when a sign-in is stored. Returns whether it is ready.
fn prepare(state: &AppState) -> bool {
    let hub = &state.sync;
    if *hub.ready.lock().unwrap_or_else(|e| e.into_inner()) {
        return true;
    }
    let found = {
        let Ok(mut guard) = state.db.lock() else {
            return false;
        };
        let Some(conn) = guard.conn.as_mut() else {
            return false; // still locked
        };
        if meta::get(conn, meta::DEVICE_NAME).ok().flatten().is_none() {
            let _ = meta::set(conn, meta::DEVICE_NAME, &default_device_name());
        }
        let _ = meta::device_id(conn);
        let token = load_token(conn, state.keys.as_ref());
        let creds = credentials(conn);
        token.zip(creds)
    };
    if hub.engine.load().is_err() {
        return false;
    }
    if let Some((token, (creds, _))) = found {
        match make_remote(creds, &token, None) {
            Ok(remote) => hub.engine.set_remote(Some(remote)),
            Err(e) => tracing::warn!(error = %e, "couldn't set up Google Drive"),
        }
    }
    *hub.ready.lock().unwrap_or_else(|e| e.into_inner()) = true;
    true
}

/// Starts the thread that keeps syncing in the background for the life of the app.
pub fn spawn(state: AppState) -> anyhow::Result<()> {
    use anyhow::Context;
    std::thread::Builder::new()
        .name("drive-sync".into())
        .spawn(move || loop {
            if !prepare(&state) {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            let wait = state.sync.engine.tick();
            std::thread::sleep(wait.min(Duration::from_millis(500)));
        })
        .context("start the Google Drive sync thread")?;
    Ok(())
}
