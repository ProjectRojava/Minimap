use minimap_types::{
    ActivityAction, CreateWaitingOn, EdgeType, NewEdge, NodeRef, NodeType, Patch, UpdateWaitingOn,
    WaitingOn,
};
use rusqlite::{params, Connection, Row, Transaction};
use time::Date;
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    edges,
    error::{Result, StoreError},
    repo::{fetch, fetch_all},
    tasks,
};

const TABLE: &str = "waiting_on";
const COLS: &str = "id, description, person_id, asked_on, expected_by, follow_up_on, resolved_on, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<WaitingOn> {
    Ok(WaitingOn {
        id: col_uuid(r, 0)?,
        description: r.get(1)?,
        person_id: col_uuid(r, 2)?,
        asked_on: col_date(r, 3)?,
        expected_by: col_date_opt(r, 4)?,
        follow_up_on: col_date_opt(r, 5)?,
        resolved_on: col_date_opt(r, 6)?,
        created_at: col_ts(r, 7)?,
        updated_at: col_ts(r, 8)?,
        archived_at: col_ts_opt(r, 9)?,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<WaitingOn> {
    fetch(conn, TABLE, COLS, NodeType::WaitingOn, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<WaitingOn>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateWaitingOn) -> Result<WaitingOn> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateWaitingOn) -> Result<WaitingOn> {
    let at = now();
    let w = WaitingOn {
        id: Uuid::now_v7(),
        description: input.description,
        person_id: input.person_id,
        asked_on: input.asked_on.unwrap_or_else(today),
        expected_by: input.expected_by,
        follow_up_on: input.follow_up_on,
        resolved_on: None,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    ensure_not_blank("description", &w.description)?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
        params![
            id_s(w.id),
            w.description,
            id_s(w.person_id),
            date_s(Some(w.asked_on)),
            date_s(w.expected_by),
            date_s(w.follow_up_on),
            date_s(w.resolved_on),
            ts_s(w.created_at),
            ts_s(w.updated_at),
            ts_opt_s(w.archived_at),
        ],
    )?;
    activity::record_created(tx, at, NodeType::WaitingOn, w.id, &w)?;
    Ok(w)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateWaitingOn) -> Result<WaitingOn> {
    let tx = conn.transaction()?;
    let updated = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(updated)
}

/// [`update`] inside a caller's transaction.
pub(crate) fn update_in_tx(
    tx: &Transaction,
    id: Uuid,
    patch: UpdateWaitingOn,
) -> Result<WaitingOn> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    ensure_not_blank("description", &new.description)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET description=?2, person_id=?3, asked_on=?4, expected_by=?5, follow_up_on=?6, resolved_on=?7, updated_at=?8 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.description,
            id_s(new.person_id),
            date_s(Some(new.asked_on)),
            date_s(new.expected_by),
            date_s(new.follow_up_on),
            date_s(new.resolved_on),
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        tx,
        new.updated_at,
        NodeType::WaitingOn,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    Ok(new)
}

/// The active waiting-ons that are about a task (spec 56): the other end of an active `about`
/// link, however it was made. Resolved ones are included; archived ones are not.
pub fn for_task(conn: &Connection, task: Uuid) -> Result<Vec<WaitingOn>> {
    let mut out = Vec::new();
    for e in edges::list_for_node(conn, task, false)? {
        if e.edge_type == EdgeType::About && e.to_id == task && e.from_type == NodeType::WaitingOn {
            let w = get(conn, e.from_id)?;
            if w.archived_at.is_none() {
                out.push(w);
            }
        }
    }
    out.sort_by_key(|w| (w.asked_on, w.id));
    Ok(out)
}

/// A waiting-on made from a task's pane (spec 56): the waiting-on and its `about` link to the
/// task in one transaction, so one undo step. Refused for an archived or missing task.
pub fn create_for_task(
    conn: &mut Connection,
    task: Uuid,
    input: CreateWaitingOn,
) -> Result<WaitingOn> {
    let tx = conn.transaction()?;
    let t = tasks::get(&tx, task)?;
    if t.archived_at.is_some() {
        return Err(StoreError::Invalid("that task is archived".into()));
    }
    let w = create_in_tx(&tx, input)?;
    edges::add_in_tx(
        &tx,
        NewEdge {
            edge_type: EdgeType::About,
            from: NodeRef::new(NodeType::WaitingOn, w.id),
            to: NodeRef::new(NodeType::Task, task),
            attrs: serde_json::json!({}),
        },
    )?;
    tx.commit()?;
    Ok(w)
}

/// Resolves every open waiting-on about a task, as of `today`, in one transaction (the dialog
/// that follows finishing a task). Returns the ones it resolved; already resolved ones keep
/// their date.
pub fn resolve_for_task(conn: &mut Connection, task: Uuid, today: Date) -> Result<Vec<WaitingOn>> {
    let tx = conn.transaction()?;
    let mut resolved = Vec::new();
    for w in for_task(&tx, task)? {
        if w.resolved_on.is_none() {
            resolved.push(update_in_tx(
                &tx,
                w.id,
                UpdateWaitingOn {
                    resolved_on: Patch::Set(today),
                    ..Default::default()
                },
            )?);
        }
    }
    tx.commit()?;
    Ok(resolved)
}
