use minimap_types::{
    ActivityAction, CreateTask, NodeType, Task, TaskStatus, UpdateTask, DEFAULT_PRIORITY,
};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    repo::{fetch, fetch_all},
};

const TABLE: &str = "tasks";
const COLS: &str = "id, title, description, project_id, status, estimate_days, start_date, due_date, completed_at, priority, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        project_id: col_uuid_opt(r, 3)?,
        status: col_enum(r, 4)?,
        estimate_days: r.get(5)?,
        start_date: col_date_opt(r, 6)?,
        due_date: col_date_opt(r, 7)?,
        completed_at: col_ts_opt(r, 8)?,
        priority: r.get(9)?,
        created_at: col_ts(r, 10)?,
        updated_at: col_ts(r, 11)?,
        archived_at: col_ts_opt(r, 12)?,
    })
}

fn validate(t: &Task) -> Result<()> {
    ensure_not_blank("title", &t.title)?;
    ensure_priority(t.priority)?;
    match t.estimate_days {
        Some(e) if !e.is_finite() || e < 0.0 => Err(StoreError::Invalid(
            "estimate_days must be a non-negative number".into(),
        )),
        _ => Ok(()),
    }
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Task> {
    fetch(conn, TABLE, COLS, NodeType::Task, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Task>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateTask) -> Result<Task> {
    let at = now();
    let status = input.status.unwrap_or(TaskStatus::Todo);
    let t = Task {
        id: Uuid::now_v7(),
        title: input.title,
        description: input.description,
        project_id: input.project_id,
        status,
        estimate_days: input.estimate_days,
        start_date: input.start_date,
        due_date: input.due_date,
        completed_at: (status == TaskStatus::Done).then_some(at),
        priority: input.priority.unwrap_or(DEFAULT_PRIORITY),
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    validate(&t)?;
    let tx = conn.transaction()?;
    tx.execute(
        &format!(
            "INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        ),
        params![
            id_s(t.id),
            t.title,
            t.description,
            id_opt_s(t.project_id),
            t.status.as_str(),
            t.estimate_days,
            date_s(t.start_date),
            date_s(t.due_date),
            ts_opt_s(t.completed_at),
            t.priority,
            ts_s(t.created_at),
            ts_s(t.updated_at),
            ts_opt_s(t.archived_at),
        ],
    )?;
    activity::record_created(&tx, at, NodeType::Task, t.id, &t)?;
    tx.commit()?;
    Ok(t)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateTask) -> Result<Task> {
    let tx = conn.transaction()?;
    let old = get(&tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    // completed_at follows status: set on entering `done`, cleared on leaving it.
    let at = now();
    if old.status != TaskStatus::Done && new.status == TaskStatus::Done {
        new.completed_at = Some(at);
    } else if old.status == TaskStatus::Done && new.status != TaskStatus::Done {
        new.completed_at = None;
    }
    validate(&new)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = at;
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, description=?3, project_id=?4, status=?5, estimate_days=?6, start_date=?7, due_date=?8, completed_at=?9, priority=?10, updated_at=?11 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.description,
            id_opt_s(new.project_id),
            new.status.as_str(),
            new.estimate_days,
            date_s(new.start_date),
            date_s(new.due_date),
            ts_opt_s(new.completed_at),
            new.priority,
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        &tx,
        at,
        NodeType::Task,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    tx.commit()?;
    Ok(new)
}
