//! Finished tasks that are old enough are archived on their own, so Done doesn't fill up for ever.
//!
//! "Old enough" is Settings' `done_archive_days` after the day a task was finished (a meeting has
//! its own, shorter, `done_archive_meeting_days`; 0 = never). It is the ordinary archive, so it
//! can be undone and the task is restored from the Archive screen; one sweep is one transaction.
//! Some finished tasks are kept because archiving them would change something that is still live:
//!
//! * a sub-task whose parent is still open (the parent's "2 of 5 done" would shrink), and a
//!   parent that still has open sub-tasks;
//! * a task that blocks something still open (the "blocked by" list would lose it);
//! * a task with a live ping (someone asked to come back to it).

use std::collections::{HashMap, HashSet};

use minimap_types::{EdgeType, NodeRef, NodeType, TaskStatus};
use rusqlite::Connection;
use time::Date;
use uuid::Uuid;

use crate::{edges, error::Result, nodes, pings, settings, tasks};

/// Archives the finished tasks that are old enough on `today`; returns their ids (none when the
/// settings say never, or nothing is old enough).
pub fn archive_old_done(conn: &mut Connection, today: Date) -> Result<Vec<Uuid>> {
    let s = settings::get(conn)?;
    if s.done_archive_days == 0 && s.done_archive_meeting_days == 0 {
        return Ok(Vec::new());
    }
    let all = tasks::list(conn, false)?;
    let status: HashMap<Uuid, TaskStatus> = all.iter().map(|t| (t.id, t.status)).collect();
    let open = |id: &Uuid| {
        status
            .get(id)
            .is_some_and(|s| !matches!(s, TaskStatus::Done | TaskStatus::Cancelled))
    };

    let mut parent_of: HashMap<Uuid, Uuid> = HashMap::new();
    let mut has_open_child: HashSet<Uuid> = HashSet::new();
    for e in edges::list_active_of_type(conn, EdgeType::SubtaskOf)? {
        // child -> parent
        parent_of.insert(e.from_id, e.to_id);
        if open(&e.from_id) {
            has_open_child.insert(e.to_id);
        }
    }
    let mut blocks_open: HashSet<Uuid> = HashSet::new();
    for e in edges::list_active_of_type(conn, EdgeType::Blocks)? {
        // blocker -> blocked
        if open(&e.to_id) {
            blocks_open.insert(e.from_id);
        }
    }

    let mut doomed = Vec::new();
    for t in all.iter().filter(|t| t.status == TaskStatus::Done) {
        let Some(finished) = t.completed_at.map(|c| c.date()) else {
            continue;
        };
        let after = if t.is_meeting() {
            s.done_archive_meeting_days
        } else {
            s.done_archive_days
        };
        if after == 0 || (today - finished).whole_days() < i64::from(after) {
            continue;
        }
        let kept = parent_of.get(&t.id).is_some_and(open)
            || has_open_child.contains(&t.id)
            || blocks_open.contains(&t.id)
            || pings::live_for(conn, t.id)?.is_some();
        if !kept {
            doomed.push(t.id);
        }
    }
    if doomed.is_empty() {
        return Ok(doomed);
    }
    let tx = conn.transaction()?;
    for id in &doomed {
        nodes::archive_in_tx(&tx, NodeRef::new(NodeType::Task, *id))?;
    }
    tx.commit()?;
    Ok(doomed)
}
