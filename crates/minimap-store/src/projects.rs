use std::collections::HashSet;

use minimap_core::slug;
use minimap_types::{
    ActivityAction, CreateProject, NodeRef, NodeType, Project, ProjectStatus, TaskDisposition,
    UpdateProject, DEFAULT_PRIORITY,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::json;
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    nodes,
    repo::{fetch, fetch_all},
};

const TABLE: &str = "projects";
const COLS: &str = "id, title, slug, description, owner_person_id, start_date, target_date, status, priority, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        slug: r.get(2)?,
        description: r.get(3)?,
        owner_person_id: col_uuid_opt(r, 4)?,
        start_date: col_date_opt(r, 5)?,
        target_date: col_date_opt(r, 6)?,
        status: col_enum(r, 7)?,
        priority: r.get(8)?,
        created_at: col_ts(r, 9)?,
        updated_at: col_ts(r, 10)?,
        archived_at: col_ts_opt(r, 11)?,
    })
}

fn validate(p: &Project) -> Result<()> {
    ensure_not_blank("title", &p.title)?;
    ensure_priority(p.priority)?;
    slug::validate(&p.slug).map_err(StoreError::Invalid)
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Project> {
    fetch(conn, TABLE, COLS, NodeType::Project, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Project>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

/// The active project using `slug`, other than `except`, as (id, title).
fn slug_owner(conn: &Connection, slug: &str, except: Option<Uuid>) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT title FROM projects WHERE slug = ?1 AND archived_at IS NULL AND id != ?2",
            params![slug, except.map(id_s).unwrap_or_default()],
            |r| r.get(0),
        )
        .optional()?)
}

fn ensure_slug_free(conn: &Connection, slug: &str, except: Option<Uuid>) -> Result<()> {
    match slug_owner(conn, slug, except)? {
        Some(title) => Err(StoreError::Invalid(format!(
            "the handle #{slug} is already used by \"{title}\""
        ))),
        None => Ok(()),
    }
}

pub fn create(conn: &mut Connection, input: CreateProject) -> Result<Project> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateProject) -> Result<Project> {
    let at = now();
    let handle = match input
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(given) => {
            slug::validate(given).map_err(StoreError::Invalid)?;
            ensure_slug_free(tx, given, None)?;
            given.to_owned()
        }
        None => {
            let taken: HashSet<String> = tx
                .prepare("SELECT slug FROM projects WHERE archived_at IS NULL")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            slug::unique(&slug::slugify(&input.title), |s| taken.contains(s))
        }
    };
    let p = Project {
        id: Uuid::now_v7(),
        title: input.title,
        slug: handle,
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
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"),
        params![
            id_s(p.id),
            p.title,
            p.slug,
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
    activity::record_created(tx, at, NodeType::Project, p.id, &p)?;
    Ok(p)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateProject) -> Result<Project> {
    let tx = conn.transaction()?;
    let new = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(new)
}

/// [`update`] inside a caller's transaction.
pub(crate) fn update_in_tx(tx: &Transaction, id: Uuid, patch: UpdateProject) -> Result<Project> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    new.slug = new.slug.trim().to_owned();
    validate(&new)?;
    if new.slug != old.slug && old.archived_at.is_none() {
        ensure_slug_free(tx, &new.slug, Some(id))?;
    }
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, slug=?3, description=?4, owner_person_id=?5, start_date=?6, target_date=?7, status=?8, priority=?9, updated_at=?10 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.slug,
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
        tx,
        new.updated_at,
        NodeType::Project,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    Ok(new)
}

/// Archives a project and decides, in the same transaction, what happens to its active
/// tasks: archive them too, or detach them (`project_id` cleared) so they land in the inbox.
pub fn archive(conn: &mut Connection, id: Uuid, tasks: TaskDisposition) -> Result<()> {
    let tx = conn.transaction()?;
    let project = NodeRef::new(NodeType::Project, id);
    if nodes::archived_at(&tx, project)?.is_some() {
        return Err(StoreError::AlreadyArchived {
            node_type: NodeType::Project,
            id,
        });
    }
    let task_ids: Vec<Uuid> = tx
        .prepare("SELECT id FROM tasks WHERE project_id = ?1 AND archived_at IS NULL ORDER BY id")?
        .query_map([id_s(id)], |r| col_uuid(r, 0))?
        .collect::<rusqlite::Result<_>>()?;
    let at = now();
    for task in task_ids {
        match tasks {
            TaskDisposition::Archive => {
                nodes::archive_in_tx(&tx, NodeRef::new(NodeType::Task, task))?;
            }
            TaskDisposition::Inbox => {
                tx.execute(
                    "UPDATE tasks SET project_id = NULL, updated_at = ?2 WHERE id = ?1",
                    params![id_s(task), ts_s(at)],
                )?;
                activity::record(
                    &tx,
                    at,
                    NodeType::Task,
                    task,
                    ActivityAction::Updated,
                    &json!({ "project_id": [id, null] }),
                )?;
            }
        }
    }
    nodes::archive_in_tx(&tx, project)?;
    tx.commit()?;
    Ok(())
}
