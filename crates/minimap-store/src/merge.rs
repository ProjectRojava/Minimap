//! Merging another device's copy of the data into this one (spec 22, ADR-0011).
//!
//! The other device's snapshot is opened as a second connection and compared with the live
//! database table by table, inside one transaction on the live one (all or nothing):
//!
//! * rows match by id (edges by `(edge_type, from_id, to_id)`); the later `updated_at` wins the
//!   whole row, equal timestamps go to the larger content hash so both devices choose the same
//!   copy ([`minimap_core::sync::remote_wins`]);
//! * a hard delete leaves a tombstone, which beats any edit;
//! * activity is unioned; work-related settings merge per key;
//! * afterwards the rules the schema cannot express are re-checked and repaired (duplicate
//!   project handles, two "me" people, loops, links and pointers to things that are gone), and
//!   every repair is reported.
//!
//! Merging is symmetric and repeatable: merging A into B and B into A converge, and merging the
//! same snapshot twice changes nothing.

use std::collections::{BTreeMap, HashMap, HashSet};

use minimap_core::sync::{fnv1a, links_to_drop, remote_wins, LoopLink};
use minimap_types::{MergeSummary, NodeRef, NodeType};
use rusqlite::{params_from_iter, types::Value, Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::{
    convert::{now, ts_s},
    error::Result,
    meta, nodes,
    repo::table,
    settings::LOCAL_ONLY,
};

type RowMap = BTreeMap<String, Value>;

/// What a merge does with the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    /// Work out what would change and change nothing (the transaction is rolled back).
    Preview,
    /// Merge for real.
    Apply,
    /// Bring back items from an older copy (a checkpoint): whatever differs from the old copy
    /// is put back as it was, stamped as a new edit so it wins on the other devices too; items
    /// that only exist here stay.
    Recover,
}

/// One kind of row and how it is matched and compared.
struct Spec {
    table: &'static str,
    /// The tombstone kind, for rows that can be hard-deleted.
    tombstone_kind: Option<&'static str>,
    key: &'static [&'static str],
    /// Columns left out of the content hash (and never copied over an existing row).
    local_cols: &'static [&'static str],
}

const EDGES: Spec = Spec {
    table: "edges",
    tombstone_kind: None,
    key: &["edge_type", "from_id", "to_id"],
    local_cols: &["id"],
};

const ATTACHMENTS: Spec = Spec {
    table: "attachments",
    tombstone_kind: Some("attachment"),
    key: &["id"],
    local_cols: &[],
};

fn node_spec(node_type: NodeType) -> Spec {
    Spec {
        table: table(node_type),
        tombstone_kind: Some(node_type.as_str()),
        key: &["id"],
        // Who is "me" is a fact about this device, not about the item.
        local_cols: if node_type == NodeType::Person {
            &["is_self"]
        } else {
            &[]
        },
    }
}

struct Cx<'a> {
    tx: &'a Transaction<'a>,
    remote: &'a Connection,
    mode: MergeMode,
    device: &'a str,
    /// Stamp for rows brought back by a recovery.
    recovered_at: String,
    summary: MergeSummary,
    tombstoned: HashSet<(String, String)>,
    /// This database already has its "me".
    has_self: bool,
}

fn q(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", q(table)))?;
    let cols = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(cols.collect::<rusqlite::Result<_>>()?)
}

