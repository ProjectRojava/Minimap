//! The file that goes to Google Drive (spec 22): a copy of the database keyed with the vault key
//! and stripped of everything that belongs to this device; and the other direction, opening a
//! copy another device saved so it can be merged.

use std::path::Path;

use rusqlite::Connection;

use crate::{
    error::{Result, StoreError},
    meta,
    security::{self, Key, RawKey},
    settings::LOCAL_ONLY,
};

/// Writes a snapshot of `conn` to `dest`: an export into a new file keyed with `vault` (the
/// pages are never written unencrypted; SQLCipher's backup API can't copy a plain database into
/// an encrypted file, and the live one is plain unless encryption is on), then everything device-local is removed from the
/// copy (this device's id, the vault key itself, sign-in tokens, per-device settings) and the
/// file is compacted so none of it lingers in free pages.
pub fn create(conn: &Connection, dest: &Path, vault: &RawKey) -> Result<()> {
    let key = Key::Raw(vault.clone());
    let _ = std::fs::remove_file(dest);
    if let Err(e) = security::build_copy(conn, dest, &key).and_then(|()| scrub(dest, &key)) {
        security::secure_remove(dest);
        return Err(e);
    }
    security::restrict_permissions(dest);
    Ok(())
}

fn scrub(dest: &Path, key: &Key) -> Result<()> {
    let copy = security::connect(dest, key)?;
    let only: Vec<String> = LOCAL_ONLY.iter().map(|k| format!("'{k}'")).collect();
    // The merge flag keeps the "deleted a setting" trigger from leaving tombstones behind.
    copy.execute_batch(&format!(
        "INSERT OR REPLACE INTO app_meta (key, value) VALUES ('{merging}', '1');
         DELETE FROM settings WHERE key IN ({only});
         DELETE FROM app_meta WHERE key LIKE 'local.%';
         VACUUM;",
        merging = meta::MERGING,
        only = only.join(", ")
    ))?;
    let check: String = copy.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(StoreError::Invalid(format!(
            "The snapshot failed its check: {check}"
        )));
    }
    Ok(())
}

/// Opens a snapshot another device saved, ready to be merged. `path` must be a copy this
/// process owns: a snapshot from an older version is upgraded in place. A wrong key is
/// `WrongKey`; a file that isn't Minimap's is `Invalid`; one from a newer version is
/// `NewerData` (the sync engine tells the user to update).
pub fn open_remote(path: &Path, vault: &RawKey) -> Result<Connection> {
    let mut conn = security::connect(path, &Key::Raw(vault.clone()))?;
    let version = crate::schema_version(&conn)?;
    let made_by: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'created_by'",
            [],
            |r| r.get(0),
        )
        .ok();
    if version == 0 || made_by.as_deref() != Some("minimap") {
        return Err(StoreError::Invalid(
            "That file isn't a Minimap snapshot".into(),
        ));
    }
    if version > crate::LATEST_SCHEMA {
        return Err(StoreError::NewerData {
            found: version,
            supported: crate::LATEST_SCHEMA,
        });
    }
    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(StoreError::Invalid(format!(
            "That snapshot is damaged (integrity check: {check})"
        )));
    }
    if version < crate::LATEST_SCHEMA {
        crate::upgrade(&mut conn)?;
    }
    Ok(conn)
}

/// Does this database hold nothing of the user's own: no items, links or attachments, apart
/// from the "me" that first run creates? A device like that simply adopts what is on Drive.
pub fn is_pristine(conn: &Connection) -> Result<bool> {
    for table in [
        "objectives",
        "projects",
        "tasks",
        "teams",
        "notes",
        "decisions",
        "waiting_on",
        "edges",
        "attachments",
    ] {
        let any: bool =
            conn.query_row(&format!("SELECT EXISTS (SELECT 1 FROM {table})"), [], |r| {
                r.get(0)
            })?;
        if any {
            return Ok(false);
        }
    }
    let others: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM people WHERE is_self = 0)",
        [],
        |r| r.get(0),
    )?;
    Ok(!others)
}

