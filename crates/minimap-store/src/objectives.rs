use minimap_types::{
    CreateObjective, NodeType, Objective, ObjectiveStatus, UpdateObjective, DEFAULT_PRIORITY,
};
use rusqlite::{params, Connection, Row, Transaction};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    repo::{fetch, fetch_all},
};

const TABLE: &str = "objectives";
const COLS: &str = "id, title, description, target_date, status, priority, created_at, updated_at, archived_at, ongoing, review_every_days, last_reviewed_on";

/// Longest review rhythm, in days (a year).
const MAX_REVIEW_DAYS: u32 = 365;

fn from_row(r: &Row) -> rusqlite::Result<Objective> {
    Ok(Objective {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        target_date: col_date_opt(r, 3)?,
        status: col_enum(r, 4)?,
        priority: r.get(5)?,
        created_at: col_ts(r, 6)?,
        updated_at: col_ts(r, 7)?,
        archived_at: col_ts_opt(r, 8)?,
        ongoing: r.get::<_, i64>(9)? != 0,
        review_every_days: r.get::<_, Option<u32>>(10)?,
        last_reviewed_on: col_date_opt(r, 11)?,
    })
}

fn validate(o: &Objective) -> Result<()> {
    ensure_not_blank("title", &o.title)?;
    ensure_priority(o.priority)?;
    if let Some(n) = o.review_every_days {
        if n == 0 || n > MAX_REVIEW_DAYS {
            return Err(StoreError::Invalid(format!(
                "review every must be between 1 and {MAX_REVIEW_DAYS} days"
            )));
        }
    }
    if o.ongoing && o.target_date.is_some() {
        return Err(StoreError::Invalid(
            "an ongoing objective has no target date".into(),
        ));
    }
    if o.ongoing && o.status == ObjectiveStatus::Done {
        return Err(StoreError::Invalid(
            "an ongoing objective is never done; archive it when it ends".into(),
        ));
    }
    Ok(())
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Objective> {
    fetch(conn, TABLE, COLS, NodeType::Objective, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Objective>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateObjective) -> Result<Objective> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateObjective) -> Result<Objective> {
    let at = now();
    let o = Objective {
        id: Uuid::now_v7(),
        title: input.title,
        description: input.description,
        target_date: input.target_date,
        status: input.status.unwrap_or(ObjectiveStatus::OnTrack),
        priority: input.priority.unwrap_or(DEFAULT_PRIORITY),
        ongoing: input.ongoing,
        review_every_days: input.review_every_days,
        last_reviewed_on: None,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    validate(&o)?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"),
        params![
            id_s(o.id),
            o.title,
            o.description,
            date_s(o.target_date),
            o.status.as_str(),
            o.priority,
            ts_s(o.created_at),
            ts_s(o.updated_at),
            ts_opt_s(o.archived_at),
            i64::from(o.ongoing),
            o.review_every_days,
            date_s(o.last_reviewed_on),
        ],
    )?;
    activity::record_created(tx, at, NodeType::Objective, o.id, &o)?;
    Ok(o)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateObjective) -> Result<Objective> {
    let tx = conn.transaction()?;
    let new = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(new)
}

/// [`update`] inside a caller's transaction.
pub(crate) fn update_in_tx(
    tx: &Transaction,
    id: Uuid,
    patch: UpdateObjective,
) -> Result<Objective> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    // Making an objective ongoing takes its end away (its date, and "done"); asking for a date
    // or "done" on one that stays ongoing is refused by `validate`.
    let becoming_ongoing = patch.ongoing == Some(true) && !old.ongoing;
    patch.apply(&mut new);
    if becoming_ongoing {
        new.target_date = None;
        if new.status == ObjectiveStatus::Done {
            new.status = ObjectiveStatus::OnTrack;
        }
    }
    validate(&new)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, description=?3, target_date=?4, status=?5, priority=?6, ongoing=?7, review_every_days=?8, last_reviewed_on=?9, updated_at=?10 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.description,
            date_s(new.target_date),
            new.status.as_str(),
            new.priority,
            i64::from(new.ongoing),
            new.review_every_days,
            date_s(new.last_reviewed_on),
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        tx,
        new.updated_at,
        NodeType::Objective,
        id,
        minimap_types::ActivityAction::Updated,
        &diff.into(),
    )?;
    Ok(new)
}
