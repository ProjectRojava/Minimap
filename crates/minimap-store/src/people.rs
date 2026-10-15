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

/// "Me" can't be archived, but an older version could end up with one that was (another device
/// archived what it saw as a duplicate). Puts that right: if an active person has the same name,
/// "me" was that person all along (the archived record was the duplicate), so the archived one is
/// merged into them and they become "me" on every device; otherwise "me" is simply restored.
/// Returns whether anything changed.
pub fn repair_self(conn: &mut Connection) -> Result<bool> {
    let tx = conn.transaction()?;
    let Some(me) = get_self(&tx)? else {
        return Ok(false);
    };
    if me.archived_at.is_none() {
        return Ok(false);
    }
    let twin: Option<String> = tx
        .query_row(
            "SELECT id FROM people
             WHERE id != ?1 AND archived_at IS NULL AND lower(trim(name)) = lower(trim(?2))
             ORDER BY created_at, id LIMIT 1",
            params![id_s(me.id), me.name],
            |r| r.get(0),
        )
        .optional()?;
    match twin {
        Some(twin) => {
            let twin = Uuid::parse_str(&twin)
                .map_err(|e| StoreError::Invalid(format!("malformed person id: {e}")))?;
            tx.execute("UPDATE people SET is_self = 0 WHERE id = ?1", [id_s(me.id)])?;
            tx.execute("UPDATE people SET is_self = 1 WHERE id = ?1", [id_s(twin)])?;
            crate::settings::set_self_person_id(&tx, twin)?;
            crate::self_person::fold(&tx, me.id, twin)?;
        }
        None => crate::nodes::unarchive_in_tx(
            &tx,
            minimap_types::NodeRef::new(NodeType::Person, me.id),
        )?,
    }
    tx.commit()?;
    Ok(true)
}

/// Merges `from` into `into`, for two records of the same person: everything linked to `from`
/// (tasks, projects they own, waiting-on, mentions in notes, links, files, pings) moves to
/// `into`, and `from` is deleted for good, on every device. If `from` was "me", `into` is "me"
/// from now on, on every device too.
pub fn merge_into(conn: &mut Connection, from: Uuid, into: Uuid) -> Result<Person> {
    if from == into {
        return Err(StoreError::Invalid(
            "pick two different people to merge".into(),
        ));
    }
    let tx = conn.transaction()?;
    let source = get(&tx, from)?;
    let target = get(&tx, into)?;
    if target.archived_at.is_some() {
        return Err(StoreError::Invalid(format!(
            "{} is archived; restore them before merging someone into them",
            target.name
        )));
    }
    if source.is_self {
        tx.execute(
            "UPDATE people SET is_self = 0 WHERE id = ?1",
            [id_s(from)],
        )?;
        tx.execute("UPDATE people SET is_self = 1 WHERE id = ?1", [id_s(into)])?;
        crate::settings::set_self_person_id(&tx, into)?;
    }
    let moved = crate::self_person::fold(&tx, from, into)?;
    activity::record(
        &tx,
        now(),
        NodeType::Person,
        from,
        ActivityAction::Deleted,
        &serde_json::json!({ "merged_into": id_s(into), "moved": moved }),
    )?;
    tx.commit()?;
    get(conn, into)
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
