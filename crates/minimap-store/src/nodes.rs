//! Operations that work the same for every node type: archive, unarchive, hard delete.

use minimap_types::{timefmt::fmt_ts, ActivityAction, NodeRef};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use crate::{
    activity,
    convert::*,
    edges,
    error::{Result, StoreError},
    repo::table,
};

/// `None` = node exists and is active, `Some(ts)` = archived at `ts`. Errors if the node is missing.
pub(crate) fn archived_at(conn: &Connection, node: NodeRef) -> Result<Option<String>> {
    conn.query_row(
        &format!(
            "SELECT archived_at FROM {} WHERE id = ?1",
            table(node.node_type)
        ),
        [id_s(node.id)],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()?
    .ok_or(StoreError::NotFound {
        node_type: node.node_type,
        id: node.id,
    })
}

fn touching_edges(
    tx: &rusqlite::Transaction,
    node: NodeRef,
    archived_at_filter: Option<&str>,
) -> Result<Vec<minimap_types::Edge>> {
    let sql = match archived_at_filter {
        None => format!(
            "SELECT {} FROM edges WHERE (from_id = ?1 OR to_id = ?1) AND archived_at IS NULL",
            edges::COLS
        ),
        Some(_) => format!(
            "SELECT {} FROM edges WHERE (from_id = ?1 OR to_id = ?1) AND archived_at = ?2",
            edges::COLS
        ),
    };
    let mut stmt = tx.prepare(&sql)?;
    let rows = match archived_at_filter {
        None => stmt.query_map([id_s(node.id)], edges::from_row)?,
        Some(ts) => stmt.query_map(params![id_s(node.id), ts], edges::from_row)?,
    };
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Archives the node and every active edge touching it, in one transaction.
pub fn archive(conn: &mut Connection, node: NodeRef) -> Result<()> {
    let tx = conn.transaction()?;
    if archived_at(&tx, node)?.is_some() {
        return Err(StoreError::AlreadyArchived {
            node_type: node.node_type,
            id: node.id,
        });
    }
    let at = now();
    tx.execute(
        &format!(
            "UPDATE {} SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
            table(node.node_type)
        ),
        params![id_s(node.id), ts_s(at)],
    )?;
    activity::record(
        &tx,
        at,
        node.node_type,
        node.id,
        ActivityAction::Archived,
        &json!({ "archived_at": [null, fmt_ts(at)] }),
    )?;
    for edge in touching_edges(&tx, node, None)? {
        edges::archive_in_tx(&tx, &edge, at)?;
    }
    tx.commit()?;
    Ok(())
}

/// Restores the node and the edges that were archived along with it
/// (same timestamp), unless their other endpoint is still archived.
pub fn unarchive(conn: &mut Connection, node: NodeRef) -> Result<()> {
    let tx = conn.transaction()?;
    let Some(archived) = archived_at(&tx, node)? else {
        return Err(StoreError::NotArchivedYet {
            node_type: node.node_type,
            id: node.id,
        });
    };
    let at = now();
    tx.execute(
        &format!(
            "UPDATE {} SET archived_at = NULL, updated_at = ?2 WHERE id = ?1",
            table(node.node_type)
        ),
        params![id_s(node.id), ts_s(at)],
    )?;
    activity::record(
        &tx,
        at,
        node.node_type,
        node.id,
        ActivityAction::Unarchived,
        &json!({ "archived_at": [archived, null] }),
    )?;
    for edge in touching_edges(&tx, node, Some(&archived))? {
        if archived_at(&tx, edges::endpoint_ref(&edge, node))?.is_none() {
            edges::restore_in_tx(&tx, &edge, at)?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Permanently removes an archived node and all its edges. Activity history is kept.
/// Fails with `Constraint` if other records still reference the node.
pub fn delete(conn: &mut Connection, node: NodeRef) -> Result<()> {
    let tx = conn.transaction()?;
    if archived_at(&tx, node)?.is_none() {
        return Err(StoreError::NotArchived {
            node_type: node.node_type,
            id: node.id,
        });
    }
    tx.execute(
        "DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1",
        [id_s(node.id)],
    )?;
    tx.execute(
        &format!("DELETE FROM {} WHERE id = ?1", table(node.node_type)),
        [id_s(node.id)],
    )?;
    activity::record(
        &tx,
        now(),
        node.node_type,
        node.id,
        ActivityAction::Deleted,
        &json!({}),
    )?;
    tx.commit()?;
    Ok(())
}
