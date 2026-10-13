//! Pings (docs/product-design/05-pings.md, ADR-0020): a marker on any item with a day and an
//! optional line of text. A small table that points at its target (no foreign key: targets are
//! polymorphic), not a node type, so it has no page, no links and no search entry.
//!
//! Every write logs an `updated` row **on the target** with a single `ping` key,
//! `{"ping": [old, new]}`, where each side is the ping's state (`null` before it existed). That is
//! what undo reads (`minimap_core::undo`), and what the item's Activity list shows. The rules
//! (note length, no past day) are `minimap_core::pings`.
//!
//! One live ping per target is kept here, inside the transaction: making a ping on an item that
//! has a live one changes that one. (No unique index: two devices can each make one, and the
//! merge repairs it.)

use minimap_core::pings::validate;
use minimap_types::{
    timefmt::fmt_date, ActivityAction, CreatePing, NodeRef, Patch, Ping, PingRow, UpdatePing,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::{json, Value};
use time::Date;
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    nodes,
};

const COLS: &str =
    "id, target_type, target_id, note, on_date, cleared_on, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Ping> {
    Ok(Ping {
        id: col_uuid(r, 0)?,
        target: NodeRef::new(col_enum(r, 1)?, col_uuid(r, 2)?),
        note: r.get(3)?,
        on_date: col_date(r, 4)?,
        cleared_on: col_date_opt(r, 5)?,
        created_at: col_ts(r, 6)?,
        updated_at: col_ts(r, 7)?,
        archived_at: col_ts_opt(r, 8)?,
    })
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Ping> {
    conn.query_row(
        &format!("SELECT {COLS} FROM pings WHERE id = ?1"),
        [id_s(id)],
        from_row,
    )
    .optional()?
    .ok_or_else(|| invalid("that ping no longer exists"))
}

/// Every ping, cleared and removed ones included, oldest first (the full export).
pub fn list_all(conn: &Connection) -> Result<Vec<Ping>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM pings ORDER BY created_at, id"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The live ping on an item (not cleared, not removed), whether or not the item is archived.
pub fn live_for(conn: &Connection, target_id: Uuid) -> Result<Option<Ping>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {COLS} FROM pings
                 WHERE target_id = ?1 AND archived_at IS NULL AND cleared_on IS NULL
                 ORDER BY created_at DESC, id DESC LIMIT 1"
            ),
            [id_s(target_id)],
            from_row,
        )
        .optional()?)
}

/// Every live ping of an item that is not archived, with the item (unordered; core arranges).
pub fn list_live(conn: &Connection) -> Result<Vec<PingRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM pings WHERE archived_at IS NULL AND cleared_on IS NULL ORDER BY id"
    ))?;
    let pings = stmt
        .query_map([], from_row)?
        .collect::<rusqlite::Result<Vec<Ping>>>()?;
    let mut rows = Vec::with_capacity(pings.len());
    for ping in pings {
        // A ping whose item is gone, or archived, is hidden (not deleted).
        let Ok(target) = nodes::summary(conn, ping.target) else {
            continue;
        };
        if !target.archived {
            rows.push(PingRow { ping, target });
        }
    }
    Ok(rows)
}

/// The state of a ping as the activity log keeps it.
fn state(p: &Ping) -> Value {
    json!({
        "id": p.id,
        "note": p.note,
        "on_date": fmt_date(p.on_date),
        "cleared_on": p.cleared_on.map(fmt_date),
        "archived": p.archived_at.is_some(),
    })
}

fn record(tx: &Transaction, target: NodeRef, old: Option<&Ping>, new: &Ping) -> Result<()> {
    activity::record(
        tx,
        new.updated_at,
        target.node_type,
        target.id,
        ActivityAction::Updated,
        &json!({ "ping": [old.map(state), state(new)] }),
    )
}

fn insert(tx: &Transaction, p: &Ping) -> Result<()> {
    tx.execute(
        &format!("INSERT INTO pings ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"),
        params![
            id_s(p.id),
            p.target.node_type.as_str(),
            id_s(p.target.id),
            p.note,
            date_s(Some(p.on_date)),
            date_s(p.cleared_on),
            ts_s(p.created_at),
            ts_s(p.updated_at),
            ts_opt_s(p.archived_at),
        ],
    )?;
    Ok(())
}