fn read_rows(conn: &Connection, table: &str, cols: &[String]) -> Result<Vec<RowMap>> {
    if cols.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<String> = cols.iter().map(|c| q(c)).collect();
    let mut stmt = conn.prepare(&format!("SELECT {} FROM {}", list.join(", "), q(table)))?;
    let rows = stmt.query_map([], |r| {
        let mut row = RowMap::new();
        for (i, c) in cols.iter().enumerate() {
            row.insert(c.clone(), r.get::<_, Value>(i)?);
        }
        Ok(row)
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn text(row: &RowMap, col: &str) -> String {
    match row.get(col) {
        Some(Value::Text(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => format!("{other:?}"),
    }
}

fn key_of(row: &RowMap, key: &[&str]) -> String {
    key.iter()
        .map(|c| text(row, c))
        .collect::<Vec<_>>()
        .join("\u{1}")
}

/// A hash of what a row says (everything except the columns that stay local), the same on every
/// machine.
fn content_hash(row: &RowMap, skip: &[&str]) -> u64 {
    let mut bytes = Vec::new();
    for (name, value) in row {
        if skip.contains(&name.as_str()) {
            continue;
        }
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(b'=');
        bytes.extend_from_slice(format!("{value:?}").as_bytes());
        bytes.push(0);
    }
    fnv1a(&bytes)
}

fn insert_row(tx: &Transaction, table: &str, row: &RowMap) -> Result<()> {
    let names: Vec<String> = row.keys().map(|c| q(c)).collect();
    let marks: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
    tx.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES ({})",
            q(table),
            names.join(", "),
            marks.join(", ")
        ),
        params_from_iter(row.values()),
    )?;
    Ok(())
}

/// Overwrites the row with `id`, leaving `skip` columns (and the id) as they are.
fn update_row(tx: &Transaction, table: &str, id: &str, row: &RowMap, skip: &[&str]) -> Result<()> {
    let cols: Vec<(&String, &Value)> = row
        .iter()
        .filter(|(c, _)| c.as_str() != "id" && !skip.contains(&c.as_str()))
        .collect();
    if cols.is_empty() {
        return Ok(());
    }
    let sets: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, (c, _))| format!("{} = ?{}", q(c), i + 1))
        .collect();
    let mut values: Vec<&Value> = cols.iter().map(|(_, v)| *v).collect();
    let id_value = Value::Text(id.to_owned());
    values.push(&id_value);
    tx.execute(
        &format!(
            "UPDATE {} SET {} WHERE id = ?{}",
            q(table),
            sets.join(", "),
            values.len()
        ),
        params_from_iter(values),
    )?;
    Ok(())
}

fn label(tx: &Transaction, node_type: NodeType, id: &str) -> String {
    Uuid::parse_str(id)
        .ok()
        .and_then(|u| nodes::summary(tx, NodeRef::new(node_type, u)).ok())
        .map(|s| s.label)
        .unwrap_or_else(|| id.to_owned())
}

// ------------------------------------------------------------------ the merge

/// Merges `remote` (another device's snapshot, already upgraded to this schema) into `conn`.
pub fn merge(
    conn: &mut Connection,
    remote: &Connection,
    remote_device: &str,
    mode: MergeMode,
) -> Result<MergeSummary> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.pragma_update(None, "defer_foreign_keys", true)?;
    tx.execute(
        "INSERT OR REPLACE INTO app_meta (key, value) VALUES (?1, '1')",
        [meta::MERGING],
    )?;
    let has_self: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM people WHERE is_self = 1)",
        [],
        |r| r.get(0),
    )?;
    let at = now();
    let mut cx = Cx {
        tx: &tx,
        remote,
        mode,
        device: remote_device,
        recovered_at: ts_s(at),
        summary: MergeSummary {
            from_device: remote_device.to_owned(),
            at: minimap_core::backup::display_time(at),
            ..Default::default()
        },
        tombstoned: HashSet::new(),
        has_self,
    };

    if mode != MergeMode::Recover {
        apply_tombstones(&mut cx)?;
    }
    load_tombstones(&mut cx)?;
    for node_type in NodeType::ALL {
        merge_table(&mut cx, &node_spec(*node_type))?;
    }
    merge_table(&mut cx, &ATTACHMENTS)?;
    merge_table(&mut cx, &EDGES)?;
    merge_activity(&mut cx)?;
    if mode != MergeMode::Recover {
        merge_settings(&mut cx)?;
    }
    repair(&mut cx)?;

    let summary = cx.summary;
    tx.execute("DELETE FROM app_meta WHERE key = ?1", [meta::MERGING])?;
    match mode {
        MergeMode::Preview => tx.rollback()?,
        MergeMode::Apply | MergeMode::Recover => tx.commit()?,
    }
    Ok(summary)
}

// ------------------------------------------------------------------ tombstones

/// `(kind, id, deleted_at, lifted_at)` of the other device's tombstones.
type Tombstone = (String, String, String, Option<String>);

