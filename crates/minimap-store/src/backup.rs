//! Local backups (spec 20): SQLite's online backup API into a folder, listing, retention,
//! checking a file before it is restored, and restoring.
//!
//! Backups of an encrypted database are encrypted with the same key (the copy is keyed before
//! the pages are written; SQLCipher refuses to write an encrypted database into a plain file).
//! Restoring a backup whose encryption differs from the live database's goes through
//! `security::replace_live`, which re-keys the copy as it comes in.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use minimap_core::backup::{display_time, file_name, parse_name, prune_plan};
use minimap_types::{
    BackupCount, BackupEntry, BackupKind, RestorePreview, RestoreResult, UpdateSettings,
};
use rusqlite::{backup::Backup, Connection, OpenFlags};
use time::OffsetDateTime;

use crate::{
    convert::now,
    error::{Result, StoreError},
    security::{self, FileState, Key},
    settings,
};

/// Pages copied per step of the online backup.
const PAGES_PER_STEP: i32 = 256;

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

fn io(context: &str, e: std::io::Error) -> StoreError {
    StoreError::Invalid(format!("{context}: {e}"))
}

fn entry_for(path: &Path, kind: BackupKind, at: OffsetDateTime, bytes: u64) -> BackupEntry {
    let age = (now() - at).whole_minutes().max(0) as u64;
    BackupEntry {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
        kind,
        created: display_time(at),
        age_minutes: age,
        bytes,
        encrypted: security::file_is_encrypted(path),
    }
}

/// Copies the live database into `dir` as a new backup file of `kind` and returns it. The copy
/// is written under a temporary name and renamed once it has been checked, so a half-written
/// file never looks like a backup. `key` is how the live database is keyed (the copy gets the
/// same); `schema_version` is only used in pre-migration names.
pub fn create(
    conn: &Connection,
    dir: &Path,
    kind: BackupKind,
    schema_version: u32,
    key: &Key,
) -> Result<BackupEntry> {
    fs::create_dir_all(dir).map_err(|e| {
        io(
            &format!("Couldn't create the backup folder {}", dir.display()),
            e,
        )
    })?;
    // A second backup in the same second gets the next free second.
    let mut at = now().replace_nanosecond(0).unwrap_or_else(|_| now());
    let mut target = dir.join(file_name(kind, at, schema_version));
    while target.exists() {
        at += time::Duration::seconds(1);
        target = dir.join(file_name(kind, at, schema_version));
    }
    let partial = target.with_extension("db.part");
    let _ = fs::remove_file(&partial);
    let copied = copy_database(conn, &partial, key);
    if let Err(e) = copied {
        security::secure_remove(&partial);
        return Err(e);
    }
    fs::rename(&partial, &target)
        .map_err(|e| io(&format!("Couldn't save the backup {}", target.display()), e))?;
    security::restrict_permissions(&target);
    let bytes = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    Ok(entry_for(&target, kind, at, bytes))
}

fn copy_database(conn: &Connection, to: &Path, key: &Key) -> Result<()> {
    let mut dest = Connection::open(to)?;
    security::apply(&dest, key)?;
    {
        let backup = Backup::new(conn, &mut dest)?;
        backup.run_to_completion(PAGES_PER_STEP, Duration::from_millis(0), None)?;
    }
    // One self-contained file: no -wal/-shm beside it.
    dest.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))?;
    let check: String = dest.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(invalid(format!(
            "The backup copy failed its check: {check}"
        )));
    }
    Ok(())
}

/// Minimap's backups in `dir`, newest first. A missing folder is an empty list; files Minimap
/// did not name are ignored.
pub fn list(dir: &Path) -> Result<Vec<BackupEntry>> {
    let read = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(io(
                &format!("Couldn't read the backup folder {}", dir.display()),
                e,
            ))
        }
    };
    let mut found: Vec<(OffsetDateTime, BackupEntry)> = Vec::new();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        let Some((kind, at)) = parse_name(&name) else {
            continue;
        };
        let Ok(meta) = item.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        found.push((at, entry_for(&item.path(), kind, at, meta.len())));
    }
    found.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.file_name.cmp(&a.1.file_name))
    });
    Ok(found.into_iter().map(|(_, e)| e).collect())
}

