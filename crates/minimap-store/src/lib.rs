//! SQLite persistence: connection setup, migrations and repositories.

pub mod activity;
mod convert;
pub mod decisions;
pub mod edges;
mod error;
pub mod nodes;
pub mod notes;
pub mod objectives;
pub mod people;
pub mod projects;
mod repo;
pub mod tasks;
pub mod teams;
pub mod views;
pub mod waiting_on;

use std::path::Path;

pub use error::{Result, StoreError};
pub use rusqlite::Connection;
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

/// Opens (creating if needed) the database at `path`, applies pragmas and migrations.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    init(conn)
}

/// In-memory database, for tests.
pub fn open_in_memory() -> Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(mut conn: Connection) -> Result<Connection> {
    // journal_mode returns a row, so it must be read rather than executed.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
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
        assert_eq!(schema_version(&conn).unwrap(), 4);
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
    fn upgrades_from_v1() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_version(&mut conn, 1).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
        migrations().to_latest(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 4);
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
