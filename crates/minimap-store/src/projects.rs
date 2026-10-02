use minimap_types::{
    ActivityAction, CreateProject, NodeType, Project, ProjectStatus, UpdateProject,
    DEFAULT_PRIORITY,
};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::Result,
    repo::{fetch, fetch_all},
};

const TABLE: &str = "projects";
const COLS: &str = "id, title, description, owner_person_id, start_date, target_date, status, priority, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        owner_person_id: col_uuid_opt(r, 3)?,
        start_date: col_date_opt(r, 4)?,
        target_date: col_date_opt(r, 5)?,
        status: col_enum(r, 6)?,
        priority: r.get(7)?,
        created_at: col_ts(r, 8)?,
        updated_at: col_ts(r, 9)?,
        archived_at: col_ts_opt(r, 10)?,
    })
}

fn validate(p: &Project) -> Result<()> {
    ensure_not_blank("title", &p.title)?;
    ensure_priority(p.priority)
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Project> {
    fetch(conn, TABLE, COLS, NodeType::Project, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Project>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateProject) -> Result<Project> {
    let at = now();
    let p = Project {
        id: Uuid::now_v7(),
        title: input.title,
        description: input.description,
        owner_person_id: input.owner_person_id,
        start_date: input.start_date,
        target_date: input.target_date,
        status: input.status.unwrap_or(ProjectStatus::Planned),
        priority: input.priority.unwrap_or(DEFAULT_PRIORITY),
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    validate(&p)?;
    let tx = conn.transaction()?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"),
        params![
            id_s(p.id),
            p.title,
            p.description,
            id_opt_s(p.owner_person_id),
            date_s(p.start_date),
            date_s(p.target_date),
            p.status.as_str(),
            p.priority,
            ts_s(p.created_at),
            ts_s(p.updated_at),
            ts_opt_s(p.archived_at),
        ],
    )?;
    activity::record_created(&tx, at, NodeType::Project, p.id, &p)?;
    tx.commit()?;
    Ok(p)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateProject) -> Result<Project> {
    let tx = conn.transaction()?;
    let old = get(&tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    validate(&new)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, description=?3, owner_person_id=?4, start_date=?5, target_date=?6, status=?7, priority=?8, updated_at=?9 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.description,
            id_opt_s(new.owner_person_id),
            date_s(new.start_date),
            date_s(new.target_date),
            new.status.as_str(),
            new.priority,
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        &tx,
        new.updated_at,
        NodeType::Project,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    tx.commit()?;
    Ok(new)
}
