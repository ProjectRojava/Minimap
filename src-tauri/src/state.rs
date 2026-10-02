use std::sync::{Arc, Mutex};

use minimap_store::Connection;
use minimap_types::AppError;

use crate::error::app_error;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        Self {
            db: Arc::new(Mutex::new(conn)),
        }
    }

    /// Runs blocking database work off the async runtime.
    pub async fn run<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, AppError> + Send + 'static,
    {
        let db = self.db.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut conn = db
                .lock()
                .map_err(|_| app_error("internal", "database lock poisoned"))?;
            f(&mut conn)
        })
        .await
        .map_err(|e| app_error("internal", e))?
    }
}
