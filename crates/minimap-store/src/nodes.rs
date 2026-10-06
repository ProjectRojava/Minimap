//! Operations that work the same for every node type: archive, unarchive, hard delete.

use minimap_types::{timefmt::fmt_ts, ActivityAction, NodeRef, NodeSummary, NodeType};
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

/// The person who is the user can be neither archived nor deleted.
fn ensure_not_self(conn: &Connection, node: NodeRef) -> Result<()> {
    if node.node_type != NodeType::Person {
        return Ok(());
    }
    let is_self: Option<bool> = conn
        .query_row(
            "SELECT is_self FROM people WHERE id = ?1",
            [id_s(node.id)],
            |r| r.get(0),
        )
        .optional()?;
    if is_self == Some(true) {
        return Err(StoreError::Invalid(
            "you can't archive or delete yourself".into(),
        ));
    }
    Ok(())
}

/// Archives the node and every active edge touching it, in one transaction.
pub fn archive(conn: &mut Connection, node: NodeRef) -> Result<()> {
    let tx = conn.transaction()?;
    archive_in_tx(&tx, node)?;
    tx.commit()?;
    Ok(())
}

/// [`archive`] inside a caller's transaction, so several archives can be one atomic write.
pub(crate) fn archive_in_tx(tx: &rusqlite::Transaction, node: NodeRef) -> Result<()> {
    ensure_not_self(tx, node)?;
    if archived_at(tx, node)?.is_some() {
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
        tx,
        at,
        node.node_type,
        node.id,
        ActivityAction::Archived,
        &json!({ "archived_at": [null, fmt_ts(at)] }),
    )?;
    for edge in touching_edges(tx, node, None)? {
        edges::archive_in_tx(tx, &edge, at)?;
    }
    Ok(())
}

/// Restores the node and the edges that were archived along with it
/// (same timestamp), unless their other endpoint is still archived.
pub fn unarchive(conn: &mut Connection, node: NodeRef) -> Result<()> {
    let tx = conn.transaction()?;
    unarchive_in_tx(&tx, node)?;
    tx.commit()?;
    Ok(())
}

/// [`unarchive`] inside a caller's transaction.
pub(crate) fn unarchive_in_tx(tx: &rusqlite::Transaction, node: NodeRef) -> Result<()> {
    let Some(archived) = archived_at(tx, node)? else {
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
        tx,
        at,
        node.node_type,
        node.id,
        ActivityAction::Unarchived,
        &json!({ "archived_at": [archived, null] }),
    )?;
    for edge in touching_edges(tx, node, Some(&archived))? {
        if archived_at(tx, edges::endpoint_ref(&edge, node))?.is_none() {
            edges::restore_in_tx(tx, &edge, at)?;
        }
    }
    Ok(())
}

/// Permanently removes an archived node and all its edges. Activity history is kept.
/// Fails with `Constraint` if other records still reference the node.
pub fn delete(conn: &mut Connection, node: NodeRef) -> Result<()> {
    let tx = conn.transaction()?;
    delete_in_tx(&tx, node)?;
    tx.commit()?;
    Ok(())
}

/// [`delete`] inside a caller's transaction, so several deletes can be one atomic write.
pub(crate) fn delete_in_tx(tx: &rusqlite::Transaction, node: NodeRef) -> Result<()> {
    ensure_not_self(tx, node)?;
    if archived_at(tx, node)?.is_none() {
        return Err(StoreError::NotArchived {
            node_type: node.node_type,
            id: node.id,
        });
    }
    tx.execute(
        "DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1",
        [id_s(node.id)],
    )?;
    crate::attachments::delete_for_node(tx, node.id)?;
    tx.execute(
        &format!("DELETE FROM {} WHERE id = ?1", table(node.node_type)),
        [id_s(node.id)],
    )?;
    // The deletion must reach the other devices, and the item must never come back from them.
    tx.execute(
        "INSERT INTO tombstones (kind, id, deleted_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(kind, id) DO UPDATE SET deleted_at = excluded.deleted_at",
        params![node.node_type.as_str(), id_s(node.id), ts_s(now())],
    )?;
    activity::record(
        tx,
        now(),
        node.node_type,
        node.id,
        ActivityAction::Deleted,
        &json!({}),
    )?;
    Ok(())
}

/// Column holding the human-readable name of each node type.
pub(crate) fn label_column(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Person | NodeType::Team => "name",
        NodeType::WaitingOn => "description",
        _ => "title",
    }
}

/// Label and archived state of any node, for lists, links and the detail pane header.
pub fn summary(conn: &Connection, node: NodeRef) -> Result<NodeSummary> {
    conn.query_row(
        &format!(
            "SELECT {}, archived_at IS NOT NULL FROM {} WHERE id = ?1",
            label_column(node.node_type),
            table(node.node_type)
        ),
        [id_s(node.id)],
        |r| {
            Ok(NodeSummary {
                node,
                label: r.get(0)?,
                archived: r.get(1)?,
            })
        },
    )
    .optional()?
    .ok_or(StoreError::NotFound {
        node_type: node.node_type,
        id: node.id,
    })
}

/// Active nodes of one type as (id, label), ordered by label. For pickers and link targets.
pub fn list_summaries(conn: &Connection, node_type: NodeType) -> Result<Vec<NodeSummary>> {
    let col = label_column(node_type);
    let mut stmt = conn.prepare(&format!(
        "SELECT id, {col} FROM {} WHERE archived_at IS NULL ORDER BY lower({col}), id",
        table(node_type)
    ))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, label)| {
            let id = uuid::Uuid::parse_str(&id).map_err(|e| {
                StoreError::Invalid(format!("malformed id in {}: {e}", table(node_type)))
            })?;
            Ok(NodeSummary {
                node: NodeRef::new(node_type, id),
                label,
                archived: false,
            })
        })
        .collect()
}

/// Which kind of node has this id (ids are unique across all tables), if any.
pub fn find(conn: &Connection, id: uuid::Uuid) -> Result<Option<NodeRef>> {
    for &node_type in NodeType::ALL {
        let hit: Option<i64> = conn
            .query_row(
                &format!("SELECT 1 FROM {} WHERE id = ?1", table(node_type)),
                [id_s(id)],
                |r| r.get(0),
            )
            .optional()?;
        if hit.is_some() {
            return Ok(Some(NodeRef::new(node_type, id)));
        }
    }
    Ok(None)
}
