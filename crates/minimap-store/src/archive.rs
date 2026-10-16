//! What has been archived: one list of everything, whatever it is (tasks, projects, objectives,
//! people, teams, notes, decisions, waiting-ons), for the Archive screen; and the finished tasks
//! that were archived, which the weekly review still counts.

use std::collections::HashMap;

use minimap_types::{
    ArchivedItem, Edge, EdgeType, NodeRef, NodeSummary, NodeType, TaskRow, TaskStatus,
};
use rusqlite::Connection;
use time::Date;
use uuid::Uuid;

use crate::{
    convert::col_uuid,
    edges,
    error::{Result, StoreError},
    nodes, projects,
    repo::table,
    tasks,
};

/// Every archived item, newest archive first. A task says what state it was left in and which
/// project it belonged to.
pub fn list_all(conn: &Connection) -> Result<Vec<ArchivedItem>> {
    let project_names: HashMap<Uuid, String> = projects::list(conn, true)?
        .into_iter()
        .map(|p| (p.id, p.title))
        .collect();
    let mut out = Vec::new();
    for &node_type in NodeType::ALL {
        let label = nodes::label_column(node_type);
        let mut stmt = conn.prepare(&format!(
            "SELECT id, {label}, archived_at FROM {} WHERE archived_at IS NOT NULL",
            table(node_type)
        ))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((col_uuid(r, 0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, name, archived_at) in rows {
            let detail = if node_type == NodeType::Task {
                let t = tasks::get(conn, id)?;
                let project = t
                    .project_id
                    .and_then(|p| project_names.get(&p))
                    .map(String::as_str);
                match project {
                    Some(p) => format!("{} · {p}", t.status.as_str().replace('_', " ")),
                    None => t.status.as_str().replace('_', " "),
                }
            } else {
                String::new()
            };
            let archived_on: Date = archived_at
                .get(..10)
                .and_then(|d| minimap_types::timefmt::parse_date(d).ok())
                .ok_or_else(|| StoreError::Invalid(format!("bad archive time {archived_at}")))?;
            out.push(ArchivedItem {
                node: NodeSummary {
                    node: NodeRef::new(node_type, id),
                    label: name,
                    archived: true,
                },
                archived_on,
                archived_at,
                detail,
            });
        }
    }
    // Newest first; the full time breaks ties so an automatic sweep keeps its order.
    out.sort_by(|a, b| b.archived_at.cmp(&a.archived_at));
    Ok(out)
}

/// The finished tasks that were archived (by hand or by the sweep) and were finished from `from`
/// to `to`, as rows like [`views::task_rows`] makes them, for the weekly review: archiving a task
/// doesn't take it out of the week it was done in.
///
/// [`views::task_rows`]: crate::views::task_rows
pub fn done_rows_between(conn: &Connection, from: Date, to: Date) -> Result<Vec<TaskRow>> {
    let names: HashMap<Uuid, String> = projects::list(conn, true)?
        .into_iter()
        .map(|p| (p.id, p.title))
        .collect();
    let people: HashMap<Uuid, String> = crate::people::list(conn, true)?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    let mut rows = Vec::new();
    for task in tasks::list(conn, true)? {
        if task.archived_at.is_none() || task.status != TaskStatus::Done {
            continue;
        }
        let Some(done) = task.completed_at.map(|c| c.date()) else {
            continue;
        };
        if done < from || done > to {
            continue;
        }
        // Archiving a task archives its links too, so the assignee is read from those.
        let assignee = edges::list_for_node(conn, task.id, true)?
            .into_iter()
            .find(|e: &Edge| e.edge_type == EdgeType::AssignedTo && e.from_id == task.id)
            .and_then(|e| people.get(&e.to_id).map(|n| (e.to_id, n.clone())));
        rows.push(TaskRow {
            project: task.project_id.and_then(|p| {
                names.get(&p).map(|n| NodeSummary {
                    node: NodeRef::new(NodeType::Project, p),
                    label: n.clone(),
                    archived: false,
                })
            }),
            assignee: assignee.map(|(id, name)| NodeSummary {
                node: NodeRef::new(NodeType::Person, id),
                label: name,
                archived: false,
            }),
            link_count: 0,
            attachment_count: 0,
            parent: None,
            subtask_count: 0,
            subtasks_done: 0,
            task,
        });
    }
    Ok(rows)
}

/// Brings an archived item back (with the links that were archived along with it).
pub fn restore(conn: &mut Connection, node: NodeRef) -> Result<()> {
    nodes::unarchive(conn, node)
}
