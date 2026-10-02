use minimap_types::{ActivityAction, CreateTeam, NodeType, Team, UpdateTeam};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::Result,
    repo::{fetch, fetch_all},
};

const TABLE: &str = "teams";
const COLS: &str = "id, name, description, parent_team_id, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Team> {
    Ok(Team {
        id: col_uuid(r, 0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        parent_team_id: col_uuid_opt(r, 3)?,
        created_at: col_ts(r, 4)?,
        updated_at: col_ts(r, 5)?,
        archived_at: col_ts_opt(r, 6)?,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Team> {
    fetch(conn, TABLE, COLS, NodeType::Team, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Team>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateTeam) -> Result<Team> {
    let at = now();
    let t = Team {
        id: Uuid::now_v7(),
        name: input.name,
        description: input.description,
        parent_team_id: input.parent_team_id,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    ensure_not_blank("name", &t.name)?;
    let tx = conn.transaction()?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7)"),
        params![
            id_s(t.id),
            t.name,
            t.description,
            id_opt_s(t.parent_team_id),
            ts_s(t.created_at),
            ts_s(t.updated_at),
            ts_opt_s(t.archived_at),
        ],
    )?;
    activity::record_created(&tx, at, NodeType::Team, t.id, &t)?;
    tx.commit()?;
    Ok(t)
}

/// Note: cycle detection for team nesting lives in core (feature 07).
pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateTeam) -> Result<Team> {
    let tx = conn.transaction()?;
    let old = get(&tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    ensure_not_blank("name", &new.name)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!("UPDATE {TABLE} SET name=?2, description=?3, parent_team_id=?4, updated_at=?5 WHERE id=?1"),
        params![
            id_s(id),
            new.name,
            new.description,
            id_opt_s(new.parent_team_id),
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        &tx,
        new.updated_at,
        NodeType::Team,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    tx.commit()?;
    Ok(new)
}