fn remote_tombstones(cx: &Cx) -> Result<Vec<Tombstone>> {
    let mut stmt = cx
        .remote
        .prepare("SELECT kind, id, deleted_at, lifted_at FROM tombstones")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// SQL: a tombstone counts while it is newer than its lifting (if it was ever lifted).
const ACTIVE: &str = "(lifted_at IS NULL OR deleted_at > lifted_at)";

/// Merges the other device's tombstones into ours (both columns take the later value, so the
/// devices agree) and deletes here what is now deleted for good.
fn apply_tombstones(cx: &mut Cx) -> Result<()> {
    for (kind, id, deleted_at, lifted_at) in remote_tombstones(cx)? {
        if kind == "setting" {
            continue; // handled with the settings
        }
        let table_name = match kind.parse::<NodeType>() {
            Ok(nt) => Some(table(nt)),
            Err(_) if kind == "attachment" => Some("attachments"),
            Err(_) => None,
        };
        let Some(table_name) = table_name else {
            continue; // a kind from a newer version
        };
        cx.tx.execute(
            "INSERT INTO tombstones (kind, id, deleted_at, lifted_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(kind, id) DO UPDATE SET
               deleted_at = MAX(deleted_at, excluded.deleted_at),
               lifted_at = CASE
                 WHEN excluded.lifted_at IS NULL THEN lifted_at
                 WHEN lifted_at IS NULL THEN excluded.lifted_at
                 ELSE MAX(lifted_at, excluded.lifted_at) END",
            rusqlite::params![&kind, &id, &deleted_at, &lifted_at],
        )?;
        let active: bool = cx.tx.query_row(
            &format!("SELECT {ACTIVE} FROM tombstones WHERE kind = ?1 AND id = ?2"),
            [&kind, &id],
            |r| r.get(0),
        )?;
        if !active {
            continue;
        }
        // Never delete this device's "me" because another device did.
        let is_self: bool = table_name == "people"
            && cx.tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM people WHERE id = ?1 AND is_self = 1)",
                [&id],
                |r| r.get(0),
            )?;
        if is_self {
            continue;
        }
        let existed: bool = cx.tx.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM {} WHERE id = ?1)",
                q(table_name)
            ),
            [&id],
            |r| r.get(0),
        )?;
        if !existed {
            continue;
        }
        if table_name != "attachments" {
            cx.tx
                .execute("DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1", [&id])?;
            cx.tx.execute(
                "INSERT INTO tombstones (kind, id, deleted_at)
                 SELECT 'attachment', id, ?2 FROM attachments WHERE node_id = ?1 AND true
                 ON CONFLICT(kind, id) DO UPDATE SET deleted_at = MAX(deleted_at, excluded.deleted_at)",
                rusqlite::params![&id, &deleted_at],
            )?;
            cx.tx
                .execute("DELETE FROM attachments WHERE node_id = ?1", [&id])?;
        }
        cx.tx.execute(
            &format!("DELETE FROM {} WHERE id = ?1", q(table_name)),
            [&id],
        )?;
        cx.summary.deleted += 1;
    }
    Ok(())
}

fn load_tombstones(cx: &mut Cx) -> Result<()> {
    let mut stmt = cx
        .tx
        .prepare(&format!("SELECT kind, id FROM tombstones WHERE {ACTIVE}"))?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    cx.tombstoned = rows.collect::<rusqlite::Result<_>>()?;
    Ok(())
}

// ------------------------------------------------------------------ rows

