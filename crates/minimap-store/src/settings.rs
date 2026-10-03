//! App settings (key/value rows). Missing keys fall back to the defaults in `Settings`.

use minimap_types::{Settings, UpdateSettings};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};

const HOURS_PER_DAY: &str = "hours_per_day";

fn read(conn: &Connection, key: &str) -> Result<Option<serde_json::Value>> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(raw.map(|r| serde_json::from_str(&r)).transpose()?)
}

pub fn get(conn: &Connection) -> Result<Settings> {
    let mut s = Settings::default();
    if let Some(h) = read(conn, HOURS_PER_DAY)?.and_then(|v| v.as_f64()) {
        s.hours_per_day = h;
    }
    Ok(s)
}

pub fn update(conn: &mut Connection, patch: UpdateSettings) -> Result<Settings> {
    if let Some(h) = patch.hours_per_day {
        if !h.is_finite() || h <= 0.0 || h > 24.0 {
            return Err(StoreError::Invalid(
                "hours per day must be between 0 and 24".into(),
            ));
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![HOURS_PER_DAY, serde_json::to_string(&h)?],
        )?;
    }
    get(conn)
}
