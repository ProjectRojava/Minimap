use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use minimap_store::{security::Key, Connection};
use minimap_types::{AppError, KeyMethod};

use crate::{
    error::app_error,
    keystore::KeyStore,
    sync::{SyncHub, TauriHost},
};
use minimap_sync::Engine;

/// The open database and how it is keyed. `conn` is `None` while an encrypted database waits for
/// its key (the app shows the unlock screen and every other command answers `locked`).
pub struct Vault {
    pub conn: Option<Connection>,
    pub key: Key,
    /// How the key is kept; `None` when plain, locked, or opened with a recovery key and not
    /// stored anywhere.
    pub method: Option<KeyMethod>,
}

impl Vault {
    pub fn open(conn: Connection, key: Key, method: Option<KeyMethod>) -> Self {
        Self {
            conn: Some(conn),
            key,
            method,
        }
    }

    pub fn locked() -> Self {
        Self {
            conn: None,
            key: Key::None,
            method: None,
        }
    }

    /// The connection and its key, or the `locked` error.
    pub fn parts(&mut self) -> Result<(&mut Connection, &Key), AppError> {
        match self.conn.as_mut() {
            Some(conn) => Ok((conn, &self.key)),
            None => Err(locked()),
        }
    }
}

pub fn locked() -> AppError {
    app_error(
        "locked",
        "The database is encrypted and locked. Enter your passphrase or recovery key to open it",
    )
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Vault>>,
    /// The app's data folder (the database, the log and the default backup folder live here).
    pub data_dir: PathBuf,
    pub keys: Arc<dyn KeyStore>,
    /// Google Drive sync (spec 22): the engine and the sign-in in progress.
    pub sync: Arc<SyncHub>,
}

impl AppState {
    pub fn new(vault: Vault, data_dir: PathBuf, keys: Arc<dyn KeyStore>) -> Self {
        let db = Arc::new(Mutex::new(vault));
        let host = Arc::new(TauriHost {
            db: db.clone(),
            data_dir: data_dir.clone(),
        });
        Self {
            db,
            data_dir,
            keys,
            sync: Arc::new(SyncHub::new(Engine::new(host))),
        }
    }

    /// Runs blocking database work off the async runtime. Answers `locked` while the database
    /// is waiting for its key.
    pub async fn run<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, AppError> + Send + 'static,
    {
        self.run_vault(move |vault, _| f(vault.parts()?.0)).await
    }

    /// Like [`run`](Self::run) but with the whole vault (key included) and the keychain, and it
    /// works while locked.
    pub async fn run_vault<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Vault, &dyn KeyStore) -> Result<T, AppError> + Send + 'static,
    {
        let db = self.db.clone();
        let keys = self.keys.clone();
        let engine = self.sync.engine.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut vault = db
                .lock()
                .map_err(|_| app_error("internal", "database lock poisoned"))?;
            let before = vault.conn.as_ref().map(Connection::total_changes);
            let result = f(&mut vault, keys.as_ref());
            // Any command that wrote (even one that then failed half way) leaves changes to
            // save to Drive. The count is per connection, so a swapped connection counts too.
            let after = vault.conn.as_ref().map(Connection::total_changes);
            if before != after {
                if let Some(conn) = vault.conn.as_ref() {
                    engine.note_change(conn);
                }
            }
            result
        })
        .await
        .map_err(|e| app_error("internal", e))?
    }
}
