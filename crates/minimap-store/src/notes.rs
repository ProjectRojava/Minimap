use minimap_types::{ActivityAction, CreateNote, NodeType, Note, NoteKind, UpdateNote};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::Result,
    repo::{fetch, fetch_all},
};

const TABLE: &str = "notes";
const COLS: &str = "id, title, body, note_date, kind, created_at, updated_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        body: r.get(2)?,
        note_date: col_date(r, 3)?,
        kind: col_enum(r, 4)?,
        created_at: col_ts(r, 5)?,
        updated_at: col_ts(r, 6)?,
        archived_at: col_ts_opt(r, 7)?,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Note> {
    fetch(conn, TABLE, COLS, NodeType::Note, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Note>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateNote) -> Result<Note> {
    let at = now();
    let n = Note {
        id: Uuid::now_v7(),
        title: input.title,
        body: input.body,
        note_date: input.note_date.unwrap_or_else(today),
        kind: input.kind.unwrap_or(NoteKind::General),
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    ensure_not_blank("title", &n.title)?;
    let tx = conn.transaction()?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"),
        params![
            id_s(n.id),
            n.title,
            n.body,
            date_s(Some(n.note_date)),
            n.kind.as_str(),
            ts_s(n.created_at),
            ts_s(n.updated_at),
            ts_opt_s(n.archived_at),
        ],
    )?;
    activity::record_created(&tx, at, NodeType::Note, n.id, &n)?;
    tx.commit()?;
    Ok(n)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateNote) -> Result<Note> {
    let tx = conn.transaction()?;
    let old = get(&tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    ensure_not_blank("title", &new.title)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!("UPDATE {TABLE} SET title=?2, body=?3, note_date=?4, kind=?5, updated_at=?6 WHERE id=?1"),
        params![
            id_s(id),
            new.title,
            new.body,
            date_s(Some(new.note_date)),
            new.kind.as_str(),
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        &tx,
        new.updated_at,
        NodeType::Note,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    tx.commit()?;
    Ok(new)
}
