use minimap_types::{ActivityAction, CreatePerson, NodeType, Person, UpdatePerson};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    repo::{fetch, fetch_all},
};

const TABLE: &str = "people";
const COLS: &str = "id, name, role_title, email, weekly_capacity_hours, is_self, notes, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Person> {
    Ok(Person {
        id: col_uuid(r, 0)?,
        name: r.get(1)?,
        role_title: r.get(2)?,
        email: r.get(3)?,
        weekly_capacity_hours: r.get(4)?,
        is_self: r.get(5)?,
        notes: r.get(6)?,
        created_at: col_ts(r, 7)?,
        updated_at: col_ts(r, 8)?,
        archived_at: col_ts_opt(r, 9)?,
    })
}

fn validate(p: &Person) -> Result<()> {
    ensure_not_blank("name", &p.name)?;
    if !p.weekly_capacity_hours.is_finite() || p.weekly_capacity_hours <= 0.0 {
        return Err(StoreError::Invalid(
            "weekly_capacity_hours must be greater than 0".into(),
        ));
    }
    Ok(())
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Person> {
    fetch(conn, TABLE, COLS, NodeType::Person, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Person>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

/// The person who is the user, if one has been created yet.
pub fn get_self(conn: &Connection) -> Result<Option<Person>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM {TABLE} WHERE is_self = 1"),
            [],
            from_row,
        )
        .optional()?)
}

/// First-run setup: returns the self person, creating them (named `name`, or "Me") if absent.
pub fn ensure_self(conn: &mut Connection, name: &str) -> Result<Person> {
    if let Some(p) = get_self(conn)? {
        return Ok(p);
    }
    let name = match name.trim() {
        "" => "Me",
        n => n,
    };
    create(
        conn,
        CreatePerson {
            name: name.to_owned(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: true,
            notes: String::new(),
        },
    )
}

pub fn create(conn: &mut Connection, input: CreatePerson) -> Result<Person> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreatePerson) -> Result<Person> {
    let at = now();
    let p = Person {
        id: Uuid::now_v7(),
        name: input.name,
        role_title: input.role_title,
        email: input.email,
        weekly_capacity_hours: match input.weekly_capacity_hours {
            Some(h) => h,
            None => crate::settings::default_weekly_capacity_hours(tx)?,
        },
        is_self: input.is_self,
        notes: input.notes,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    validate(&p)?;
    if p.is_self && get_self(tx)?.is_some() {
        return Err(StoreError::Invalid("a self person already exists".into()));
    }
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
        params![
            id_s(p.id),
            p.name,
            p.role_title,
            p.email,
            p.weekly_capacity_hours,
            p.is_self,
            p.notes,
            ts_s(p.created_at),
            ts_s(p.updated_at),
            ts_opt_s(p.archived_at),
        ],
    )?;
    activity::record_created(tx, at, NodeType::Person, p.id, &p)?;
    Ok(p)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdatePerson) -> Result<Person> {
    let tx = conn.transaction()?;
    let new = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(new)
}

/// [`update`] inside a caller's transaction.
pub(crate) fn update_in_tx(tx: &Transaction, id: Uuid, patch: UpdatePerson) -> Result<Person> {
    let old = get(tx, id)?;
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
            "UPDATE {TABLE} SET name=?2, role_title=?3, email=?4, weekly_capacity_hours=?5, notes=?6, updated_at=?7 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.name,
            new.role_title,
            new.email,
            new.weekly_capacity_hours,
            new.notes,
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        tx,
        new.updated_at,
        NodeType::Person,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    Ok(new)
}
