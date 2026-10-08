//! Files attached to items (spec 22). This module keeps the rows (what a file is, where it is
//! attached); the bytes are kept by the sync crate (a local cache and Google Drive), named by
//! `sha256`, so the same file attached twice is stored once.

use minimap_core::attachments::{check_size, classify, clean_name, is_sha256, markdown};
use minimap_types::{
    ActivityAction, Attachment, AttachmentKind, AttachmentState, NodeRef, NodeType,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::json;
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    error::{Result, StoreError},
    nodes,
};

const COLS: &str =
    "id, node_type, node_id, sha256, file_name, mime_type, size_bytes, created_at, archived_at";

fn from_row(r: &Row) -> rusqlite::Result<Attachment> {
    let id = col_uuid(r, 0)?;
    let file_name: String = r.get(4)?;
    let kind = classify(&file_name)
        .map(|(kind, _)| kind)
        .unwrap_or(AttachmentKind::Text);
    let size: i64 = r.get(6)?;
    Ok(Attachment {
        id,
        node_type: col_enum(r, 1)?,
        node_id: col_uuid(r, 2)?,
        sha256: r.get(3)?,
        mime_type: r.get(5)?,
        kind,
        size_bytes: u64::try_from(size).unwrap_or(0),
        created_at: col_ts(r, 7)?,
        archived_at: col_ts_opt(r, 8)?,
        markdown: markdown(id, &file_name, kind),
        file_name,
        // The command layer knows where the bytes are and fills this in.
        state: AttachmentState::LocalOnly,
    })
}

