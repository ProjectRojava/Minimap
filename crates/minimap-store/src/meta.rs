//! Device-local facts kept in `app_meta` under keys that start with `local.` (spec 22): who this
//! device is, the vault key, the Drive sign-in, what was last synced. They are never merged,
//! never uploaded in a snapshot, and are kept across a restore. Everything else in `app_meta`
//! describes the database file itself.

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::{
    error::{Result, StoreError},
    security::RawKey,
};

/// Every device-local key starts with this.
pub const LOCAL_PREFIX: &str = "local.";

pub const DEVICE_ID: &str = "local.device_id";
pub const DEVICE_NAME: &str = "local.device_name";
/// The 256-bit key everything uploaded to Drive (and every cached attachment) is encrypted with.
pub const VAULT_KEY: &str = "local.vault_key";
/// The Google refresh token, only where there is no OS keychain to hold it.
pub const DRIVE_TOKEN: &str = "local.drive_refresh_token";
/// `{ "client_id": ..., "client_secret": ... }` entered in Settings.
pub const DRIVE_CLIENT: &str = "local.drive_client";
/// JSON written by the sync engine: what was merged from each device, the last save, ...
pub const SYNC_STATE: &str = "local.sync_state";
/// Until when the "local only" banner is hidden (`YYYY-MM-DD`).
pub const BANNER_UNTIL: &str = "local.banner_until";
/// JSON list of the items the demo data added on this computer, so "Remove demo data" removes
/// exactly those (spec 24).
pub const DEMO_ITEMS: &str = "local.demo_items";
/// Present only while a merge is applying another device's rows; the triggers that stamp
/// `updated_at` look for it and stand down.
pub(crate) const MERGING: &str = "local.merging";

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM app_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    if !key.starts_with(LOCAL_PREFIX) {
        return Err(StoreError::Invalid(format!(
            "{key} is not a device-local key"
        )));
    }
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn remove(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM app_meta WHERE key = ?1", [key])?;
    Ok(())
}

/// This device's id, made on first use (a lowercase uuid; it ends up in file names on Drive).
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = get(conn, DEVICE_ID)? {
        return Ok(id);
    }
    let id = Uuid::now_v7().to_string();
    set(conn, DEVICE_ID, &id)?;
    Ok(id)
}

/// What this device is called in the device list. Defaults to "This computer".
pub fn device_name(conn: &Connection) -> Result<String> {
    Ok(get(conn, DEVICE_NAME)?.unwrap_or_else(|| "This computer".to_owned()))
}

/// The vault key, if one has been made.
pub fn vault_key(conn: &Connection) -> Result<Option<RawKey>> {
    Ok(get(conn, VAULT_KEY)?.and_then(|hex| RawKey::from_hex(&hex)))
}

/// The vault key, made (from the OS's secure random source) the first time it is needed.
pub fn ensure_vault_key(conn: &Connection) -> Result<RawKey> {
    if let Some(key) = vault_key(conn)? {
        return Ok(key);
    }
    let key = RawKey::generate()?;
    set(conn, VAULT_KEY, key.to_hex().as_str())?;
    Ok(key)
}

/// Replaces the vault key (a second device adopting the one shown as the recovery key).
pub fn set_vault_key(conn: &Connection, key: &RawKey) -> Result<()> {
    set(conn, VAULT_KEY, key.to_hex().as_str())
}

/// Every device-local row, to carry across a restore (the backup's own would be wrong here).
pub fn local_rows(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT key, value FROM app_meta WHERE key LIKE 'local.%' AND key != 'local.merging'",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Puts device-local rows back after a restore replaced `app_meta` wholesale.
pub fn put_local_rows(conn: &Connection, rows: &[(String, String)]) -> Result<()> {
    conn.execute("DELETE FROM app_meta WHERE key LIKE 'local.%'", [])?;
    for (k, v) in rows {
        set(conn, k, v)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_gets_one_stable_id_and_a_name() {
        let conn = crate::open_in_memory().unwrap();
        let a = device_id(&conn).unwrap();
        assert_eq!(a, device_id(&conn).unwrap());
        assert!(minimap_core::sync::is_valid_device_id(&a), "{a}");
        assert_eq!(device_name(&conn).unwrap(), "This computer");
        set(&conn, DEVICE_NAME, "Laptop").unwrap();
        assert_eq!(device_name(&conn).unwrap(), "Laptop");
    }

    #[test]
    fn the_vault_key_is_made_once_and_can_be_replaced() {
        let conn = crate::open_in_memory().unwrap();
        assert!(vault_key(&conn).unwrap().is_none());
        let first = ensure_vault_key(&conn).unwrap();
        assert!(first == ensure_vault_key(&conn).unwrap());
        let other = RawKey::generate().unwrap();
        set_vault_key(&conn, &other).unwrap();
        assert!(vault_key(&conn).unwrap().unwrap() == other);
    }

    #[test]
    fn only_local_keys_can_be_written_and_they_survive_a_round_trip() {
        let conn = crate::open_in_memory().unwrap();
        assert!(set(&conn, "created_by", "x").is_err());
        set(&conn, DEVICE_NAME, "Desk").unwrap();
        let rows = local_rows(&conn).unwrap();
        conn.execute("DELETE FROM app_meta WHERE key LIKE 'local.%'", [])
            .unwrap();
        assert_eq!(get(&conn, DEVICE_NAME).unwrap(), None);
        put_local_rows(&conn, &rows).unwrap();
        assert_eq!(get(&conn, DEVICE_NAME).unwrap().as_deref(), Some("Desk"));
        // `created_by` is untouched.
        assert_eq!(
            get(&conn, "created_by").unwrap().as_deref(),
            Some("minimap")
        );
    }
}