/// Deletes automatic backups beyond the newest `keep`; returns how many went.
pub fn prune_auto(dir: &Path, keep: usize) -> Result<u32> {
    let listed: Vec<(String, BackupKind, OffsetDateTime)> = list(dir)?
        .into_iter()
        .filter_map(|e| parse_name(&e.file_name).map(|(k, at)| (e.file_name, k, at)))
        .collect();
    let mut removed = 0;
    for name in prune_plan(&listed, keep) {
        // Only ever a file Minimap named, inside the backup folder.
        if fs::remove_file(dir.join(&name)).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// What each kind of item is called when a backup is described.
const COUNTED: [(&str, &str); 8] = [
    ("Objectives", "objectives"),
    ("Projects", "projects"),
    ("Tasks", "tasks"),
    ("People", "people"),
    ("Teams", "teams"),
    ("Notes", "notes"),
    ("Decisions", "decisions"),
    ("Waiting on", "waiting_on"),
];

/// Opens a backup for reading with whichever key opens it. A plain file needs none; an
/// encrypted one is tried with the live key, then the `extra` key the user supplied. Without
/// a working key: `BackupKeyNeeded` when none was supplied, `WrongKey` when one was.
fn open_backup(
    path: &Path,
    live: &Key,
    extra: &[Key],
    read_write: bool,
) -> Result<(Connection, Key, fs::Metadata)> {
    let meta =
        fs::metadata(path).map_err(|_| invalid(format!("{} does not exist", path.display())))?;
    if !meta.is_file() {
        return Err(invalid("That is a folder, not a backup file"));
    }
    let flags = if read_write {
        OpenFlags::default()
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let damaged =
        || invalid("This file isn't a usable Minimap backup: it is damaged or not a database");
    match security::file_state(path)? {
        FileState::Missing => Err(damaged()),
        FileState::Plain => {
            let conn = Connection::open_with_flags(path, flags).map_err(|_| damaged())?;
            Ok((conn, Key::None, meta))
        }
        FileState::Encrypted => {
            for key in std::iter::once(live).chain(extra.iter()) {
                if key.is_none() {
                    continue;
                }
                let Ok(conn) = Connection::open_with_flags(path, flags) else {
                    continue;
                };
                if security::apply(&conn, key).is_ok() && security::verify(&conn).is_ok() {
                    return Ok((conn, key.clone(), meta));
                }
            }
            Err(if !extra.is_empty() {
                StoreError::WrongKey
            } else {
                StoreError::BackupKeyNeeded
            })
        }
    }
}

/// Checks that `path` can be restored and says what is in it. Refuses (with a message meant
/// for the user) a missing file, something that isn't a SQLite database, a database that fails
/// its integrity check, one that isn't Minimap's, and one from a newer version. An encrypted
/// file is opened with `live` (the current key) or one of `extra` (keys the user typed).
pub fn inspect(
    path: &Path,
    current_version: u32,
    live: &Key,
    extra: &[Key],
) -> Result<RestorePreview> {
    inspect_open(path, current_version, live, extra, false).map(|(preview, _, _)| preview)
}

fn inspect_open(
    path: &Path,
    current_version: u32,
    live: &Key,
    extra: &[Key],
    read_write: bool,
) -> Result<(RestorePreview, Connection, Key)> {
    let (conn, key, meta) = open_backup(path, live, extra, read_write)?;
    let damaged =
        || invalid("This file isn't a usable Minimap backup: it is damaged or not a database");
    // Reading anything from a non-database fails here.
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| damaged())?;
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|_| damaged())?;
    if check != "ok" {
        return Err(invalid(format!(
            "This backup is damaged and can't be restored (integrity check: {check})"
        )));
    }
    let made_by: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'created_by'",
            [],
            |r| r.get(0),
        )
        .ok();
    if version == 0 || made_by.as_deref() != Some("minimap") {
        return Err(invalid("This file is a database, but not a Minimap backup"));
    }
    if version > current_version {
        return Err(invalid(format!(
            "This backup was made by a newer version of Minimap (its data format is {version}, \
             this version understands up to {current_version}). Update Minimap to restore it"
        )));
    }
    let mut counts = Vec::new();
    for (label, table) in COUNTED {
        // Table names are the constants above.
        let n: u32 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE archived_at IS NULL"),
                [],
                |r| r.get(0),
            )
            .map_err(|_| damaged())?;
        counts.push(BackupCount {
            label: label.to_owned(),
            count: n,
        });
    }
    let preview = RestorePreview {
        path: path.display().to_string(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        bytes: meta.len(),
        schema_version: version,
        current_schema_version: current_version,
        encrypted: !key.is_none(),
        will_upgrade: version < current_version,
        counts,
    };
    Ok((preview, conn, key))
}

