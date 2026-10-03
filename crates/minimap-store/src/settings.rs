//! App settings (key/value rows). Missing keys fall back to the defaults in `Settings`.

use minimap_types::{Settings, UpdateSettings};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};

const HOURS_PER_DAY: &str = "hours_per_day";
const THEME: &str = "theme";

fn write(conn: &Connection, key: &str, value: &serde_json::Value) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, serde_json::to_string(value)?],
    )?;
    Ok(())
}

/// A theme id: lowercase letters, digits and hyphens (e.g. `catppuccin-mocha`, `system`).
fn valid_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

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
    if let Some(t) = read(conn, THEME)?.and_then(|v| v.as_str().map(str::to_owned)) {
        if valid_theme_id(&t) {
            s.theme = t;
        }
    }
    Ok(s)
}

pub fn update(conn: &mut Connection, patch: UpdateSettings) -> Result<Settings> {
    // Validate everything first so a bad value changes nothing.
    if let Some(h) = patch.hours_per_day {
        if !h.is_finite() || h <= 0.0 || h > 24.0 {
            return Err(StoreError::Invalid(
                "hours per day must be between 0 and 24".into(),
            ));
        }
    }
    if let Some(t) = &patch.theme {
        if !valid_theme_id(t) {
            return Err(StoreError::Invalid(
                "a theme id uses lowercase letters, digits and hyphens".into(),
            ));
        }
    }
    let tx = conn.transaction()?;
    if let Some(h) = patch.hours_per_day {
        write(&tx, HOURS_PER_DAY, &serde_json::json!(h))?;
    }
    if let Some(t) = patch.theme {
        write(&tx, THEME, &serde_json::json!(t))?;
    }
    tx.commit()?;
    get(conn)
}
