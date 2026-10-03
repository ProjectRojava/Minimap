use minimap_types::{ActivityAction, CreateWaitingOn, NodeType, UpdateWaitingOn, WaitingOn};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::Result,
    repo::{fetch, fetch_all},
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
    let tx = conn.transaction()?;
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
    activity::record_created(&tx, at, NodeType::WaitingOn, w.id, &w)?;
    tx.commit()?;
    Ok(w)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateWaitingOn) -> Result<WaitingOn> {
    let tx = conn.transaction()?;
    let old = get(&tx, id)?;
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
        &tx,
        new.updated_at,
        NodeType::WaitingOn,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    tx.commit()?;
    Ok(new)
}