/// Every attachment row, removed ones included, oldest first (the full export).
pub fn list_records(conn: &Connection) -> Result<Vec<minimap_types::AttachmentRecord>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM attachments ORDER BY created_at, id"
    ))?;
    let rows = stmt.query_map([], |r| {
        let size: i64 = r.get(6)?;
        Ok(minimap_types::AttachmentRecord {
            id: col_uuid(r, 0)?,
            node_type: col_enum(r, 1)?,
            node_id: col_uuid(r, 2)?,
            sha256: r.get(3)?,
            file_name: r.get(4)?,
            mime_type: r.get(5)?,
            size_bytes: u64::try_from(size).unwrap_or(0),
            created_at: col_ts(r, 7)?,
            archived_at: col_ts_opt(r, 8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Attachment> {
    conn.query_row(
        &format!("SELECT {COLS} FROM attachments WHERE id = ?1"),
        [id_s(id)],
        from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::Invalid(format!("attachment {id} not found")))
}

/// An item's attachments, oldest first.
pub fn list_for_node(
    conn: &Connection,
    node_id: Uuid,
    include_archived: bool,
) -> Result<Vec<Attachment>> {
    let filter = if include_archived {
        ""
    } else {
        "AND archived_at IS NULL"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM attachments WHERE node_id = ?1 {filter} ORDER BY id"
    ))?;
    let rows = stmt.query_map([id_s(node_id)], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// How many active files each item has (items with none are absent), for list views.
pub fn counts(conn: &Connection) -> Result<std::collections::HashMap<Uuid, u32>> {
    let mut stmt = conn.prepare(
        "SELECT node_id, COUNT(*) FROM attachments WHERE archived_at IS NULL GROUP BY node_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let (id, n) = row?;
        if let Ok(id) = Uuid::parse_str(&id) {
            out.insert(id, n);
        }
    }
    Ok(out)
}

/// Attaches a file (already stored under `sha256`) to an item. Attaching the same file to the
/// same item again returns the existing attachment.
pub fn add(
    conn: &mut Connection,
    node: NodeRef,
    file_name: &str,
    size_bytes: u64,
    sha256: &str,
) -> Result<Attachment> {
    let name = clean_name(file_name);
    let (_, mime) = classify(&name).map_err(StoreError::Invalid)?;
    check_size(size_bytes).map_err(StoreError::Invalid)?;
    if !is_sha256(sha256) {
        return Err(StoreError::Invalid(
            "the file's checksum is not valid".into(),
        ));
    }
    let tx = conn.transaction()?;
    if nodes::archived_at(&tx, node)?.is_some() {
        return Err(StoreError::Invalid(format!(
            "cannot attach a file to an archived {}",
            node.node_type
        )));
    }
    let existing: Option<Attachment> = tx
        .query_row(
            &format!(
                "SELECT {COLS} FROM attachments
                 WHERE node_id = ?1 AND sha256 = ?2 AND archived_at IS NULL"
            ),
            params![id_s(node.id), sha256],
            from_row,
        )
        .optional()?;
    if let Some(found) = existing {
        return Ok(found);
    }
    let id = Uuid::now_v7();
    let at = ts_s(now());
    tx.execute(
        &format!(
            "INSERT INTO attachments ({COLS}, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,NULL,?8)"
        ),
        params![
            id_s(id),
            node.node_type.as_str(),
            id_s(node.id),
            sha256,
            name,
            mime,
            i64::try_from(size_bytes).unwrap_or(i64::MAX),
            at
        ],
    )?;
    record(&tx, node, json!({ "attachment": [null, name] }))?;
    let made = get(&tx, id)?;
    tx.commit()?;
    Ok(made)
}

/// Removes an attachment from its item (a soft removal; the bytes go once nothing refers to
/// them).
pub fn remove(conn: &mut Connection, id: Uuid) -> Result<()> {
    let tx = conn.transaction()?;
    let found = get(&tx, id)?;
    if found.archived_at.is_some() {
        return Err(StoreError::Invalid(
            "that attachment was already removed".into(),
        ));
    }
    let at = ts_s(now());
    tx.execute(
        "UPDATE attachments SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id_s(id), at],
    )?;
    record(
        &tx,
        NodeRef::new(found.node_type, found.node_id),
        json!({ "attachment": [found.file_name, null] }),
    )?;
    Ok(tx.commit()?)
}

fn record(tx: &Transaction, node: NodeRef, diff: serde_json::Value) -> Result<()> {
    activity::record(
        tx,
        now(),
        node.node_type,
        node.id,
        ActivityAction::Updated,
        &diff,
    )
}

/// Hard-deletes an item's attachment rows (the item itself is being deleted), leaving a
/// tombstone for each so the deletion reaches the other devices.
pub(crate) fn delete_for_node(tx: &Transaction, node_id: Uuid) -> Result<()> {
    let at = ts_s(now());
    tx.execute(
        "INSERT INTO tombstones (kind, id, deleted_at)
         SELECT 'attachment', id, ?2 FROM attachments WHERE node_id = ?1 AND true
         ON CONFLICT(kind, id) DO UPDATE SET deleted_at = excluded.deleted_at",
        params![id_s(node_id), at],
    )?;
    tx.execute(
        "DELETE FROM attachments WHERE node_id = ?1",
        [id_s(node_id)],
    )?;
    Ok(())
}

/// Every distinct file that an active attachment refers to: `(sha256, size_bytes)`.
pub fn live_blobs(conn: &Connection) -> Result<Vec<(String, u64)>> {
    let mut stmt = conn.prepare(
        "SELECT sha256, MAX(size_bytes) FROM attachments WHERE archived_at IS NULL GROUP BY sha256",
    )?;
    let rows = stmt.query_map([], |r| {
        let size: i64 = r.get(1)?;
        Ok((r.get::<_, String>(0)?, u64::try_from(size).unwrap_or(0)))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Files still needed: referenced by an active attachment, or by one removed after `since`
/// (a device that was offline may not have heard yet).
pub fn referenced_shas(conn: &Connection, since: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT sha256 FROM attachments WHERE archived_at IS NULL OR archived_at > ?1",
    )?;
    let rows = stmt.query_map([since], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The kind of item an attachment belongs to, for callers that only have the id.
pub fn owner(conn: &Connection, id: Uuid) -> Result<NodeRef> {
    let found = get(conn, id)?;
    Ok(NodeRef::new(found.node_type, found.node_id))
}

#[allow(dead_code)]
fn _node_type_is_used(_: NodeType) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::add_task;
    use crate::{nodes, tasks};

    fn sha(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn a_task(conn: &mut Connection) -> NodeRef {
        add_task(conn, "Plan");
        let id = tasks::list(conn, false).unwrap()[0].id;
        NodeRef::new(NodeType::Task, id)
    }

    #[test]
    fn attaching_and_listing() {
        let mut conn = crate::open_in_memory().unwrap();
        let task = a_task(&mut conn);
        let a = add(&mut conn, task, "/tmp/Budget Q1.XLSX", 2048, &sha('a')).unwrap();
        assert_eq!(a.file_name, "Budget Q1.XLSX");
        assert_eq!(a.kind, AttachmentKind::Excel);
        assert_eq!(a.size_bytes, 2048);
        assert!(a.markdown.starts_with("[Budget Q1.XLSX](attachment:"));
        let pic = add(&mut conn, task, "map.svg", 10, &sha('b')).unwrap();
        assert!(pic.markdown.starts_with("![map.svg](attachment:"));
        let listed = list_for_node(&conn, task.id, false).unwrap();
        assert_eq!(
            listed.iter().map(|x| x.id).collect::<Vec<_>>(),
            [a.id, pic.id]
        );
        // The activity log says what was attached.
        let history = activity::list_for_node(&conn, task.id).unwrap();
        assert!(history.iter().any(|h| h
            .diff
            .get("attachment")
            .is_some_and(|d| d[1] == "Budget Q1.XLSX")));
    }

    #[test]
    fn the_same_file_on_the_same_item_is_attached_once() {
        let mut conn = crate::open_in_memory().unwrap();
        let task = a_task(&mut conn);
        let first = add(&mut conn, task, "a.png", 10, &sha('a')).unwrap();
        let again = add(&mut conn, task, "a-copy.png", 10, &sha('a')).unwrap();
        assert_eq!(first.id, again.id);
        assert_eq!(list_for_node(&conn, task.id, false).unwrap().len(), 1);
        // Another item may attach the same bytes: one blob, two attachments.
        add_task(&mut conn, "Other");
        let other = tasks::list(&conn, false).unwrap()[1].id;
        add(
            &mut conn,
            NodeRef::new(NodeType::Task, other),
            "a.png",
            10,
            &sha('a'),
        )
        .unwrap();
        assert_eq!(live_blobs(&conn).unwrap(), vec![(sha('a'), 10)]);
    }

    #[test]
    fn bad_files_are_refused() {
        let mut conn = crate::open_in_memory().unwrap();
        let task = a_task(&mut conn);
        for (name, size, hash) in [
            ("run.exe", 10, sha('a')),
            ("empty.png", 0, sha('a')),
            (
                "huge.png",
                minimap_types::MAX_ATTACHMENT_BYTES + 1,
                sha('a'),
            ),
            ("ok.png", 10, "nothex".to_owned()),
        ] {
            assert!(
                matches!(
                    add(&mut conn, task, name, size, &hash),
                    Err(StoreError::Invalid(_))
                ),
                "{name}"
            );
        }
        assert!(list_for_node(&conn, task.id, true).unwrap().is_empty());
        // Nothing can be attached to a missing or archived item.
        let missing = NodeRef::new(NodeType::Task, Uuid::now_v7());
        assert!(add(&mut conn, missing, "a.png", 10, &sha('a')).is_err());
        nodes::archive(&mut conn, task).unwrap();
        assert!(add(&mut conn, task, "a.png", 10, &sha('a')).is_err());
    }

    #[test]
    fn removing_hides_the_row_and_frees_the_blob_after_the_grace_period() {
        let mut conn = crate::open_in_memory().unwrap();
        let task = a_task(&mut conn);
        let a = add(&mut conn, task, "a.png", 10, &sha('a')).unwrap();
        remove(&mut conn, a.id).unwrap();
        assert!(list_for_node(&conn, task.id, false).unwrap().is_empty());
        assert_eq!(list_for_node(&conn, task.id, true).unwrap().len(), 1);
        assert!(live_blobs(&conn).unwrap().is_empty());
        assert!(remove(&mut conn, a.id).is_err());
        // Still "referenced" for a device that has been away since before the removal...
        assert_eq!(
            referenced_shas(&conn, "2000-01-01T00:00:00.000Z").unwrap(),
            vec![sha('a')]
        );
        // ...and free once the removal is older than the cutoff.
        assert!(referenced_shas(&conn, "2999-01-01T00:00:00.000Z")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn deleting_an_item_deletes_its_attachment_rows_and_leaves_tombstones() {
        let mut conn = crate::open_in_memory().unwrap();
        let task = a_task(&mut conn);
        let a = add(&mut conn, task, "a.png", 10, &sha('a')).unwrap();
        nodes::archive(&mut conn, task).unwrap();
        nodes::delete(&mut conn, task).unwrap();
        assert!(get(&conn, a.id).is_err());
        let kinds: Vec<(String, String)> = conn
            .prepare("SELECT kind, id FROM tombstones ORDER BY kind")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            kinds,
            vec![
                ("attachment".to_owned(), a.id.to_string()),
                ("task".to_owned(), task.id.to_string())
            ]
        );
    }
}
