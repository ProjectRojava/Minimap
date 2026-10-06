use minimap_types::{
    ActivityAction, CreateDecision, Decision, DecisionStatus, Edge, EdgeType, NewEdge, NodeRef,
    NodeType, UpdateDecision,
};
use rusqlite::{params, Connection, Row, Transaction};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    edges,
    error::Result,
    repo::{fetch, fetch_all},
};

const TABLE: &str = "decisions";
const COLS: &str =
    "id, title, context, decision, rationale, decided_on, status, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Decision> {
    Ok(Decision {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        context: r.get(2)?,
        decision: r.get(3)?,
        rationale: r.get(4)?,
        decided_on: col_date_opt(r, 5)?,
        status: col_enum(r, 6)?,
        created_at: col_ts(r, 7)?,
        updated_at: col_ts(r, 8)?,
        archived_at: col_ts_opt(r, 9)?,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Decision> {
    fetch(conn, TABLE, COLS, NodeType::Decision, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Decision>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateDecision) -> Result<Decision> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateDecision) -> Result<Decision> {
    let at = now();
    let d = Decision {
        id: Uuid::now_v7(),
        title: input.title,
        context: input.context,
        decision: input.decision,
        rationale: input.rationale,
        status: input.status.unwrap_or(DecisionStatus::Proposed),
        // Deciding something puts a date on it.
        decided_on: input
            .decided_on
            .or_else(|| (input.status == Some(DecisionStatus::Decided)).then(today)),
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    ensure_not_blank("title", &d.title)?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
        params![
            id_s(d.id),
            d.title,
            d.context,
            d.decision,
            d.rationale,
            date_s(d.decided_on),
            d.status.as_str(),
            ts_s(d.created_at),
            ts_s(d.updated_at),
            ts_opt_s(d.archived_at),
        ],
    )?;
    activity::record_created(tx, at, NodeType::Decision, d.id, &d)?;
    Ok(d)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateDecision) -> Result<Decision> {
    let tx = conn.transaction()?;
    let new = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(new)
}

/// Moving a decision to `decided` without a date stamps today.
pub(crate) fn update_in_tx(tx: &Transaction, id: Uuid, patch: UpdateDecision) -> Result<Decision> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    if new.status == DecisionStatus::Decided
        && old.status != DecisionStatus::Decided
        && new.decided_on.is_none()
    {
        new.decided_on = Some(today());
    }
    ensure_not_blank("title", &new.title)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, context=?3, decision=?4, rationale=?5, decided_on=?6, status=?7, updated_at=?8 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.context,
            new.decision,
            new.rationale,
            date_s(new.decided_on),
            new.status.as_str(),
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        tx,
        new.updated_at,
        NodeType::Decision,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    Ok(new)
}

/// `new` replaces `old`: adds the `supersedes` link and marks `old` as superseded, in one
/// transaction (the matrix and loop checks are the caller's job).
pub fn supersede(conn: &mut Connection, new: Uuid, old: Uuid) -> Result<Edge> {
    let tx = conn.transaction()?;
    let edge = supersede_in_tx(&tx, new, old)?;
    tx.commit()?;
    Ok(edge)
}

/// [`supersede`] inside a caller's transaction.
pub(crate) fn supersede_in_tx(tx: &Transaction, new: Uuid, old: Uuid) -> Result<Edge> {
    let edge = edges::add_in_tx(
        tx,
        NewEdge {
            edge_type: EdgeType::Supersedes,
            from: NodeRef::new(NodeType::Decision, new),
            to: NodeRef::new(NodeType::Decision, old),
            attrs: serde_json::json!({}),
        },
    )?;
    let older = get(tx, old)?;
    if older.status != DecisionStatus::Superseded {
        update_in_tx(
            tx,
            old,
            UpdateDecision {
                status: Some(DecisionStatus::Superseded),
                ..Default::default()
            },
        )?;
    }
    Ok(edge)
}
