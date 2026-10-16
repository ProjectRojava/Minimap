//! Helpers for tests, in this crate and the ones built on it. Not part of the app.

use rusqlite::{params, Connection};
use uuid::Uuid;

/// The id of the project [`project`] makes. It is the same on every database, with the same
/// timestamps, so two test devices that each made it hold one identical row and merge cleanly.
pub const TEST_PROJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7e57_8000_0000_0000_0001);

/// A project for a test task to be in (a task needs one): the same one every time, created the
/// first time it is asked for.
pub fn project(conn: &Connection) -> Uuid {
    conn.execute(
        "INSERT OR IGNORE INTO projects (id, title, slug, description, status, priority, created_at, updated_at)
         VALUES (?1, 'Test project', 'test-project', '', 'active', 3, ?2, ?2)",
        params![TEST_PROJECT.to_string(), "2026-01-01T00:00:00.000Z"],
    )
    .unwrap();
    TEST_PROJECT
}