/// How many active items there are, for "is it worth a safety backup" decisions and summaries.
pub fn item_count(conn: &Connection) -> Result<u64> {
    let mut total = 0u64;
    for table in [
        "objectives",
        "projects",
        "tasks",
        "people",
        "teams",
        "notes",
        "decisions",
        "waiting_on",
    ] {
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE archived_at IS NULL"),
            [],
            |r| r.get(0),
        )?;
        total += u64::try_from(n).unwrap_or(0);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::{merge, MergeMode};
    use crate::test_support::{add_person, add_task, temp_dir};
    use crate::{settings, tasks};
    use minimap_types::UpdateSettings;

    fn data(conn: &mut Connection) {
        add_person(conn, "Priya");
        add_task(conn, "Ship it");
        settings::update(
            conn,
            UpdateSettings {
                hours_per_day: Some(6.0),
                theme: Some("nord".into()),
                backup_folder: Some("/tmp/somewhere".into()),
                ..Default::default()
            },
        )
        .unwrap();
        meta::set(conn, meta::DEVICE_NAME, "Laptop").unwrap();
        meta::set(conn, meta::DRIVE_TOKEN, "refresh-token-text").unwrap();
    }

    #[test]
    fn a_snapshot_is_encrypted_holds_the_data_and_nothing_of_this_device() {
        let dir = temp_dir("snapshot");
        let mut conn = crate::open_in_memory().unwrap();
        data(&mut conn);
        let vault = meta::ensure_vault_key(&conn).unwrap();
        let path = dir.join("snap.db.enc");
        create(&conn, &path, &vault).unwrap();

        // Not a readable SQLite file, and none of the plain text is in it.
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"));
        for secret in ["Priya", "Ship it", "refresh-token-text", "Laptop"] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{secret} is readable in the snapshot"
            );
        }
        assert!(
            !bytes.windows(64).any(|w| w == vault.to_hex().as_bytes()),
            "the vault key is not inside its own snapshot"
        );

        // With the key: the data is there, the device's own facts are not.
        let opened = open_remote(&path, &vault).unwrap();
        assert_eq!(tasks::list(&opened, false).unwrap().len(), 1);
        assert_eq!(settings::get(&opened).unwrap().hours_per_day, 6.0);
        assert_eq!(settings::get(&opened).unwrap().backup_folder, None);
        assert_ne!(settings::get(&opened).unwrap().theme, "nord");
        assert!(meta::local_rows(&opened).unwrap().is_empty());
        let tombstones: i64 = opened
            .query_row("SELECT COUNT(*) FROM tombstones", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            tombstones, 0,
            "scrubbing left no tombstones for per-device settings"
        );
        drop(opened);

        // Without it, or with another: refused.
        let other = RawKey::generate().unwrap();
        assert!(matches!(
            open_remote(&path, &other),
            Err(StoreError::WrongKey)
        ));
        // The live database kept everything of its own.
        assert_eq!(
            meta::get(&conn, meta::DRIVE_TOKEN).unwrap().as_deref(),
            Some("refresh-token-text")
        );
        assert_eq!(settings::get(&conn).unwrap().theme, "nord");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_snapshot_merges_into_another_device() {
        let dir = temp_dir("snapshot-merge");
        let mut a = crate::open_in_memory().unwrap();
        data(&mut a);
        let mut b = crate::open_in_memory().unwrap();
        let vault = meta::ensure_vault_key(&a).unwrap();
        let path = dir.join("a.db.enc");
        create(&a, &path, &vault).unwrap();
        let remote = open_remote(&path, &vault).unwrap();
        let summary = merge(&mut b, &remote, "Laptop", MergeMode::Apply).unwrap();
        assert!(summary.added >= 2, "{summary:?}");
        assert_eq!(tasks::list(&b, false).unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn data_from_a_newer_version_is_refused_and_older_data_is_upgraded() {
        let dir = temp_dir("snapshot-versions");
        let conn = crate::open_in_memory().unwrap();
        let vault = RawKey::generate().unwrap();
        let newer = dir.join("newer.db.enc");
        create(&conn, &newer, &vault).unwrap();
        security::connect(&newer, &Key::Raw(vault.clone()))
            .unwrap()
            .pragma_update(None, "user_version", crate::LATEST_SCHEMA + 1)
            .unwrap();
        assert!(matches!(
            open_remote(&newer, &vault),
            Err(StoreError::NewerData { .. })
        ));

        // A snapshot from a version that predates sync (schema 5) is upgraded when opened.
        let mut old = Connection::open_in_memory().unwrap();
        crate::register_functions(&old).unwrap();
        crate::migrations().to_version(&mut old, 5).unwrap();
        old.execute(
            "INSERT INTO tasks (id, title, status, priority, created_at, updated_at)
             VALUES ('01a10732-b318-740e-a2ed-5ecf4225916c', 'Old task', 'todo', 3, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        let older = dir.join("older.db.enc");
        create(&old, &older, &vault).unwrap();
        let opened = open_remote(&older, &vault).unwrap();
        assert_eq!(
            crate::schema_version(&opened).unwrap(),
            crate::LATEST_SCHEMA
        );
        let mut target = crate::open_in_memory().unwrap();
        let summary = merge(&mut target, &opened, "Old laptop", MergeMode::Apply).unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(tasks::list(&target, false).unwrap()[0].title, "Old task");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_is_not_a_snapshot_is_refused() {
        let dir = temp_dir("snapshot-foreign");
        let vault = RawKey::generate().unwrap();
        let foreign = dir.join("foreign.db.enc");
        {
            let c = security::connect(&foreign, &Key::Raw(vault.clone())).unwrap();
            c.execute_batch("CREATE TABLE t(x); PRAGMA user_version = 3;")
                .unwrap();
        }
        assert!(matches!(
            open_remote(&foreign, &vault),
            Err(StoreError::Invalid(_))
        ));
        std::fs::write(dir.join("junk"), b"not a database at all, just bytes").unwrap();
        assert!(open_remote(&dir.join("junk"), &vault).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_new_device_is_pristine_until_it_holds_something() {
        let mut conn = crate::open_in_memory().unwrap();
        assert!(is_pristine(&conn).unwrap());
        crate::people::ensure_self(&mut conn, "Me").unwrap();
        assert!(
            is_pristine(&conn).unwrap(),
            "first run's \"me\" doesn't count"
        );
        add_task(&mut conn, "Something");
        assert!(!is_pristine(&conn).unwrap());
        assert_eq!(item_count(&conn).unwrap(), 2);
        let mut other = crate::open_in_memory().unwrap();
        add_person(&mut other, "Priya");
        assert!(!is_pristine(&other).unwrap());
    }
}