/// Replaces the live data with the backup at `src`. Order: check the file, save the current
/// data as a `PreRestore` backup in `dir`, bring the backup in (SQLite's backup API when the
/// backup and the live database are both encrypted or both plain, one transaction so it is all
/// or nothing; otherwise a verified re-keyed copy swapped in), upgrade the schema if the backup
/// is older, and keep this installation's backup settings (the backup's own would point
/// elsewhere). `live` is the key the live database has; `extra` holds keys to try for a backup
/// encrypted with another key.
pub fn restore(
    conn: &mut Connection,
    src: &Path,
    dir: &Path,
    live: &Key,
    extra: &[Key],
) -> Result<RestoreResult> {
    let current = crate::schema_version(conn)?;
    let same_kind = |encrypted: bool| encrypted == !live.is_none();
    let (preview, source, _) = inspect_open(src, current, live, extra, false)?;
    let kept = settings::get(conn)?;
    let saved = create(conn, dir, BackupKind::PreRestore, current, live)?;
    if same_kind(preview.encrypted) {
        let backup = Backup::new(&source, conn)?;
        backup.run_to_completion(PAGES_PER_STEP, Duration::from_millis(0), None)?;
    } else {
        // Plain into encrypted or the other way: the export needs a writable handle.
        drop(source);
        let (_, writable, _) = inspect_open(src, current, live, extra, true)?;
        security::replace_live(conn, Some(&writable), live, live)?;
    }
    crate::upgrade(conn)?;
    settings::update(
        conn,
        UpdateSettings {
            backup_folder: Some(kept.backup_folder.unwrap_or_default()),
            auto_backup: Some(kept.auto_backup),
            ..Default::default()
        },
    )?;
    Ok(RestoreResult {
        saved_current_as: saved,
        restored_from: preview.file_name,
    })
}

