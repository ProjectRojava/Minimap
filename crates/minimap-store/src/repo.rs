//! Shared plumbing for the per-node repositories.

use minimap_types::NodeType;
use rusqlite::{Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::{
    convert::id_s,
    error::{Result, StoreError},
};

/// `table` and `cols` are compile-time constants from the repositories, never user input.
pub(crate) fn fetch<T>(
    conn: &Connection,
    table: &str,
    cols: &str,
    node_type: NodeType,
    id: Uuid,
    map: fn(&Row) -> rusqlite::Result<T>,
) -> Result<T> {
    conn.query_row(
        &format!("SELECT {cols} FROM {table} WHERE id = ?1"),
        [id_s(id)],
        map,
    )
    .optional()?
    .ok_or(StoreError::NotFound { node_type, id })
}

pub(crate) fn fetch_all<T>(
    conn: &Connection,
    table: &str,
    cols: &str,
    include_archived: bool,
    map: fn(&Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let filter = if include_archived {
        ""
    } else {
        "WHERE archived_at IS NULL"
    };
    // uuid v7 ids sort by creation time.
    let mut stmt = conn.prepare(&format!("SELECT {cols} FROM {table} {filter} ORDER BY id"))?;
    let rows = stmt.query_map([], map)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn table(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Objective => "objectives",
        NodeType::Project => "projects",
        NodeType::Task => "tasks",
        NodeType::Person => "people",
        NodeType::Team => "teams",
        NodeType::Note => "notes",
        NodeType::Decision => "decisions",
        NodeType::WaitingOn => "waiting_on",
    }
}
