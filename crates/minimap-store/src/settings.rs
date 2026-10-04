//! App settings (key/value rows). Missing keys fall back to the defaults in `Settings`.

use minimap_types::{Settings, UpdateSettings};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};

const HOURS_PER_DAY: &str = "hours_per_day";
const THEME: &str = "theme";
const STALE_WAITING_DAYS: &str = "stale_waiting_days";
const HEALTH: &str = "health";
const CAPACITY_TASK_LIMIT: &str = "capacity_task_limit";
const REPORT_TEMPLATE: &str = "report_template";

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
    if let Some(d) = read(conn, STALE_WAITING_DAYS)?
        .and_then(|v| v.as_u64())
        .and_then(|d| u32::try_from(d).ok())
    {
        if (1..=365).contains(&d) {
            s.stale_waiting_days = d;
        }
    }
    if let Some(n) = read(conn, CAPACITY_TASK_LIMIT)?
        .and_then(|v| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
    {
        if (1..=500).contains(&n) {
            s.capacity_task_limit = n;
        }
    }
    if let Some(h) = read(conn, HEALTH)?
        .and_then(|v| serde_json::from_value::<minimap_types::HealthThresholds>(v).ok())
    {
        // A stored value that is no longer valid falls back to the defaults.
        if h.validate().is_ok() {
            s.health = h;
        }
    }
    if let Some(t) = read(conn, REPORT_TEMPLATE)?.and_then(|v| v.as_str().map(str::to_owned)) {
        // A stored template that is no longer valid falls back to the built-in one.
        if minimap_core::report::validate_template(&t).is_ok() {
            s.report_template = t;
        }
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
    if let Some(d) = patch.stale_waiting_days {
        if !(1..=365).contains(&d) {
            return Err(StoreError::Invalid(
                "stale after must be between 1 and 365 days".into(),
            ));
        }
    }
    if let Some(h) = &patch.health {
        h.validate().map_err(StoreError::Invalid)?;
    }
    if let Some(n) = patch.capacity_task_limit {
        if !(1..=500).contains(&n) {
            return Err(StoreError::Invalid(
                "the open-task limit must be between 1 and 500".into(),
            ));
        }
    }
    // An empty text restores the built-in template.
    let template = patch.report_template.as_deref().map(str::trim);
    if let Some(t) = template.filter(|t| !t.is_empty()) {
        minimap_core::report::validate_template(t).map_err(StoreError::Invalid)?;
    }
    let tx = conn.transaction()?;
    match (template, &patch.report_template) {
        (Some(""), _) => {
            tx.execute("DELETE FROM settings WHERE key = ?1", [REPORT_TEMPLATE])?;
        }
        (Some(_), Some(text)) => write(&tx, REPORT_TEMPLATE, &serde_json::json!(text))?,
        _ => {}
    }
    if let Some(h) = patch.health {
        write(&tx, HEALTH, &serde_json::to_value(h)?)?;
    }
    if let Some(n) = patch.capacity_task_limit {
        write(&tx, CAPACITY_TASK_LIMIT, &serde_json::json!(n))?;
    }
    if let Some(h) = patch.hours_per_day {
        write(&tx, HOURS_PER_DAY, &serde_json::json!(h))?;
    }
    if let Some(t) = patch.theme {
        write(&tx, THEME, &serde_json::json!(t))?;
    }
    if let Some(d) = patch.stale_waiting_days {
        write(&tx, STALE_WAITING_DAYS, &serde_json::json!(d))?;
    }
    tx.commit()?;
    get(conn)
}
