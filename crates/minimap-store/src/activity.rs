//! Activity log. Every repository write calls into here inside its transaction.

use minimap_types::{timefmt::fmt_ts, Activity, ActivityAction, NodeType};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
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

/// The whole history, oldest first (the full export).
pub fn list_all(conn: &Connection) -> Result<Vec<Activity>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM activity ORDER BY at, rowid"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Activity from `from` through `to` (inclusive UTC days), oldest first.
pub fn list_between(conn: &Connection, from: time::Date, to: time::Date) -> Result<Vec<Activity>> {
    let start = fmt_ts(from.midnight().assume_utc());
    let end = fmt_ts((to + time::Duration::days(1)).midnight().assume_utc());
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM activity WHERE at >= ?1 AND at < ?2 ORDER BY at, id"
    ))?;
    let rows = stmt.query_map([start, end], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The rowid of the newest activity row (0 when there is none): a marker to ask [`since`] about
/// what a command wrote.
pub fn latest_rowid(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT COALESCE(MAX(rowid), 0) FROM activity", [], |r| {
            r.get(0)
        })?,
    )
}

/// The rows written after `marker` (from [`latest_rowid`]), oldest first.
pub fn since(conn: &Connection, marker: i64) -> Result<Vec<Activity>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM activity WHERE rowid > ?1 ORDER BY rowid"
    ))?;
    let rows = stmt.query_map([marker], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0))?)
}

/// How long after a note's last save a further save still counts as the same editing session.
pub(crate) const SAVE_SESSION_SECS: i64 = 300;

/// Records an `updated` row, or folds it into the node's latest row when that row is also an
/// `updated` made within [`SAVE_SESSION_SECS`] (so autosaving while typing is one entry, not
/// one per pause). Folding keeps the earliest "old" and the latest "new" of each field and
/// drops fields that ended up unchanged; if nothing is left the row is removed.
pub(crate) fn record_update_merged(
    tx: &Transaction,
    at: OffsetDateTime,
    node_type: NodeType,
    node_id: Uuid,
    diff: Map<String, Value>,
) -> Result<()> {
    let latest: Option<(String, String, String, String)> = tx
        .query_row(
            "SELECT id, at, action, diff FROM activity WHERE node_id = ?1 ORDER BY at DESC, id DESC LIMIT 1",
            [id_s(node_id)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((row_id, row_at, action, row_diff)) = latest {
        let recent = minimap_types::timefmt::parse_ts(&row_at)
            .map(|t| (at - t).whole_seconds() < SAVE_SESSION_SECS)
            .unwrap_or(false);
        if action == ActivityAction::Updated.as_str() && recent {
            let mut merged: Map<String, Value> = match serde_json::from_str(&row_diff)? {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            for (key, change) in diff {
                let old = merged
                    .get(&key)
                    .map(|c| c[0].clone())
                    .unwrap_or_else(|| change[0].clone());
                let new = change[1].clone();
                if old == new {
                    merged.remove(&key);
                } else {
                    merged.insert(key, json!([old, new]));
                }
            }
            if merged.is_empty() {
                tx.execute("DELETE FROM activity WHERE id = ?1", [row_id])?;
            } else {
                tx.execute(
                    "UPDATE activity SET at = ?2, diff = ?3 WHERE id = ?1",
                    params![
                        row_id,
                        fmt_ts(at),
                        serde_json::to_string(&Value::Object(merged))?
                    ],
                )?;
            }
            return Ok(());
        }
    }
    if diff.is_empty() {
        return Ok(());
    }
    record(
        tx,
        at,
        node_type,
        node_id,
        ActivityAction::Updated,
        &Value::Object(diff),
    )
}
