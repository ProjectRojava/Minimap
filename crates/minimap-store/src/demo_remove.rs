//! Removing the demo data (spec 24).
//!
//! What counts as demo data:
//! * data added by this version: [`demo::seed`] writes the ids of everything it made into a
//!   device-local `app_meta` row ([`meta::DEMO_ITEMS`]), so removal is exact;
//! * data added before that (no row): recognised by *title*. The seed is deterministic in its
//!   titles, so a scratch database is seeded in memory and the real database's items are matched
//!   to it by type and exact title. Fewer than half of the demo items found means "not demo
//!   data".
//!
//! "me" is never removed. Anything of the user's that points at a demo item is **kept and
//! detached**: tasks in a demo project move to the inbox, projects owned by a demo person lose
//! the owner, teams inside a demo team become top-level, and a demo person a waiting-on of the
//! user's is about stays (a waiting-on can't be without a person). Links between a demo item and
//! one of the user's go with the demo item.
//!
//! Removal is one transaction: every demo item is archived and then hard-deleted (tombstones
//! leave, as for any hard delete), its history is dropped, and the marker is cleared. The caller
//! takes a backup first.

use std::collections::HashSet;

use minimap_types::{
    DemoImpact, DemoSource, DemoStatus, DemoSummary, NodeRef, NodeType, Patch, UpdateProject,
    UpdateTask, UpdateTeam,
};
use rusqlite::{Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::{
    convert::id_s,
    demo,
    error::{Result, StoreError},
    meta, nodes, projects,
    repo::table,
    tasks, teams,
};

/// Delete order: whatever points at something goes before it (waiting-ons and tasks before
/// people and projects, projects before people, teams before people).
const ORDER: [NodeType; 8] = [
    NodeType::WaitingOn,
    NodeType::Task,
    NodeType::Note,
    NodeType::Decision,
    NodeType::Project,
    NodeType::Objective,
    NodeType::Team,
    NodeType::Person,
];

fn bad(e: impl std::fmt::Display) -> StoreError {
    StoreError::Invalid(e.to_string())
}

/// Ids (and labels) of every item of a type; "me" is left out. With `active_only`, archived
/// items are too.
fn labelled(
    conn: &Connection,
    node_type: NodeType,
    active_only: bool,
) -> Result<Vec<(Uuid, String)>> {
    let mut conditions = Vec::new();
    if node_type == NodeType::Person {
        conditions.push("is_self = 0");
    }
    if active_only {
        conditions.push("archived_at IS NULL");
    }
    let filter = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT id, {} FROM {}{filter}",
        nodes::label_column(node_type),
        table(node_type)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.map(|r| {
        let (id, label) = r?;
        Ok((Uuid::parse_str(&id).map_err(bad)?, label))
    })
    .collect()
}

/// Written by `seed` just before it commits: everything but "me" is demo data.
pub(crate) fn record(conn: &Connection) -> Result<()> {
    let mut items = Vec::new();
    for node_type in NodeType::ALL {
        for (id, _) in labelled(conn, *node_type, false)? {
            items.push(NodeRef::new(*node_type, id));
        }
    }
    meta::set(
        conn,
        meta::DEMO_ITEMS,
        &serde_json::to_string(&items).map_err(bad)?,
    )
}

/// The items the marker names that still exist.
fn recorded(conn: &Connection, json: &str) -> Result<Vec<NodeRef>> {
    let named: Vec<NodeRef> = serde_json::from_str(json).unwrap_or_default();
    let mut existing = HashSet::new();
    for node_type in NodeType::ALL {
        existing.extend(
            labelled(conn, *node_type, false)?
                .into_iter()
                .map(|(id, _)| id),
        );
    }
    Ok(named
        .into_iter()
        .filter(|n| existing.contains(&n.id))
        .collect())
}

/// Items whose type and exact title match the demo data's.
fn by_titles(conn: &Connection) -> Result<Vec<NodeRef>> {
    let mut scratch = crate::open_in_memory()?;
    demo::seed(&mut scratch, crate::today())?;
    let mut known = HashSet::new();
    for node_type in NodeType::ALL {
        for (_, title) in labelled(&scratch, *node_type, false)? {
            known.insert((*node_type, title));
        }
    }
    let mut found = Vec::new();
    for node_type in NodeType::ALL {
        for (id, title) in labelled(conn, *node_type, true)? {
            if known.contains(&(*node_type, title)) {
                found.push(NodeRef::new(*node_type, id));
            }
        }
    }
    // A few titles in common is a coincidence, not the demo data.
    Ok(if found.len() * 2 < known.len() {
        Vec::new()
    } else {
        found
    })
}

fn pairs(conn: &Connection, sql: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What removal would do, worked out without changing anything.
#[derive(Default)]
struct Plan {
    items: Vec<NodeRef>,
    source: Option<DemoSource>,
    tasks_to_inbox: Vec<Uuid>,
    owners_cleared: Vec<Uuid>,
    teams_unparented: Vec<Uuid>,
    people_kept: u32,
    links: u32,
    your_links: u32,
}

fn plan(conn: &Connection) -> Result<Plan> {
    let (mut items, source) = match meta::get(conn, meta::DEMO_ITEMS)? {
        Some(json) => (recorded(conn, &json)?, DemoSource::Recorded),
        None => (by_titles(conn)?, DemoSource::Titles),
    };
    if items.is_empty() {
        return Ok(Plan::default());
    }
    let mut set: HashSet<String> = items.iter().map(|n| id_s(n.id)).collect();
    // Everything that is demo data, kept people included: a link to one of these is not "yours".
    let demo_all = set.clone();

    // A waiting-on of the user's needs its person: that demo person stays.
    let mut kept = HashSet::new();
    for (waiting, person) in pairs(conn, "SELECT id, person_id FROM waiting_on")? {
        if !set.contains(&waiting) && set.contains(&person) {
            kept.insert(person);
        }
    }
    items.retain(|n| !kept.contains(&id_s(n.id)));
    set.retain(|id| !kept.contains(id));

    let ids = |sql: &str| -> Result<Vec<Uuid>> {
        pairs(conn, sql)?
            .into_iter()
            .filter(|(own, pointee)| !set.contains(own) && set.contains(pointee))
            .map(|(own, _)| Uuid::parse_str(&own).map_err(bad))
            .collect()
    };
    let tasks_to_inbox = ids("SELECT id, project_id FROM tasks WHERE project_id IS NOT NULL")?;
    let owners_cleared =
        ids("SELECT id, owner_person_id FROM projects WHERE owner_person_id IS NOT NULL")?;
    let teams_unparented =
        ids("SELECT id, parent_team_id FROM teams WHERE parent_team_id IS NOT NULL")?;

    // "me" is the user, not a thing of theirs that happens to be linked: links to me (a task
    // assigned to me) are not worth warning about.
    let me: Option<String> = conn
        .query_row("SELECT id FROM people WHERE is_self = 1", [], |r| r.get(0))
        .optional()?;
    let (mut links, mut your_links) = (0, 0);
    for (from, to) in pairs(conn, "SELECT from_id, to_id FROM edges")? {
        let (from_in, to_in) = (set.contains(&from), set.contains(&to));
        if from_in || to_in {
            links += 1;
        }
        if from_in != to_in {
            let other = if from_in { &to } else { &from };
            if !demo_all.contains(other) && me.as_deref() != Some(other.as_str()) {
                your_links += 1;
            }
        }
    }
    Ok(Plan {
        items,
        source: Some(source),
        tasks_to_inbox,
        owners_cleared,
        teams_unparented,
        people_kept: kept.len() as u32,
        links,
        your_links,
    })
}

impl Plan {
    fn summary(&self) -> DemoSummary {
        let n = |t: NodeType| self.items.iter().filter(|i| i.node_type == t).count() as u32;
        DemoSummary {
            objectives: n(NodeType::Objective),
            projects: n(NodeType::Project),
            tasks: n(NodeType::Task),
            people: n(NodeType::Person),
            teams: n(NodeType::Team),
            notes: n(NodeType::Note),
            decisions: n(NodeType::Decision),
            waiting_ons: n(NodeType::WaitingOn),
            links: self.links,
        }
    }

    fn impact(&self) -> DemoImpact {
        DemoImpact {
            your_links: self.your_links,
            tasks_to_inbox: self.tasks_to_inbox.len() as u32,
            owners_cleared: self.owners_cleared.len() as u32,
            teams_unparented: self.teams_unparented.len() as u32,
            people_kept: self.people_kept,
        }
    }
}

/// Is there demo data, and what would removing it do? Changes nothing.
pub fn status(conn: &Connection) -> Result<DemoStatus> {
    let plan = plan(conn)?;
    Ok(DemoStatus {
        found: !plan.items.is_empty(),
        source: plan.source.filter(|_| !plan.items.is_empty()),
        items: plan.summary(),
        impact: plan.impact(),
    })
}

/// Removes the demo data in one transaction (all or nothing). Refuses when there is none.
pub fn remove(conn: &mut Connection) -> Result<(DemoSummary, DemoImpact)> {
    let plan = plan(conn)?;
    if plan.items.is_empty() {
        return Err(StoreError::Invalid(
            "There is no demo data to remove.".into(),
        ));
    }
    let tx = conn.transaction()?;
    detach(&tx, &plan)?;
    tx.execute_batch("CREATE TEMP TABLE demo_ids (id TEXT PRIMARY KEY)")?;
    for node in &plan.items {
        tx.execute("INSERT INTO demo_ids (id) VALUES (?1)", [id_s(node.id)])?;
    }
    // Teams inside teams: all of them go, so unhook the parents before the first delete.
    tx.execute(
        "UPDATE teams SET parent_team_id = NULL WHERE id IN (SELECT id FROM demo_ids)",
        [],
    )?;
    for node_type in ORDER {
        for node in plan.items.iter().filter(|n| n.node_type == node_type) {
            // The rule for every hard delete: archive first.
            if nodes::archived_at(&tx, *node)?.is_none() {
                nodes::archive_in_tx(&tx, *node)?;
            }
            nodes::delete_in_tx(&tx, *node)?;
        }
    }
    // No trace of the demo data is left in the history either.
    tx.execute(
        "DELETE FROM activity WHERE node_id IN (SELECT id FROM demo_ids)",
        [],
    )?;
    tx.execute_batch("DROP TABLE demo_ids")?;
    meta::remove(&tx, meta::DEMO_ITEMS)?;
    tx.commit()?;
    Ok((plan.summary(), plan.impact()))
}

/// Unhooks the user's own items from the demo items that are about to go.
fn detach(tx: &Transaction, plan: &Plan) -> Result<()> {
    for id in &plan.tasks_to_inbox {
        let patch = UpdateTask {
            project_id: Patch::Clear,
            ..Default::default()
        };
        tasks::update_in_tx(tx, *id, patch)?;
    }
    for id in &plan.owners_cleared {
        let patch = UpdateProject {
            owner_person_id: Patch::Clear,
            ..Default::default()
        };
        projects::update_in_tx(tx, *id, patch)?;
    }
    for id in &plan.teams_unparented {
        let patch = UpdateTeam {
            parent_team_id: Patch::Clear,
            ..Default::default()
        };
        teams::update_in_tx(tx, *id, patch)?;
    }
    Ok(())
}