fn write(tx: &Transaction, p: &Ping) -> Result<()> {
    tx.execute(
        "UPDATE pings SET note=?2, on_date=?3, cleared_on=?4, updated_at=?5, archived_at=?6 WHERE id=?1",
        params![
            id_s(p.id),
            p.note,
            date_s(Some(p.on_date)),
            date_s(p.cleared_on),
            ts_s(p.updated_at),
            ts_opt_s(p.archived_at),
        ],
    )?;
    Ok(())
}

fn refuse_archived_target(tx: &Transaction, target: NodeRef) -> Result<()> {
    if nodes::archived_at(tx, target)?.is_some() {
        return Err(invalid(format!(
            "You can't ping an archived {}",
            target.node_type.as_str().replace('_', " ")
        )));
    }
    Ok(())
}

/// Makes a ping on an item, or, when the item already has a live ping, changes that one (its note
/// and day): one live ping per item.
pub fn create(conn: &mut Connection, today: Date, input: CreatePing) -> Result<Ping> {
    let tx = conn.transaction()?;
    let made = create_in_tx(&tx, today, input)?;
    tx.commit()?;
    Ok(made)
}

/// [`create`] inside a caller's transaction (quick-add composes it).
pub(crate) fn create_in_tx(tx: &Transaction, today: Date, input: CreatePing) -> Result<Ping> {
    refuse_archived_target(tx, input.target)?;
    if let Some(live) = live_for(tx, input.target.id)? {
        let patch = UpdatePing {
            note: Some(input.note),
            on_date: Some(input.on_date),
            cleared_on: Patch::Keep,
        };
        return update_in_tx(tx, live.id, today, patch);
    }
    let note =
        validate(&input.note, input.on_date, today, None).map_err(|e| invalid(e.to_string()))?;
    let at = now();
    let ping = Ping {
        id: Uuid::now_v7(),
        target: input.target,
        note,
        on_date: input.on_date,
        cleared_on: None,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    insert(tx, &ping)?;
    record(tx, ping.target, None, &ping)?;
    Ok(ping)
}

pub fn update(conn: &mut Connection, id: Uuid, today: Date, patch: UpdatePing) -> Result<Ping> {
    let tx = conn.transaction()?;
    let updated = update_in_tx(&tx, id, today, patch)?;
    tx.commit()?;
    Ok(updated)
}

/// [`update`] inside a caller's transaction. Nothing changed writes nothing.
pub(crate) fn update_in_tx(
    tx: &Transaction,
    id: Uuid,
    today: Date,
    patch: UpdatePing,
) -> Result<Ping> {
    let old = get(tx, id)?;
    if old.archived_at.is_some() {
        return Err(invalid("that ping was removed"));
    }
    let mut new = old.clone();
    patch.apply(&mut new);
    new.note = validate(&new.note, new.on_date, today, Some(old.on_date))
        .map_err(|e| invalid(e.to_string()))?;
    if state(&new) == state(&old) {
        return Ok(old);
    }
    if old.cleared_on.is_some() && new.cleared_on.is_none() {
        refuse_second_live(tx, &new)?;
    }
    new.updated_at = now();
    write(tx, &new)?;
    record(tx, new.target, Some(&old), &new)?;
    Ok(new)
}

/// Clears a ping: it is done with, and drops off This week. Clearing again keeps the first day.
pub fn clear(conn: &mut Connection, id: Uuid, today: Date) -> Result<Ping> {
    let tx = conn.transaction()?;
    let current = get(&tx, id)?;
    let patch = UpdatePing {
        cleared_on: if current.cleared_on.is_some() {
            Patch::Keep
        } else {
            Patch::Set(today)
        },
        ..Default::default()
    };
    let cleared = update_in_tx(&tx, id, today, patch)?;
    tx.commit()?;
    Ok(cleared)
}

/// Brings a cleared ping back (what undoing a clear does). Refused while its item has another live
/// ping.
pub fn reopen(conn: &mut Connection, id: Uuid, today: Date) -> Result<Ping> {
    let tx = conn.transaction()?;
    let patch = UpdatePing {
        cleared_on: Patch::Clear,
        ..Default::default()
    };
    let reopened = update_in_tx(&tx, id, today, patch)?;
    tx.commit()?;
    Ok(reopened)
}

/// Removes a ping (a soft removal, logged like the rest).
pub fn archive(conn: &mut Connection, id: Uuid) -> Result<()> {
    let tx = conn.transaction()?;
    let old = get(&tx, id)?;
    if old.archived_at.is_some() {
        return Ok(());
    }
    let mut new = old.clone();
    new.updated_at = now();
    new.archived_at = Some(new.updated_at);
    write(&tx, &new)?;
    record(&tx, new.target, Some(&old), &new)?;
    Ok(tx.commit()?)
}

fn refuse_second_live(tx: &Transaction, ping: &Ping) -> Result<()> {
    if live_for(tx, ping.target.id)?.is_some_and(|other| other.id != ping.id) {
        return Err(invalid("That item already has a ping"));
    }
    Ok(())
}

/// Undo (`store::undo`): puts the ping `new` describes back to the state `old` describes
/// (`null` = it did not exist before, so it is removed), after checking that it still is as
/// `new` says. Logged like any write, so the redo step is made from the rows it writes.
pub(crate) fn restore_in_tx(
    tx: &Transaction,
    target: NodeRef,
    old: &Value,
    new: &Value,
) -> Result<()> {
    let id = new
        .get("id")
        .or_else(|| old.get("id"))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| invalid("couldn't read the ping"))?;
    let current = get(tx, id)?;
    if current.target != target {
        return Err(invalid("that ping is on another item now"));
    }
    if !new.is_null() && state(&current) != *new {
        return Err(invalid("the ping was changed since"));
    }
    let mut back = current.clone();
    let now_at = now();
    if old.is_null() {
        back.archived_at = Some(now_at);
    } else {
        back.note = old["note"].as_str().unwrap_or_default().to_owned();
        back.on_date = old["on_date"]
            .as_str()
            .and_then(|s| minimap_types::timefmt::parse_date(s).ok())
            .ok_or_else(|| invalid("couldn't read the ping's day"))?;
        back.cleared_on = old["cleared_on"]
            .as_str()
            .and_then(|s| minimap_types::timefmt::parse_date(s).ok());
        back.archived_at = old["archived"].as_bool().unwrap_or(false).then_some(now_at);
    }
    if state(&back) == state(&current) {
        return Ok(());
    }
    if back.archived_at.is_none() && back.cleared_on.is_none() {
        refuse_second_live(tx, &back)?;
    }
    back.updated_at = now_at;
    write(tx, &back)?;
    record(tx, target, Some(&current), &back)
}

/// Hard-deletes an item's pings (the item itself is being deleted), leaving a tombstone for each
/// so the deletion reaches the other devices.
pub(crate) fn delete_for_node(tx: &Transaction, node_id: Uuid) -> Result<()> {
    tx.execute(
        "INSERT INTO tombstones (kind, id, deleted_at)
         SELECT 'ping', id, ?2 FROM pings WHERE target_id = ?1 AND true
         ON CONFLICT(kind, id) DO UPDATE SET deleted_at = excluded.deleted_at",
        params![id_s(node_id), ts_s(now())],
    )?;
    tx.execute("DELETE FROM pings WHERE target_id = ?1", [id_s(node_id)])?;
    Ok(())
}

/// How many live pings sit on items of this set (`demo_remove` reports them).
pub(crate) fn count_on(conn: &Connection, ids: &std::collections::HashSet<String>) -> Result<u32> {
    let mut stmt = conn.prepare("SELECT target_id FROM pings")?;
    let found = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(u32::try_from(found.iter().filter(|id| ids.contains(*id)).count()).unwrap_or(u32::MAX))
}
