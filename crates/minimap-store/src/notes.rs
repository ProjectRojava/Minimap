use minimap_core::notes as rules;
use minimap_types::{
    ActivityAction, AssigneeChoice, CreateNote, CreateTask, EdgeType, NewEdge, NodeRef, NodeType,
    Note, NoteKind, Patch, Task, UpdateNote,
};
use rusqlite::{params, Connection, Row, Transaction};
use serde_json::{json, Map, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    edges,
    error::{Result, StoreError},
    nodes,
    repo::{fetch, fetch_all},
    tasks,
};

const TABLE: &str = "notes";
const COLS: &str =
    "id, title, body, note_date, kind, created_at, updated_at, archived_at, recurrence";

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
        recurrence: col_recurrence(r, 8)?,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Note> {
    fetch(conn, TABLE, COLS, NodeType::Note, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Note>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

/// "1,204 chars": note bodies are private and large, so activity records their size, not their text.
fn size_of(body: &str) -> String {
    format!("{} chars", body.chars().count())
}

/// Keeps the note's `mentions` links equal to the `@[Name](node:id)` mentions in its body:
/// adds links for new mentions and archives links whose mention was removed. Mentions of
/// nodes that don't exist or are archived are ignored (they render as plain text).
fn sync_mentions(tx: &Transaction, note: Uuid, body: &str, at: OffsetDateTime) -> Result<()> {
    let mut wanted: Vec<NodeRef> = Vec::new();
    for id in rules::mention_ids(body)
        .into_iter()
        .filter(|&id| id != note)
    {
        if let Some(target) = nodes::find(tx, id)? {
            if nodes::archived_at(tx, target)?.is_none() {
                wanted.push(target);
            }
        }
    }
    let current: Vec<_> = edges::list_for_node(tx, note, false)?
        .into_iter()
        .filter(|e| e.edge_type == EdgeType::Mentions && e.from_id == note)
        .collect();
    for edge in &current {
        if !wanted.iter().any(|w| w.id == edge.to_id) {
            edges::archive_in_tx(tx, edge, at)?;
        }
    }
    for target in wanted {
        if !current.iter().any(|e| e.to_id == target.id) {
            edges::add_in_tx(
                tx,
                NewEdge {
                    edge_type: EdgeType::Mentions,
                    from: NodeRef::new(NodeType::Note, note),
                    to: target,
                    attrs: json!({}),
                },
            )?;
        }
    }
    Ok(())
}

fn validate_rule(n: &Note) -> Result<()> {
    match &n.recurrence {
        Some(rule) => minimap_core::recurrence::validate(rule).map_err(StoreError::Invalid),
        None => Ok(()),
    }
}

/// Makes the notes of repeating series that have come due (spec 27): for each note that
/// repeats, when the rule's next date after the note's own has come, a new note for that date
/// (only the latest such date, never one per missed week) with the same title and kind, the
/// rule's template and the previous note's open checklist items; the rule moves to the new
/// note. Returns how many were made. Safe to call as often as you like.
pub fn generate_due(conn: &mut Connection, today: time::Date) -> Result<u32> {
    let heads: Vec<Note> = conn
        .prepare(&format!(
            "SELECT {COLS} FROM {TABLE} WHERE recurrence IS NOT NULL AND archived_at IS NULL ORDER BY note_date, id"
        ))?
        .query_map([], from_row)?
        .collect::<rusqlite::Result<_>>()?;
    let mut made = 0;
    for head in heads {
        let Some(rule) = head.recurrence.clone() else {
            continue;
        };
        let Some(date) =
            minimap_core::recurrence::due_note_date(rule.cadence, head.note_date, today)
        else {
            continue;
        };
        let tx = conn.transaction()?;
        create_in_tx(
            &tx,
            CreateNote {
                title: head.title.clone(),
                body: minimap_core::recurrence::new_note_body(rule.template.as_deref(), &head.body),
                note_date: Some(date),
                kind: Some(head.kind),
                recurrence: Some(rule),
            },
        )?;
        update_in_tx(
            &tx,
            head.id,
            UpdateNote {
                recurrence: Patch::Clear,
                ..Default::default()
            },
        )?;
        tx.commit()?;
        made += 1;
    }
    Ok(made)
}

pub fn create(conn: &mut Connection, input: CreateNote) -> Result<Note> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateNote) -> Result<Note> {
    let at = now();
    let n = Note {
        id: Uuid::now_v7(),
        title: input.title,
        body: input.body,
        note_date: input.note_date.unwrap_or_else(today),
        kind: input.kind.unwrap_or(NoteKind::General),
        recurrence: input.recurrence,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    ensure_not_blank("title", &n.title)?;
    validate_rule(&n)?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"),
        params![
            id_s(n.id),
            n.title,
            n.body,
            date_s(Some(n.note_date)),
            n.kind.as_str(),
            ts_s(n.created_at),
            ts_s(n.updated_at),
            ts_opt_s(n.archived_at),
            recurrence_s(n.recurrence.as_ref()),
        ],
    )?;
    // Like `record_created`, but with the body reduced to its size.
    let mut diff = Map::new();
    if let Value::Object(fields) = serde_json::to_value(&n)? {
        for (k, v) in fields {
            if matches!(
                k.as_str(),
                "id" | "created_at" | "updated_at" | "archived_at"
            ) {
                continue;
            }
            let v = if k == "body" {
                json!(size_of(&n.body))
            } else {
                v
            };
            diff.insert(k, json!([null, v]));
        }
    }
    activity::record(
        tx,
        at,
        NodeType::Note,
        n.id,
        ActivityAction::Created,
        &Value::Object(diff),
    )?;
    sync_mentions(tx, n.id, &n.body, at)?;
    Ok(n)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateNote) -> Result<Note> {
    let tx = conn.transaction()?;
    let note = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(note)
}

fn update_in_tx(tx: &Transaction, id: Uuid, patch: UpdateNote) -> Result<Note> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    ensure_not_blank("title", &new.title)?;
    validate_rule(&new)?;
    let mut diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!("UPDATE {TABLE} SET title=?2, body=?3, note_date=?4, kind=?5, recurrence=?6, updated_at=?7 WHERE id=?1"),
        params![
            id_s(id),
            new.title,
            new.body,
            date_s(Some(new.note_date)),
            new.kind.as_str(),
            recurrence_s(new.recurrence.as_ref()),
            ts_s(new.updated_at),
        ],
    )?;
    if diff.contains_key("body") {
        diff.insert(
            "body".into(),
            json!([size_of(&old.body), size_of(&new.body)]),
        );
    }
    // Autosaves within one editing session fold into a single activity row.
    activity::record_update_merged(tx, new.updated_at, NodeType::Note, id, diff)?;
    if old.body != new.body {
        sync_mentions(tx, id, &new.body, new.updated_at)?;
    }
    Ok(new)
}

