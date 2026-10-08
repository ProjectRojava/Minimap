//! App settings (key/value rows). Missing keys fall back to the defaults in `Settings`.

use minimap_types::{Settings, UpdateSettings};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};

const HOURS_PER_DAY: &str = "hours_per_day";
const WORK_WEEK: &str = "work_week";
const DEFAULT_WEEKLY_CAPACITY: &str = "default_weekly_capacity_hours";
const THEME: &str = "theme";
const STALE_WAITING_DAYS: &str = "stale_waiting_days";
const HEALTH: &str = "health";
const CAPACITY_TASK_LIMIT: &str = "capacity_task_limit";
const REPORT_TEMPLATE: &str = "report_template";
const BACKUP_FOLDER: &str = "backup_folder";
const AUTO_BACKUP: &str = "auto_backup";
const TASK_TYPES: &str = "task_types";

/// Settings that belong to this device and are never synced (or put in a snapshot).
pub const LOCAL_ONLY: [&str; 3] = [THEME, BACKUP_FOLDER, AUTO_BACKUP];

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

/// A backup folder is a full path.
fn valid_folder(path: &str) -> bool {
    let p = std::path::Path::new(path);
    !path.trim().is_empty() && path.len() <= 1000 && p.is_absolute()
}

/// A weekly capacity in hours: more than none, no more than the hours in a week.
fn valid_weekly_hours(h: f64) -> bool {
    h.is_finite() && h > 0.0 && h <= 168.0
}

fn read(conn: &Connection, key: &str) -> Result<Option<serde_json::Value>> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(raw.map(|r| serde_json::from_str(&r)).transpose()?)
}

/// The weekly capacity a new person gets (the stored setting, else 40 hours). Cheap enough for
/// `people::create` to read inside its transaction.
pub fn default_weekly_capacity_hours(conn: &Connection) -> Result<f64> {
    Ok(read(conn, DEFAULT_WEEKLY_CAPACITY)?
        .and_then(|v| v.as_f64())
        .filter(|h| valid_weekly_hours(*h))
        .unwrap_or(minimap_types::DEFAULT_WEEKLY_CAPACITY_HOURS))
}

/// The task types (spec 32): the stored list, else the defaults. A stored list that no longer
/// validates falls back to the defaults, like the other structured settings.
pub fn task_types(conn: &Connection) -> Result<Vec<minimap_types::TaskType>> {
    let stored = read(conn, TASK_TYPES)?
        .and_then(|v| serde_json::from_value::<Vec<minimap_types::TaskType>>(v).ok())
        .filter(|list| minimap_core::task_types::validate(list).is_ok());
    Ok(stored.unwrap_or_else(minimap_types::default_task_types))
}

pub fn get(conn: &Connection) -> Result<Settings> {
    let mut s = Settings::default();
    if let Some(h) = read(conn, HOURS_PER_DAY)?.and_then(|v| v.as_f64()) {
        s.hours_per_day = h;
    }
    if let Some(w) = read(conn, WORK_WEEK)?
        .and_then(|v| serde_json::from_value::<minimap_types::WorkWeek>(v).ok())
    {
        s.work_week = w;
    }
    s.default_weekly_capacity_hours = default_weekly_capacity_hours(conn)?;
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
    if let Some(f) = read(conn, BACKUP_FOLDER)?.and_then(|v| v.as_str().map(str::to_owned)) {
        if valid_folder(&f) {
            s.backup_folder = Some(f);
        }
    }
    s.task_types = task_types(conn)?;
    if let Some(on) = read(conn, AUTO_BACKUP)?.and_then(|v| v.as_bool()) {
        s.auto_backup = on;
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
    if let Some(h) = patch.default_weekly_capacity_hours {
        if !valid_weekly_hours(h) {
            return Err(StoreError::Invalid(
                "weekly capacity must be more than 0 and at most 168 hours".into(),
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
    // An empty text goes back to the default folder.
    let folder = patch.backup_folder.as_deref().map(str::trim);
    if let Some(f) = folder.filter(|f| !f.is_empty()) {
        if !valid_folder(f) {
            return Err(StoreError::Invalid(
                "The backup folder must be a full path, like /home/you/Backups or C:\\Backups"
                    .into(),
            ));
        }
    }
    // New types get their ids; removing a type is refused (it would orphan tasks).
    let task_types = match &patch.task_types {
        Some(list) => {
            let current = task_types(conn)?;
            Some(
                minimap_core::task_types::normalise(list.clone(), &current)
                    .map_err(StoreError::Invalid)?,
            )
        }
        None => None,
    };
    let tx = conn.transaction()?;
    if let Some(list) = &task_types {
        write(&tx, TASK_TYPES, &serde_json::to_value(list)?)?;
    }
    match folder {
        Some("") => {
            tx.execute("DELETE FROM settings WHERE key = ?1", [BACKUP_FOLDER])?;
        }
        Some(f) => write(&tx, BACKUP_FOLDER, &serde_json::json!(f))?,
        None => {}
    }
    if let Some(on) = patch.auto_backup {
        write(&tx, AUTO_BACKUP, &serde_json::json!(on))?;
    }
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
    if let Some(w) = patch.work_week {
        write(&tx, WORK_WEEK, &serde_json::to_value(w)?)?;
    }
    if let Some(h) = patch.default_weekly_capacity_hours {
        write(&tx, DEFAULT_WEEKLY_CAPACITY, &serde_json::json!(h))?;
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
