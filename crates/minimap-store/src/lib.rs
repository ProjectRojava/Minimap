//! SQLite persistence: connection setup, migrations and repositories.

pub mod activity;
pub mod attachments;
pub mod backup;
mod convert;
pub mod decisions;
pub mod demo;
pub mod edges;
mod error;
pub mod merge;
pub mod meta;
pub mod nodes;
pub mod notes;
pub mod objectives;
pub mod people;
pub mod projects;
pub mod quick_add;
mod repo;
pub mod search;
pub mod security;
pub mod settings;
pub mod snapshot;
pub mod tasks;
pub mod teams;
#[cfg(test)]
mod test_support;
pub mod views;
pub mod waiting_on;

use std::path::Path;

pub use error::{Result, StoreError};
pub use rusqlite::Connection;

/// The current time (UTC, millisecond precision).
pub fn now() -> time::OffsetDateTime {
    convert::now()
}

/// Today's date (UTC), the app's notion of "today".
pub fn today() -> time::Date {
    convert::today()
}
use rusqlite_migration::{Migrations, M};

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/0001_init.sql")),
        M::up(include_str!("../migrations/0002_core_data.sql")),
        M::up_with_hook(
            include_str!("../migrations/0003_project_slug.sql"),
            backfill_project_slugs,
        ),
        M::up(include_str!("../migrations/0004_project_slug_index.sql")),
        M::up(include_str!("../migrations/0005_settings.sql")),
        M::up(include_str!("../migrations/0006_waiting_on_follow_up.sql")),
        M::up(include_str!("../migrations/0007_search.sql")),
        M::up(include_str!("../migrations/0008_sync.sql")),
    ])
}

/// Gives every existing project a unique handle derived from its title (oldest first).
fn backfill_project_slugs(tx: &rusqlite::Transaction) -> rusqlite_migration::HookResult {
    let rows: Vec<(String, String)> = tx
        .prepare("SELECT id, title FROM projects ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut taken = std::collections::HashSet::new();
    for (id, title) in rows {
        let slug =
            minimap_core::slug::unique(&minimap_core::slug::slugify(&title), |s| taken.contains(s));
        tx.execute(
            "UPDATE projects SET slug = ?2 WHERE id = ?1",
            rusqlite::params![id, slug],
        )?;
        taken.insert(slug);
    }
    Ok(())
}

/// The schema version after the last migration (`PRAGMA user_version`); a test keeps it equal
/// to the migration list.
pub const LATEST_SCHEMA: u32 = 8;

/// Opens (creating if needed) the unencrypted database at `path`, applies pragmas and migrations.
/// An existing database that needs upgrading is first copied to `backups/` next to it (spec 20);
/// if that copy can't be made, nothing is migrated.
pub fn open(path: &Path) -> Result<Connection> {
    open_with_key(path, &security::Key::None)
}

/// [`open`] for a database that may be encrypted. A wrong key is `StoreError::WrongKey`.
pub fn open_with_key(path: &Path, key: &security::Key) -> Result<Connection> {
    let conn = security::connect(path, key)?;
    backup::before_migration(&conn, path, LATEST_SCHEMA, key)?;
    init(conn)
}

/// [`open_with_key`] without the pre-migration backup (the schema is already current).
pub(crate) fn open_with_key_unchecked(path: &Path, key: &security::Key) -> Result<Connection> {
    init(security::connect(path, key)?)
}

/// In-memory database, for tests.
pub fn open_in_memory() -> Result<Connection> {
    init(Connection::open_in_memory()?)
}

/// SQL functions the schema's triggers call. Every connection that writes must have them
/// (`init` does it before migrating).
fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "mention_text",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let body: String = ctx.get(0)?;
            Ok(minimap_core::notes::mentions_as_names(&body))
        },
    )
}

fn init(mut conn: Connection) -> Result<Connection> {
    upgrade(&mut conn)?;
    Ok(conn)
}

