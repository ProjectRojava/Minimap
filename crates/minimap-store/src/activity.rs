//! Activity log. Every repository write calls into here inside its transaction.

use minimap_types::{timefmt::fmt_ts, Activity, ActivityAction, NodeType};
use rusqlite::{params, Connection, Transaction};
use serde::Serialize;
use serde_json::{json, Map, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    convert::{col_enum, col_ts, col_uuid, id_s},
    error::Result,
};

/// Bookkeeping fields never reported as changes.
const META: &[&str] = &["id", "created_at", "updated_at", "archived_at"];

pub(crate) fn record(
    tx: &Transaction,
    at: OffsetDateTime,
    node_type: NodeType,
    node_id: Uuid,
    action: ActivityAction,
    diff: &Value,
) -> Result<()> {
    tx.execute(
        "INSERT INTO activity (id, at, node_type, node_id, action, diff) VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            id_s(Uuid::now_v7()),
            fmt_ts(at),
            node_type.as_str(),
            id_s(node_id),
            action.as_str(),
            serde_json::to_string(diff)?,
        ],
    )?;
    Ok(())
}

pub(crate) fn record_created<T: Serialize>(
    tx: &Transaction,
    at: OffsetDateTime,
    node_type: NodeType,
    node_id: Uuid,
    node: &T,
) -> Result<()> {
    let mut diff = Map::new();
    if let Value::Object(fields) = serde_json::to_value(node)? {
        for (k, v) in fields {
            if !META.contains(&k.as_str()) {
                diff.insert(k, json!([null, v]));
            }
        }
    }
    record(
        tx,
        at,
        node_type,
        node_id,
        ActivityAction::Created,
        &Value::Object(diff),
    )
}

/// Fields that differ between `old` and `new`, as `{field: [old, new]}`.
/// Empty when nothing but bookkeeping changed.
pub(crate) fn diff<T: Serialize>(old: &T, new: &T) -> Result<Map<String, Value>> {
    let (old, new) = (serde_json::to_value(old)?, serde_json::to_value(new)?);
    let mut out = Map::new();
    if let (Value::Object(o), Value::Object(n)) = (&old, &new) {
        for (k, nv) in n {
            if META.contains(&k.as_str()) {
                continue;
            }
            let ov = o.get(k).unwrap_or(&Value::Null);
            if ov != nv {
                out.insert(k.clone(), json!([ov, nv]));
            }
        }
    }
    Ok(out)
}

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Activity> {
    let diff: String = r.get(5)?;
    Ok(Activity {
        id: col_uuid(r, 0)?,
        at: col_ts(r, 1)?,
        node_type: col_enum(r, 2)?,
        node_id: col_uuid(r, 3)?,
        action: col_enum(r, 4)?,
        diff: serde_json::from_str(&diff).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}

const COLS: &str = "id, at, node_type, node_id, action, diff";

/// History of one node, newest first.
pub fn list_for_node(conn: &Connection, node_id: Uuid) -> Result<Vec<Activity>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM activity WHERE node_id = ?1 ORDER BY at DESC, id DESC"
    ))?;
    let rows = stmt.query_map([id_s(node_id)], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Latest activity across all nodes (for "what changed" views).
pub fn list_recent(conn: &Connection, limit: u32) -> Result<Vec<Activity>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM activity ORDER BY at DESC, id DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0))?)
}