fn merge_table(cx: &mut Cx, spec: &Spec) -> Result<()> {
    let local_cols = columns(cx.tx, spec.table)?;
    let remote_cols = columns(cx.remote, spec.table)?;
    let common: Vec<String> = local_cols
        .iter()
        .filter(|c| remote_cols.contains(c))
        .cloned()
        .collect();
    let local: HashMap<String, RowMap> = read_rows(cx.tx, spec.table, &local_cols)?
        .into_iter()
        .map(|row| (key_of(&row, spec.key), row))
        .collect();
    let mut remote_rows = read_rows(cx.remote, spec.table, &common)?;
    // A stable order, so repairs come out the same on every device.
    remote_rows.sort_by_key(|r| (text(r, "created_at"), key_of(r, spec.key)));

    for mut row in remote_rows {
        let key = key_of(&row, spec.key);
        if let Some(kind) = spec.tombstone_kind {
            if cx.mode != MergeMode::Recover
                && cx.tombstoned.contains(&(kind.to_owned(), text(&row, "id")))
            {
                continue;
            }
        }
        if spec.table == "edges" && edge_endpoint_missing(cx, &row)? {
            continue;
        }
        normalise(cx, spec, &mut row)?;
        let remote_hash = content_hash(&row, spec.local_cols);
        let remote_stamp = text(&row, "updated_at");
        match local.get(&key) {
            None => {
                if cx.mode == MergeMode::Recover {
                    row.insert("updated_at".into(), Value::Text(cx.recovered_at.clone()));
                    lift_tombstone(cx, spec, &row)?;
                }
                prepare_insert(cx, spec, &mut row)?;
                insert_row(cx.tx, spec.table, &row)?;
                cx.summary.added += 1;
            }
            Some(here) => {
                let here_hash = content_hash(here, spec.local_cols);
                if here_hash == remote_hash {
                    continue;
                }
                let wins = cx.mode == MergeMode::Recover
                    || remote_wins(
                        &text(here, "updated_at"),
                        here_hash,
                        &remote_stamp,
                        remote_hash,
                    );
                if !wins {
                    continue;
                }
                if cx.mode == MergeMode::Recover {
                    row.insert("updated_at".into(), Value::Text(cx.recovered_at.clone()));
                }
                prepare_update(cx, spec, &row)?;
                let local_id = text(here, "id");
                update_row(cx.tx, spec.table, &local_id, &row, spec.local_cols)?;
                cx.summary.updated += 1;
            }
        }
    }
    Ok(())
}

/// A recovered item is no longer deleted: the tombstone is lifted (and the lifting travels).
fn lift_tombstone(cx: &mut Cx, spec: &Spec, row: &RowMap) -> Result<()> {
    if let Some(kind) = spec.tombstone_kind {
        let id = text(row, "id");
        cx.tx.execute(
            "UPDATE tombstones SET lifted_at = ?3 WHERE kind = ?1 AND id = ?2",
            rusqlite::params![kind, id, &cx.recovered_at],
        )?;
    }
    Ok(())
}

/// Repairs the remote copy of a row before it is compared, so that a rule the schema enforces
/// (a unique project handle) doesn't make two identical rows look different.
fn normalise(cx: &mut Cx, spec: &Spec, row: &mut RowMap) -> Result<()> {
    if spec.table != "projects" || !matches!(row.get("archived_at"), Some(Value::Null) | None) {
        return Ok(());
    }
    let id = text(row, "id");
    let slug = text(row, "slug");
    let holder = active_project_with_slug(cx.tx, &slug, &id)?;
    if let Some((_, holder_created, holder_id)) = holder {
        let mine = (text(row, "created_at"), id.clone());
        if mine > (holder_created, holder_id) {
            // The other project had the handle first; this one is renamed.
            let new = conflict_slug(cx.tx, &slug, &id)?;
            cx.summary.repairs.push(format!(
                "The project \"{}\" from {} wanted the handle #{slug}, which another project \
                 already has; it is now #{new}",
                text(row, "title"),
                cx.device
            ));
            row.insert("slug".into(), Value::Text(new));
        }
    }
    Ok(())
}

/// `(title, created_at, id)` of the active project that has `slug`, other than `except_id`.
fn active_project_with_slug(
    tx: &Transaction,
    slug: &str,
    except_id: &str,
) -> Result<Option<(String, String, String)>> {
    let mut stmt = tx.prepare(
        "SELECT title, created_at, id FROM projects
         WHERE slug = ?1 AND archived_at IS NULL AND id != ?2",
    )?;
    let mut rows = stmt.query(rusqlite::params![slug, except_id])?;
    Ok(match rows.next()? {
        Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?)),
        None => None,
    })
}