/// Sets the pragmas and brings the schema to the latest version. Also run after a restore, whose
/// backup may be from an older version.
pub fn upgrade(conn: &mut Connection) -> Result<()> {
    register_functions(conn)?;
    // journal_mode returns a row, so it must be read rather than executed.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrations().to_latest(conn)?;
    Ok(())
}

/// Current schema version (`PRAGMA user_version`).
pub fn schema_version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_migration_runs() {
        let conn = open_in_memory().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), LATEST_SCHEMA);
        let v: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'created_by'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "minimap");
    }

    #[test]
    fn foreign_keys_enabled() {
        let conn = open_in_memory().unwrap();
        let on: bool = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert!(on);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn slug_migration_backfills_unique_handles_for_existing_projects() {
        let mut conn = Connection::open_in_memory().unwrap();
        register_functions(&conn).unwrap();
        migrations().to_version(&mut conn, 2).unwrap();
        for (id, title, archived) in [
            ("1", "API Launch", None),
            ("2", "API launch!", None),
            ("3", "API Launch", Some("2026-01-01T00:00:00.000Z")),
            ("4", "日本語", None),
        ] {
            conn.execute(
                "INSERT INTO projects (id, title, status, priority, created_at, updated_at, archived_at)
                 VALUES (?1, ?2, 'planned', 3, 't', 't', ?3)",
                rusqlite::params![id, title, archived],
            )
            .unwrap();
        }
        migrations().to_latest(&mut conn).unwrap();
        let slugs: Vec<String> = conn
            .prepare("SELECT slug FROM projects ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            slugs,
            vec!["api-launch", "api-launch-2", "api-launch-3", "project"]
        );
        // The unique index now guards active projects.
        let dup = conn.execute(
            "INSERT INTO projects (id, title, slug, status, priority, created_at, updated_at)
             VALUES ('9', 'x', 'api-launch', 'planned', 3, 't', 't')",
            [],
        );
        assert!(dup.is_err());
    }

    #[test]
    fn search_migration_indexes_what_already_exists() {
        let mut conn = Connection::open_in_memory().unwrap();
        register_functions(&conn).unwrap();
        migrations().to_version(&mut conn, 6).unwrap();
        conn.execute_batch(
            "INSERT INTO notes (id, title, body, note_date, kind, created_at, updated_at, archived_at)
               VALUES ('n1', 'Sync', 'ship the warehouse @[Priya](node:00000000-0000-0000-0000-0000000000ab)', '2027-01-01', 'general', 't', 't', NULL),
                      ('n2', 'Old', 'warehouse again', '2027-01-01', 'general', 't', 't', 't');
             INSERT INTO people (id, name, role_title, notes, created_at, updated_at)
               VALUES ('p1', 'Priya', 'Engineer', '', 't', 't');",
        )
        .unwrap();
        migrations().to_latest(&mut conn).unwrap();

        // Search results need real uuids to build; the raw index is checked instead.
        let rows = |q: &str| -> Vec<String> {
            conn.prepare(
                "SELECT d.node_id FROM search_index JOIN search_docs d ON d.rowid = search_index.rowid
                 WHERE search_index MATCH ?1 ORDER BY d.node_id",
            )
            .unwrap()
            .query_map([q], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
        };
        assert_eq!(rows("warehouse"), ["n1", "n2"]);
        assert_eq!(rows("priya"), ["n1", "p1"]);
        assert!(
            rows("0000000000ab").is_empty(),
            "mention ids are not indexed"
        );
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM search_docs WHERE node_id = 'n2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 1);
    }

    #[test]
    fn upgrades_from_v1() {
        let mut conn = Connection::open_in_memory().unwrap();
        register_functions(&conn).unwrap();
        migrations().to_version(&mut conn, 1).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
        migrations().to_latest(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 8);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('tasks','edges','activity')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
    }
}