/// Before a schema upgrade: the database file at `db` is copied to `<its folder>/backups/`.
/// Does nothing for a new or already current database.
pub(crate) fn before_migration(conn: &Connection, db: &Path, latest: u32, key: &Key) -> Result<()> {
    let version = crate::schema_version(conn)?;
    if version == 0 || version >= latest {
        return Ok(());
    }
    let dir: PathBuf = db
        .parent()
        .map(|p| p.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"));
    create(conn, &dir, BackupKind::PreMigration, version, key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::Key;
    use crate::test_support::{add_person, add_task, fingerprint, temp_dir};
    use crate::{open, tasks, LATEST_SCHEMA};
    use minimap_types::{NodeRef, NodeType, UpdateSettings};

    #[test]
    fn backup_modify_restore_gives_back_exactly_the_backed_up_data() {
        let dir = temp_dir("roundtrip");
        let db = dir.join("minimap.db");
        let backups = dir.join("backups");
        let mut conn = open(&db).unwrap();
        add_person(&mut conn, "Priya");
        for t in ["Ship it", "Write docs", "Plan Q3"] {
            add_task(&mut conn, t);
        }
        let before = fingerprint(&conn);
        let made = create(&conn, &backups, BackupKind::Manual, 0, &Key::None).unwrap();
        assert!(made.file_name.starts_with("minimap-2") && made.file_name.ends_with(".db"));
        assert!(!made.file_name.contains("auto"));
        assert!(made.bytes > 0);
        // One self-contained file.
        let names: Vec<String> = fs::read_dir(&backups)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![made.file_name.clone()]);

        // Change plenty, including the settings and the search index.
        add_task(&mut conn, "Added after the backup");
        add_person(&mut conn, "Sam");
        let victim = tasks::list(&conn, false).unwrap()[0].id;
        crate::nodes::archive(&mut conn, NodeRef::new(NodeType::Task, victim)).unwrap();
        settings::update(
            &mut conn,
            UpdateSettings {
                hours_per_day: Some(6.0),
                theme: Some("nord".into()),
                backup_folder: Some(backups.display().to_string()),
                auto_backup: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let after = fingerprint(&conn);
        assert_ne!(before, after);

        let result = restore(&mut conn, Path::new(&made.path), &backups, &Key::None, &[]).unwrap();
        assert_eq!(result.restored_from, made.file_name);
        // Data matches the backup exactly, apart from this installation's backup settings.
        let restored = settings::get(&conn).unwrap();
        assert_eq!(restored.backup_folder, Some(backups.display().to_string()));
        assert!(
            !restored.auto_backup,
            "this installation's backup settings are kept"
        );
        assert_eq!(fingerprint(&conn), before);
        // The index and the schema work on the restored data.
        assert_eq!(crate::schema_version(&conn).unwrap(), LATEST_SCHEMA);
        add_task(&mut conn, "Works after restoring");
        assert_eq!(tasks::list(&conn, false).unwrap().len(), 4);
        let hits = crate::search::run(&conn, "\"docs\"*", &[], false, 10).unwrap();
        assert_eq!(hits.len(), 1, "full-text search was restored with the data");

        // What was live is saved first, and holds the changes made after the backup.
        let saved = Path::new(&result.saved_current_as.path);
        assert!(saved.exists());
        assert_eq!(result.saved_current_as.kind, BackupKind::PreRestore);
        let counts = inspect(saved, LATEST_SCHEMA, &Key::None, &[])
            .unwrap()
            .counts;
        let tasks_in_saved = counts.iter().find(|c| c.label == "Tasks").unwrap().count;
        assert_eq!(tasks_in_saved, 3, "3 original - 1 archived + 1 added");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_newest_fourteen_automatic_backups_survive() {
        let dir = temp_dir("retention");
        let mut conn = open(&dir.join("minimap.db")).unwrap();
        add_task(&mut conn, "Anything");
        let backups = dir.join("backups");
        for _ in 0..20 {
            create(&conn, &backups, BackupKind::Auto, 0, &Key::None).unwrap();
        }
        let manual = create(&conn, &backups, BackupKind::Manual, 0, &Key::None).unwrap();
        let premigration =
            create(&conn, &backups, BackupKind::PreMigration, 3, &Key::None).unwrap();
        fs::write(backups.join("notes.txt"), "mine").unwrap();
        assert_eq!(list(&backups).unwrap().len(), 22);
        let newest_auto: Vec<String> = list(&backups)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == BackupKind::Auto)
            .take(14)
            .map(|e| e.file_name)
            .collect();

        assert_eq!(prune_auto(&backups, 14).unwrap(), 6);
        let left = list(&backups).unwrap();
        let auto: Vec<String> = left
            .iter()
            .filter(|e| e.kind == BackupKind::Auto)
            .map(|e| e.file_name.clone())
            .collect();
        assert_eq!(auto, newest_auto, "the newest 14 stay");
        assert!(left.iter().any(|e| e.file_name == manual.file_name));
        assert!(left.iter().any(|e| e.file_name == premigration.file_name));
        assert!(
            backups.join("notes.txt").exists(),
            "other files are never touched"
        );
        assert_eq!(prune_auto(&backups, 14).unwrap(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_is_newest_first_and_ignores_what_minimap_did_not_make() {
        let dir = temp_dir("listing");
        assert!(
            list(&dir.join("nope")).unwrap().is_empty(),
            "no folder = no backups"
        );
        for name in [
            "minimap-20270101-090000.db",
            "minimap-auto-20270103-090000.db",
            "minimap-pre-restore-20270102-090000.db",
            "minimap-20270104-090000.db.part",
            "minimap.db",
            "holiday.jpg",
        ] {
            fs::write(dir.join(name), "x").unwrap();
        }
        fs::create_dir(dir.join("minimap-20270105-090000.db")).unwrap();
        let names: Vec<String> = list(&dir)
            .unwrap()
            .into_iter()
            .map(|e| e.file_name)
            .collect();
        assert_eq!(
            names,
            vec![
                "minimap-auto-20270103-090000.db",
                "minimap-pre-restore-20270102-090000.db",
                "minimap-20270101-090000.db",
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn files_that_cannot_be_restored_are_refused_with_a_reason_and_change_nothing() {
        let dir = temp_dir("refuse");
        let backups = dir.join("backups");
        let mut conn = open(&dir.join("minimap.db")).unwrap();
        add_task(&mut conn, "Mine");
        let good = create(&conn, &backups, BackupKind::Manual, 0, &Key::None).unwrap();
        let before = fingerprint(&conn);

        // Not a database at all.
        let garbage = dir.join("garbage.db");
        fs::write(
            &garbage,
            b"this is not sqlite, just text that is long enough to look like a file",
        )
        .unwrap();
        // A real backup with its middle overwritten.
        let torn = dir.join("torn.db");
        let mut bytes = fs::read(&good.path).unwrap();
        for b in bytes.iter_mut().skip(4096).take(8192) {
            *b = 0xA5;
        }
        fs::write(&torn, &bytes).unwrap();
        // A copy from a newer version.
        let newer = dir.join("newer.db");
        fs::copy(&good.path, &newer).unwrap();
        Connection::open(&newer)
            .unwrap()
            .pragma_update(None, "user_version", LATEST_SCHEMA + 1)
            .unwrap();
        // A SQLite database that is not Minimap's.
        let foreign = dir.join("foreign.db");
        {
            let other = Connection::open(&foreign).unwrap();
            other
                .execute_batch("CREATE TABLE t(x); PRAGMA user_version = 3;")
                .unwrap();
        }
        let truncated = dir.join("truncated.db");
        fs::write(&truncated, &fs::read(&good.path).unwrap()[..6000]).unwrap();

        let cases: [(&Path, &str); 7] = [
            (&garbage, "different key"),
            (&torn, ""),
            (&newer, "newer version of Minimap"),
            (&foreign, "not a Minimap backup"),
            (&truncated, ""),
            (&dir, "folder"),
            (&dir.join("missing.db"), "does not exist"),
        ];
        for (path, expect) in cases {
            let err = restore(&mut conn, path, &backups, &Key::None, &[]).unwrap_err();
            // Random bytes can't be told apart from an encrypted file: Minimap asks for a key.
            if expect == "different key" {
                assert!(
                    matches!(err, StoreError::BackupKeyNeeded),
                    "{path:?}: {err:?}"
                );
                continue;
            }
            let StoreError::Invalid(message) = err else {
                panic!("{path:?}: expected a refusal, got {err:?}");
            };
            assert!(message.contains(expect), "{path:?}: {message}");
            assert!(!message.is_empty());
        }
        assert_eq!(
            fingerprint(&conn),
            before,
            "refused restores change nothing"
        );
        assert_eq!(
            list(&backups).unwrap().len(),
            1,
            "and save no pre-restore copy"
        );
        // The good one is accepted and described.
        let preview = inspect(Path::new(&good.path), LATEST_SCHEMA, &Key::None, &[]).unwrap();
        assert!(!preview.will_upgrade);
        assert_eq!(preview.counts.len(), 8);
        assert_eq!(
            preview
                .counts
                .iter()
                .find(|c| c.label == "Tasks")
                .unwrap()
                .count,
            1
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A database file as an older version of Minimap left it (migrated only up to `version`).
    fn old_database(path: &Path, version: usize) {
        let mut conn = Connection::open(path).unwrap();
        crate::register_functions(&conn).unwrap();
        crate::migrations().to_version(&mut conn, version).unwrap();
        conn.execute(
            "INSERT INTO objectives (id, title, description, status, priority, created_at, updated_at)
             VALUES ('01a10732-b318-740e-a2ed-5ecf4225916c', 'Old objective', '', 'on_track', 3, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn an_older_backup_is_restored_and_upgraded() {
        let dir = temp_dir("older");
        let old = dir.join("old.db");
        old_database(&old, 5);
        let backups = dir.join("backups");
        let mut conn = open(&dir.join("minimap.db")).unwrap();
        add_task(&mut conn, "Current");
        let preview = inspect(&old, LATEST_SCHEMA, &Key::None, &[]).unwrap();
        assert!(preview.will_upgrade);
        assert_eq!(preview.schema_version, 5);
        restore(&mut conn, &old, &backups, &Key::None, &[]).unwrap();
        assert_eq!(crate::schema_version(&conn).unwrap(), LATEST_SCHEMA);
        assert_eq!(crate::objectives::list(&conn, false).unwrap().len(), 1);
        assert!(tasks::list(&conn, false).unwrap().is_empty());
        // The upgraded database has the newer pieces (the search index, project handles).
        conn.query_row("SELECT COUNT(*) FROM search_index", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn upgrading_an_existing_database_saves_a_copy_first() {
        let dir = temp_dir("premigration");
        let db = dir.join("minimap.db");
        old_database(&db, 5);
        let conn = open(&db).unwrap();
        assert_eq!(crate::schema_version(&conn).unwrap(), LATEST_SCHEMA);
        let saved = list(&dir.join("backups")).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].kind, BackupKind::PreMigration);
        assert!(
            saved[0].file_name.contains("-v5-"),
            "{}",
            saved[0].file_name
        );
        // The copy is the old version, untouched by the upgrade.
        let copy = Connection::open(&saved[0].path).unwrap();
        assert_eq!(crate::schema_version(&copy).unwrap(), 5);
        drop(conn);

        // Opening again (already current) and a brand-new database make no copy.
        let _ = open(&db).unwrap();
        assert_eq!(list(&dir.join("backups")).unwrap().len(), 1);
        let fresh = temp_dir("fresh");
        let _ = open(&fresh.join("minimap.db")).unwrap();
        assert!(!fresh.join("backups").exists());
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&fresh).unwrap();
    }

    #[test]
    fn the_schema_constant_matches_the_migrations() {
        assert_eq!(
            crate::migrations()
                .current_version(&Connection::open_in_memory().unwrap())
                .ok(),
            Some(rusqlite_migration::SchemaVersion::NoneSet)
        );
        let conn = crate::open_in_memory().unwrap();
        assert_eq!(crate::schema_version(&conn).unwrap(), LATEST_SCHEMA);
    }
}