/// The handle a project gets when another project already has the one it wanted: its own
/// handle plus the last six hex digits of its id (`launch-3f2a1b`). It depends only on the
/// project, never on what else has been merged or in which order, so every device derives the
/// same one (counting `-2`, `-3` would depend on arrival order and the devices would never
/// agree). In the astronomically unlikely case that it is taken too, numbers are added.
fn conflict_slug(tx: &Transaction, wanted: &str, id: &str) -> Result<String> {
    let digits: String = id.chars().filter(char::is_ascii_hexdigit).collect();
    let tail = &digits[digits.len().saturating_sub(6)..];
    // Leave room for "-" and the six digits within the handle's length limit.
    let room = minimap_core::slug::MAX_LEN.saturating_sub(tail.len() + 1);
    let base: String = wanted.chars().take(room).collect();
    let base = base.trim_end_matches('-');
    let candidate = format!("{base}-{}", tail.to_ascii_lowercase());
    let taken: HashSet<String> = {
        let mut stmt =
            tx.prepare("SELECT slug FROM projects WHERE archived_at IS NULL AND id != ?1")?;
        let rows = stmt.query_map([id], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    Ok(minimap_core::slug::unique(&candidate, |s| {
        taken.contains(s) || s == wanted
    }))
}

/// Last changes to a row about to be inserted: who is "me", and a handle held by an older
/// project here.
fn prepare_insert(cx: &mut Cx, spec: &Spec, row: &mut RowMap) -> Result<()> {
    if spec.table == "people" {
        let remote_self = matches!(row.get("is_self"), Some(Value::Integer(1)));
        let become_self = remote_self && !cx.has_self;
        if remote_self && cx.has_self {
            cx.summary.repairs.push(format!(
                "{} was the \"me\" of {}; here it is an ordinary person (this device already has its own \"me\")",
                text(row, "name"),
                cx.device
            ));
        }
        if become_self {
            cx.has_self = true;
        }
        row.insert("is_self".into(), Value::Integer(i64::from(become_self)));
    }
    if spec.table == "projects" {
        displace_slug_holder(cx, row)?;
    }
    Ok(())
}

fn prepare_update(cx: &mut Cx, spec: &Spec, row: &RowMap) -> Result<()> {
    if spec.table == "projects" {
        displace_slug_holder(cx, row)?;
    }
    Ok(())
}

/// If an older project here has the handle this row is about to take, that one is renamed (the
/// later-created project always gives way, on every device).
fn displace_slug_holder(cx: &mut Cx, row: &RowMap) -> Result<()> {
    if !matches!(row.get("archived_at"), Some(Value::Null) | None) {
        return Ok(());
    }
    let id = text(row, "id");
    let slug = text(row, "slug");
    if let Some((title, _, holder_id)) = active_project_with_slug(cx.tx, &slug, &id)? {
        let new = conflict_slug(cx.tx, &slug, &holder_id)?;
        cx.tx.execute(
            "UPDATE projects SET slug = ?2 WHERE id = ?1",
            rusqlite::params![holder_id, new],
        )?;
        cx.summary.repairs.push(format!(
            "The project \"{title}\" had to give up its handle #{slug} to an older project from {}; \
             it is now #{new}",
            cx.device
        ));
    }
    Ok(())
}

/// Does an edge point at something that is not in this database (and will not be)?
fn edge_endpoint_missing(cx: &Cx, row: &RowMap) -> Result<bool> {
    for (type_col, id_col) in [("from_type", "from_id"), ("to_type", "to_id")] {
        let Ok(node_type) = text(row, type_col).parse::<NodeType>() else {
            return Ok(true);
        };
        let id = text(row, id_col);
        if cx
            .tombstoned
            .contains(&(node_type.as_str().to_owned(), id.clone()))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

// ------------------------------------------------------------------ activity and settings

fn merge_activity(cx: &mut Cx) -> Result<()> {
    let cols = ["id", "at", "node_type", "node_id", "action", "diff"];
    let names: Vec<String> = cols.iter().map(|c| (*c).to_owned()).collect();
    let remote_rows = read_rows(cx.remote, "activity", &names)?;
    let local_at: HashMap<String, String> = {
        let mut stmt = cx.tx.prepare("SELECT id, at FROM activity")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for row in remote_rows {
        let id = text(&row, "id");
        match local_at.get(&id) {
            None => insert_row(cx.tx, "activity", &row)?,
            Some(at) if text(&row, "at") > *at => {
                cx.tx.execute(
                    "UPDATE activity SET at = ?2, diff = ?3 WHERE id = ?1",
                    rusqlite::params![id, text(&row, "at"), text(&row, "diff")],
                )?;
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn merge_settings(cx: &mut Cx) -> Result<()> {
    let mut stmt = cx
        .remote
        .prepare("SELECT key, value, updated_at FROM settings")?;
    let remote: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let local: HashMap<String, (String, String)> = {
        let mut stmt = cx
            .tx
            .prepare("SELECT key, value, updated_at FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    // A deletion on the other device removes a setting here that is older than the deletion.
    for (kind, key, deleted_at, _) in remote_tombstones(cx)? {
        if kind != "setting" || LOCAL_ONLY.contains(&key.as_str()) {
            continue;
        }
        if let Some((_, stamp)) = local.get(&key) {
            if *stamp < deleted_at {
                cx.tx
                    .execute("DELETE FROM settings WHERE key = ?1", [&key])?;
            }
        }
        cx.tx.execute(
            "INSERT INTO tombstones (kind, id, deleted_at) VALUES ('setting', ?1, ?2)
             ON CONFLICT(kind, id) DO UPDATE SET deleted_at = MAX(deleted_at, excluded.deleted_at)",
            rusqlite::params![&key, &deleted_at],
        )?;
    }
    let tomb_now: HashMap<String, String> = {
        let mut stmt = cx
            .tx
            .prepare("SELECT id, deleted_at FROM tombstones WHERE kind = 'setting'")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (key, value, stamp) in remote {
        if LOCAL_ONLY.contains(&key.as_str()) {
            continue;
        }
        if tomb_now.get(&key).is_some_and(|d| *d >= stamp) {
            continue; // deleted after this version was written
        }
        let current = cx
            .tx
            .query_row(
                "SELECT value, updated_at FROM settings WHERE key = ?1",
                [&key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .ok();
        match current {
            None => {
                cx.tx.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![key, value, stamp],
                )?;
                cx.tx.execute(
                    "DELETE FROM tombstones WHERE kind = 'setting' AND id = ?1",
                    [&key],
                )?;
            }
            Some((here, here_stamp)) => {
                let (hh, rh) = (fnv1a(here.as_bytes()), fnv1a(value.as_bytes()));
                if here == value {
                    // The same value written at two moments: both devices keep the later one.
                    if stamp > here_stamp {
                        cx.tx.execute(
                            "UPDATE settings SET updated_at = ?2 WHERE key = ?1",
                            rusqlite::params![key, stamp],
                        )?;
                    }
                } else if remote_wins(&here_stamp, hh, &stamp, rh) {
                    cx.tx.execute(
                        "UPDATE settings SET value = ?2, updated_at = ?3 WHERE key = ?1",
                        rusqlite::params![key, value, stamp],
                    )?;
                }
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ repairs

/// Re-checks what the schema cannot express. Each repair is deterministic (it depends only on
/// the data), so every device makes the same one, and none of them stamps a new `updated_at`.
fn repair(cx: &mut Cx) -> Result<()> {
    clear_dangling_pointers(cx)?;
    drop_dangling_edges(cx)?;
    break_loops(cx)?;
    break_team_loops(cx)?;
    Ok(())
}

fn clear_dangling_pointers(cx: &mut Cx) -> Result<()> {
    let fixes: [(&str, &str, &str, &str); 3] = [
        (
            "UPDATE tasks SET project_id = NULL
             WHERE project_id IS NOT NULL AND project_id NOT IN (SELECT id FROM projects)",
            "task",
            "tasks",
            "belonged to a project that was deleted on another device; they are now in the inbox",
        ),
        (
            "UPDATE projects SET owner_person_id = NULL
             WHERE owner_person_id IS NOT NULL AND owner_person_id NOT IN (SELECT id FROM people)",
            "project",
            "projects",
            "were owned by a person who was deleted on another device; they have no owner now",
        ),
        (
            "UPDATE teams SET parent_team_id = NULL
             WHERE parent_team_id IS NOT NULL AND parent_team_id NOT IN (SELECT id FROM teams)",
            "team",
            "teams",
            "were inside a team that was deleted on another device; they are top-level now",
        ),
    ];
    for (sql, _, noun, what) in fixes {
        let n = cx.tx.execute(sql, [])?;
        if n > 0 {
            cx.summary.repairs.push(format!("{n} {noun} {what}"));
        }
    }
    // A waiting-on needs a person; the one asked was deleted, so it falls back to "me".
    let me: Option<String> = cx
        .tx
        .query_row("SELECT id FROM people WHERE is_self = 1", [], |r| r.get(0))
        .ok();
    if let Some(me) = me {
        let n = cx.tx.execute(
            "UPDATE waiting_on SET person_id = ?1 WHERE person_id NOT IN (SELECT id FROM people)",
            [&me],
        )?;
        if n > 0 {
            cx.summary.repairs.push(format!(
                "{n} waiting-on items were about a person who was deleted on another device; they now point at you"
            ));
        }
    }
    Ok(())
}

fn drop_dangling_edges(cx: &mut Cx) -> Result<()> {
    let mut removed = 0;
    for node_type in NodeType::ALL {
        let t = table(*node_type);
        for (type_col, id_col) in [("from_type", "from_id"), ("to_type", "to_id")] {
            removed += cx.tx.execute(
                &format!(
                    "DELETE FROM edges WHERE {type_col} = ?1 AND {id_col} NOT IN (SELECT id FROM {t})"
                ),
                [node_type.as_str()],
            )?;
        }
    }
    if removed > 0 {
        cx.summary.repairs.push(format!(
            "{removed} links pointed at items that were deleted on another device and were removed"
        ));
    }
    Ok(())
}

/// Relations that must stay acyclic, with the node type at both ends.
const ACYCLIC: [(&str, NodeType); 4] = [
    ("blocks", NodeType::Task),
    ("depends_on", NodeType::Project),
    ("supersedes", NodeType::Decision),
    ("reports_to", NodeType::Person),
];

fn break_loops(cx: &mut Cx) -> Result<()> {
    for (edge_type, node_type) in ACYCLIC {
        let rows: Vec<(String, String, String, String)> = {
            let mut stmt = cx.tx.prepare(
                "SELECT id, from_id, to_id, updated_at FROM edges
                 WHERE edge_type = ?1 AND archived_at IS NULL",
            )?;
            let mapped = stmt.query_map([edge_type], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
            mapped.collect::<rusqlite::Result<_>>()?
        };
        let links: Vec<LoopLink> = rows
            .iter()
            .filter_map(|(_, from, to, stamp)| {
                Some(LoopLink {
                    from: Uuid::parse_str(from).ok()?,
                    to: Uuid::parse_str(to).ok()?,
                    stamp: stamp.clone(),
                    // The same on every device, whatever id the edge was given.
                    key: format!("{from}>{to}"),
                })
            })
            .collect();
        for i in links_to_drop(&links) {
            let (id, from, to, _) = &rows[i];
            // Archived "at" its own timestamp: the same value on every device.
            cx.tx.execute(
                "UPDATE edges SET archived_at = updated_at WHERE id = ?1",
                [id],
            )?;
            cx.summary.repairs.push(format!(
                "Two devices together made a loop in \"{edge_type}\" links; the newest link, {} → {}, was removed",
                label(cx.tx, node_type, from),
                label(cx.tx, node_type, to)
            ));
        }
    }
    Ok(())
}

fn break_team_loops(cx: &mut Cx) -> Result<()> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = cx.tx.prepare(
            "SELECT id, parent_team_id, updated_at FROM teams
             WHERE parent_team_id IS NOT NULL AND archived_at IS NULL",
        )?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        mapped.collect::<rusqlite::Result<_>>()?
    };
    let links: Vec<LoopLink> = rows
        .iter()
        .filter_map(|(id, parent, stamp)| {
            Some(LoopLink {
                from: Uuid::parse_str(id).ok()?,
                to: Uuid::parse_str(parent).ok()?,
                stamp: stamp.clone(),
                key: id.clone(),
            })
        })
        .collect();
    for i in links_to_drop(&links) {
        let (id, _, _) = &rows[i];
        cx.tx
            .execute("UPDATE teams SET parent_team_id = NULL WHERE id = ?1", [id])?;
        cx.summary.repairs.push(format!(
            "Two devices together put teams inside each other; \"{}\" is now a top-level team",
            label(cx.tx, NodeType::Team, id)
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
