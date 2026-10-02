use std::sync::Mutex;

use minimap_store::Connection;

pub struct AppState {
    pub db: Mutex<Connection>,
}
