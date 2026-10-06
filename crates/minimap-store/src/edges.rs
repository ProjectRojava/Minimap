//! Raw edge persistence. The edge-type matrix and cycle rules are enforced in
//! `minimap-core` (feature 07) before these functions are called.

use minimap_types::{ActivityAction, Edge, EdgeLink, EdgeType, NewEdge, NodeRef};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    nodes,
};

pub(crate) const COLS: &str =
    "id, edge_type, from_type, from_id, to_type, to_id, attrs, created_at, archived_at";

pub(crate) fn from_row(r: &Row) -> rusqlite::Result<Edge> {
    let attrs: String = r.get(6)?;
    Ok(Edge {
        id: col_uuid(r, 0)?,
        edge_type: col_enum(r, 1)?,
        from_type: col_enum(r, 2)?,
        from_id: col_uuid(r, 3)?,
        to_type: col_enum(r, 4)?,
        to_id: col_uuid(r, 5)?,
        attrs: serde_json::from_str(&attrs).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
        })?,
        created_at: col_ts(r, 7)?,
        archived_at: col_ts_opt(r, 8)?,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Edge> {
    conn.query_row(
        &format!("SELECT {COLS} FROM edges WHERE id = ?1"),
        [id_s(id)],
        from_row,
    )
    .optional()?
    .ok_or(StoreError::EdgeNotFound(id))
}

/// Edges touching `node_id` in either direction.
pub fn list_for_node(
    conn: &Connection,
    node_id: Uuid,
    include_archived: bool,
) -> Result<Vec<Edge>> {
    let filter = if include_archived {
        ""
    } else {
        "AND archived_at IS NULL"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM edges WHERE (from_id = ?1 OR to_id = ?1) {filter} ORDER BY id"
    ))?;
    let rows = stmt.query_map([id_s(node_id)], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// All active edges (input for graph building in core).
pub fn list_active(conn: &Connection) -> Result<Vec<Edge>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM edges WHERE archived_at IS NULL ORDER BY id"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Every edge, removed ones included (the full export).
pub fn list_all(conn: &Connection) -> Result<Vec<Edge>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM edges ORDER BY created_at, id"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Active edges of one type (input for cycle checks).
pub fn list_active_of_type(conn: &Connection, edge_type: EdgeType) -> Result<Vec<Edge>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM edges WHERE edge_type = ?1 AND archived_at IS NULL ORDER BY id"
    ))?;
    let rows = stmt.query_map([edge_type.as_str()], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Adds an edge, or revives an archived one with the same (type, from, to).
pub fn add(conn: &mut Connection, new: NewEdge) -> Result<Edge> {
    let tx = conn.transaction()?;
    let edge = add_in_tx(&tx, new)?;
    tx.commit()?;
    Ok(edge)
}

/// [`add`] inside a caller's transaction.
pub(crate) fn add_in_tx(tx: &Transaction, new: NewEdge) -> Result<Edge> {
    for end in [new.from, new.to] {
        if nodes::archived_at(tx, end)?.is_some() {
            return Err(StoreError::Invalid(format!(
                "cannot link to archived {} {}",
                end.node_type, end.id
            )));
        }
    }
    let at = now();
    let attrs = serde_json::to_string(&new.attrs)?;
    let existing: Option<Edge> = tx
        .query_row(
            &format!("SELECT {COLS} FROM edges WHERE edge_type=?1 AND from_id=?2 AND to_id=?3"),
            params![new.edge_type.as_str(), id_s(new.from.id), id_s(new.to.id)],
            from_row,
        )
        .optional()?;
    let edge = match existing {
        Some(e) if e.archived_at.is_none() => return Err(StoreError::DuplicateEdge),
        Some(mut e) => {
            tx.execute(
                "UPDATE edges SET archived_at = NULL, attrs = ?2 WHERE id = ?1",
                params![id_s(e.id), attrs],
            )?;
            e.archived_at = None;
            e.attrs = new.attrs;
            e
        }
        None => {
            let e = Edge {
                id: Uuid::now_v7(),
                edge_type: new.edge_type,
                from_type: new.from.node_type,
                from_id: new.from.id,
                to_type: new.to.node_type,
                to_id: new.to.id,
                attrs: new.attrs,
                created_at: at,
                archived_at: None,
            };
            tx.execute(
                &format!("INSERT INTO edges ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"),
                params![
                    id_s(e.id),
                    e.edge_type.as_str(),
                    e.from_type.as_str(),
                    id_s(e.from_id),
                    e.to_type.as_str(),
                    id_s(e.to_id),
                    attrs,
                    ts_s(e.created_at),
                    ts_opt_s(e.archived_at),
                ],
            )?;
            e
        }
    };
    record(tx, at, &edge, ActivityAction::EdgeAdded)?;
    Ok(edge)
}

/// Replaces an active edge's attributes (validation of the values is the caller's job).
/// Records one `updated` activity row on the `from` node. Unchanged attributes write nothing.
pub fn update_attrs(conn: &mut Connection, id: Uuid, attrs: serde_json::Value) -> Result<Edge> {
    let tx = conn.transaction()?;
    let mut edge = get(&tx, id)?;
    if edge.archived_at.is_some() {
        return Err(StoreError::EdgeNotFound(id));
    }
    if edge.attrs == attrs {
        return Ok(edge);
    }
    tx.execute(
        "UPDATE edges SET attrs = ?2 WHERE id = ?1",
        params![id_s(id), serde_json::to_string(&attrs)?],
    )?;
    let diff = json!({ format!("{} link", edge.edge_type): [edge.attrs, attrs] });
    activity::record(
        &tx,
        now(),
        edge.from_type,
        edge.from_id,
        ActivityAction::Updated,
        &diff,
    )?;
    edge.attrs = attrs;
    tx.commit()?;
    Ok(edge)
}

/// Soft-removes an edge.
pub fn remove(conn: &mut Connection, id: Uuid) -> Result<()> {
    let tx = conn.transaction()?;
    let edge = get(&tx, id)?;
    if edge.archived_at.is_some() {
        return Err(StoreError::EdgeNotFound(id));
    }
    archive_in_tx(&tx, &edge, now())?;
    Ok(tx.commit()?)
}

pub(crate) fn archive_in_tx(tx: &Transaction, edge: &Edge, at: OffsetDateTime) -> Result<()> {
    tx.execute(
        "UPDATE edges SET archived_at = ?2 WHERE id = ?1",
        params![id_s(edge.id), ts_s(at)],
    )?;
    record(tx, at, edge, ActivityAction::EdgeRemoved)
}

pub(crate) fn restore_in_tx(tx: &Transaction, edge: &Edge, at: OffsetDateTime) -> Result<()> {
    tx.execute(
        "UPDATE edges SET archived_at = NULL WHERE id = ?1",
        [id_s(edge.id)],
    )?;
    record(tx, at, edge, ActivityAction::EdgeAdded)
}

/// One activity row, attached to the edge's `from` node.
fn record(tx: &Transaction, at: OffsetDateTime, edge: &Edge, action: ActivityAction) -> Result<()> {
    let desc = json!({
        "id": edge.id,
        "edge_type": edge.edge_type,
        "to_type": edge.to_type,
        "to_id": edge.to_id,
        "attrs": edge.attrs,
    });
    let diff = match action {
        ActivityAction::EdgeAdded => json!({ "edge": [null, desc] }),
        _ => json!({ "edge": [desc, null] }),
    };
    activity::record(tx, at, edge.from_type, edge.from_id, action, &diff)
}

pub(crate) fn endpoint_ref(edge: &Edge, node: NodeRef) -> NodeRef {
    if edge.from_id == node.id {
        edge.to()
    } else {
        edge.from()
    }
}

/// Edges touching `node_id`, each with a summary of the node on the other end.
pub fn links_for_node(conn: &Connection, node_id: Uuid) -> Result<Vec<EdgeLink>> {
    list_for_node(conn, node_id, false)?
        .into_iter()
        .map(|edge| {
            let outgoing = edge.from_id == node_id;
            let other = if outgoing { edge.to() } else { edge.from() };
            Ok(EdgeLink {
                outgoing,
                other: nodes::summary(conn, other)?,
                edge,
            })
        })
        .collect()
}