/// Turns an unchecked `[ ]` line of the note into a task, in one transaction: the task is
/// assigned to the first person mentioned on the line (otherwise to me) and filed in the first
/// project mentioned, and the line becomes `- [x] @[title](node:task)`, which also links the
/// note to the task. `expected_text` must match the line's current text.
pub fn convert_checklist_item(
    conn: &mut Connection,
    note: Uuid,
    line: usize,
    expected_text: &str,
) -> Result<Task> {
    let tx = conn.transaction()?;
    let body = get(&tx, note)?.body;
    let item = rules::checklist(&body)
        .into_iter()
        .find(|i| i.line as usize == line)
        .ok_or_else(|| StoreError::Invalid("that line is no longer an unchecked item".into()))?;
    // The caller saw the line earlier; if it has changed since, don't convert something else.
    if item.text != expected_text {
        return Err(StoreError::Invalid(
            "that line changed since you looked at it; check the note and try again".into(),
        ));
    }

    let mut assignee = AssigneeChoice::Me;
    let mut project_id = None;
    for m in &item.mentions {
        let Some(target) = nodes::find(&tx, m.id)? else {
            continue;
        };
        if nodes::archived_at(&tx, target)?.is_some() {
            continue;
        }
        match target.node_type {
            NodeType::Person if assignee == AssigneeChoice::Me => {
                assignee = AssigneeChoice::Person(m.id);
            }
            NodeType::Project if project_id.is_none() => project_id = Some(m.id),
            _ => {}
        }
    }
    let task = tasks::create_in_tx(
        &tx,
        CreateTask {
            links: Vec::new(),
            title: item.text,
            assignee,
            description: String::new(),
            project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        },
    )?;
    let converted = rules::convert_line(&body, line, &task.title, task.id)
        .ok_or_else(|| StoreError::Invalid("that line is no longer an unchecked item".into()))?;
    update_in_tx(
        &tx,
        note,
        UpdateNote {
            body: Some(converted),
            ..Default::default()
        },
    )?;
    tx.commit()?;
    Ok(task)
}
