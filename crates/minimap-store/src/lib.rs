//! SQLite persistence: connection setup, migrations and repositories.

use std::path::Path;

pub use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("../migrations/0001_init.sql"))])
}

/// Opens (creating if needed) the database at `path`, applies pragmas and migrations.
pub fn open(path: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open(path)?;
    init(conn)
}

/// In-memory database, for tests.
pub fn open_in_memory() -> Result<Connection, StoreError> {
    init(Connection::open_in_memory()?)
}

fn init(mut conn: Connection) -> Result<Connection, StoreError> {
    // journal_mode returns a row, so it must be read rather than executed.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

/// Current schema version (`PRAGMA user_version`).
pub fn schema_version(conn: &Connection) -> Result<u32, StoreError> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_migration_runs() {
        let conn = open_in_memory().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
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
