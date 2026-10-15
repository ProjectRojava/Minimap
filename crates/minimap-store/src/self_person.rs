//! One "me" across every device (spec 22), and folding one person into another.
//!
//! Who "me" is is the synced setting `self_person_id`, once someone has chosen (by merging a
//! duplicate "me" into the right person); `people.is_self` on each device is only a copy of the
//! answer, kept for the unique index and the queries that already read it. When a merge finds
//! more than one "me" (each device made its own on first run), the setting decides; with no
//! setting, the oldest person wins, the same on every device whatever order they merge in.
//! Every other "me" is *folded* into the winner: whatever pointed at it moves across and it is
//! deleted for good, so the duplicate never comes back from another device.

use minimap_types::{NodeRef, NodeType};
use rusqlite::{params, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::{
    convert::{id_s, now, ts_s},
    error::{Result, StoreError},
    nodes, settings,
};

/// Text that can hold a mention of a person, as `@[Name](node:<id>)`.
const MENTION_COLUMNS: [(&str, &str); 9] = [
    ("notes", "body"),
    ("tasks", "description"),
    ("projects", "description"),
    ("objectives", "description"),
    ("decisions", "context"),
    ("decisions", "decision"),
    ("decisions", "rationale"),
    ("teams", "description"),
    ("people", "notes"),
];

/// A duplicate "me" that was folded into the real one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Folded {
    pub name: String,
    /// How many links, owners, notes and the like moved across.
    pub moved: usize,
}

/// What [`settle`] did.
#[derive(Debug, Default)]
pub(crate) struct Settled {
    pub folded: Vec<Folded>,
    /// "Me" had been archived (on another device) and was restored.
    pub restored: bool,
}

fn parse(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|e| StoreError::Invalid(format!("malformed person id: {e}")))
}

fn exists(tx: &Transaction, id: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM people WHERE id = ?1)",
        [id],
        |r| r.get(0),
    )?)
}

/// Makes this database have exactly one "me", the one the whole set of devices agrees on.
/// `remote_selves` are the people the other device being merged had as "me".
pub(crate) fn settle(tx: &Transaction, remote_selves: &[String]) -> Result<Settled> {
    // Everyone who is, or on the other device was, "me": (archived, created, id), so that sorting
    // puts anyone who is not archived first, then the oldest. An archived "me" is a duplicate that
    // somebody archived, so it only wins when there is nobody else.
    let mut claimants: Vec<(bool, String, String)> = tx
        .prepare("SELECT archived_at IS NOT NULL, created_at, id FROM people WHERE is_self = 1")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for id in remote_selves {
        let found: Option<(bool, String)> = tx
            .query_row(
                "SELECT archived_at IS NOT NULL, created_at FROM people WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((archived, created)) = found {
            claimants.push((archived, created, id.clone()));
        }
    }
    claimants.sort();
    claimants.dedup();

    // The setting counts only while the person it names is here.
    let named = match settings::self_person_id(tx)? {
        Some(id) if exists(tx, &id.to_string())? => Some(id),
        _ => None,
    };
    let me = match (named, claimants.first()) {
        (Some(id), _) => id,
        // Nobody has chosen: the oldest that is not archived, which every device works out the
        // same way.
        (None, Some((_, _, id))) => parse(id)?,
        (None, None) => return Ok(Settled::default()),
    };
    let me_s = id_s(me);

    tx.execute(
        "UPDATE people SET is_self = 0 WHERE is_self = 1 AND id != ?1",
        [&me_s],
    )?;
    tx.execute(
        "UPDATE people SET is_self = 1 WHERE id = ?1 AND is_self = 0",
        [&me_s],
    )?;
    // "Me" can't be archived. A merge can bring in an archived copy (the other device had it as
    // an ordinary duplicate and archived it); that is put right as a new edit, so it reaches the
    // other devices too, and the links that went with it come back.
    let node = NodeRef::new(NodeType::Person, me);
    let restored = nodes::archived_at(tx, node)?.is_some();
    if restored {
        nodes::unarchive_in_tx(tx, node)?;
    }

    let mut folded = Vec::new();
    for (_, _, id) in &claimants {
        if *id == me_s {
            continue;
        }
        let name: String = tx.query_row("SELECT name FROM people WHERE id = ?1", [id], |r| {
            r.get(0)
        })?;
        let moved = fold(tx, parse(id)?, me)?;
        folded.push(Folded { name, moved });
    }
    Ok(Settled { folded, restored })
}

/// Moves everything that points at `from` onto `into` and deletes `from` for good (leaving a
/// tombstone, so it stays gone on the other devices). Returns how many things moved. Edges that
/// would then be the same twice, or would join a person to themselves, are dropped instead.
pub(crate) fn fold(tx: &Transaction, from: Uuid, into: Uuid) -> Result<usize> {
    let (f, i) = (id_s(from), id_s(into));
    let at = ts_s(now());
    let mut moved = 0;

    tx.execute(
        "DELETE FROM edges
         WHERE (from_id = ?1 OR to_id = ?1)
           AND ((from_id IN (?1, ?2) AND to_id IN (?1, ?2))
                OR EXISTS (
                  SELECT 1 FROM edges e
                  WHERE e.id != edges.id AND e.edge_type = edges.edge_type
                    AND e.from_id = CASE WHEN edges.from_id = ?1 THEN ?2 ELSE edges.from_id END
                    AND e.to_id   = CASE WHEN edges.to_id   = ?1 THEN ?2 ELSE edges.to_id   END))",
        params![f, i],
    )?;
    moved += tx.execute(
        "UPDATE edges SET
           from_id = CASE WHEN from_id = ?1 THEN ?2 ELSE from_id END,
           to_id   = CASE WHEN to_id   = ?1 THEN ?2 ELSE to_id   END,
           updated_at = ?3
         WHERE from_id = ?1 OR to_id = ?1",
        params![f, i, at],
    )?;
    moved += tx.execute(
        "UPDATE projects SET owner_person_id = ?2, updated_at = ?3 WHERE owner_person_id = ?1",
        params![f, i, at],
    )?;
    moved += tx.execute(
        "UPDATE waiting_on SET person_id = ?2, updated_at = ?3 WHERE person_id = ?1",
        params![f, i, at],
    )?;
    moved += tx.execute(
        "UPDATE pings SET target_id = ?2, updated_at = ?3
         WHERE target_type = 'person' AND target_id = ?1",
        params![f, i, at],
    )?;
    moved += tx.execute(
        "UPDATE attachments SET node_id = ?2, updated_at = ?3
         WHERE node_type = 'person' AND node_id = ?1",
        params![f, i, at],
    )?;
    for (table, column) in MENTION_COLUMNS {
        moved += tx.execute(
            &format!(
                "UPDATE {table} SET {column} = REPLACE({column}, ?1, ?2), updated_at = ?3
                 WHERE instr({column}, ?1) > 0"
            ),
            params![f, i, at],
        )?;
    }

    tx.execute("DELETE FROM people WHERE id = ?1", [&f])?;
    tx.execute(
        "INSERT INTO tombstones (kind, id, deleted_at) VALUES ('person', ?1, ?2)
         ON CONFLICT(kind, id) DO UPDATE SET deleted_at = MAX(deleted_at, excluded.deleted_at)",
        params![f, at],
    )?;
    Ok(moved)
}
