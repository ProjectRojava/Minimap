//! Meetings (spec 38): making and moving them, following them up, and letting the clock move
//! them on. The rules are `minimap_core::meeting`; this is the part that writes.
//!
//! A meeting is a task of the built-in type, so everything here goes through the task functions
//! and is logged, undoable (apart from the clock's own moves, which the app does not put on the
//! undo stack) and exported like any other change.

use minimap_core::meeting;
use minimap_types::{
    AssigneeChoice, Clock, CreateTask, Date, EdgeType, NewEdge, NodeRef, NodeType, Patch, Task,
    TaskStatus, UpdateTask, Uuid, MEETING_TYPE,
};
use rusqlite::Connection;

use crate::{
    edges,
    error::{Result, StoreError},
    nodes, tasks,
};

/// Moves every meeting on to where `clock` says it has got to: in progress once it has started,
/// done once it has ended (a meeting that ended while the app was closed goes straight to done,
/// finished when it ended). All or nothing; the number of meetings that moved. A repeating
/// meeting that finishes makes the next one, as finishing any repeating task does.
pub fn advance(conn: &mut Connection, clock: Clock) -> Result<u32> {
    let tx = conn.transaction()?;
    let due: Vec<(Task, TaskStatus)> = tasks::list(&tx, false)?
        .into_iter()
        .filter_map(|t| meeting::next_status(&t, &clock).map(|next| (t, next)))
        .collect();
    let mut moved = 0;
    for (task, next) in due {
        let finished_at = (next == TaskStatus::Done)
            .then(|| meeting::ended_at(&task, clock.utc_offset_minutes))
            .flatten();
        tasks::update_in_tx_at(
            &tx,
            task.id,
            UpdateTask {
                status: Some(next),
                ..Default::default()
            },
            finished_at,
        )?;
        moved += 1;
    }
    tx.commit()?;
    Ok(moved)
}

/// A new meeting.
#[allow(clippy::too_many_arguments)]
pub fn create(
    conn: &mut Connection,
    title: String,
    due: Date,
    start_minute: u16,
    length_minutes: Option<u32>,
    project_id: Option<Uuid>,
    assignee: AssigneeChoice,
) -> Result<Task> {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: Some(MEETING_TYPE.to_owned()),
            focus: None,
            start_minute: Some(start_minute),
            length_minutes,
            title,
            assignee,
            description: String::new(),
            project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(due),
            priority: None,
            recurrence: None,
        },
    )
}

/// Makes a task a meeting at this time, or moves a meeting. `now` is the clock where the user
/// is: a meeting that was started or done and is moved to a time that has not come is open again.
pub fn set_time(
    conn: &mut Connection,
    id: Uuid,
    due: Date,
    start_minute: u16,
    length_minutes: Option<u32>,
    now: Clock,
) -> Result<Task> {
    let tx = conn.transaction()?;
    let old = tasks::get(&tx, id)?;
    let moved = old.due_date != Some(due) || old.start_minute != Some(start_minute);
    let mut patch = UpdateTask {
        task_type: Patch::Set(MEETING_TYPE.to_owned()),
        due_date: Patch::Set(due),
        start_minute: Patch::Set(start_minute),
        length_minutes: match length_minutes {
            Some(l) => Patch::Set(l),
            None => Patch::Clear,
        },
        ..Default::default()
    };
    if moved {
        let mut would_be = old.clone();
        would_be.due_date = Some(due);
        would_be.start_minute = Some(start_minute);
        if meeting::reopens_when_moved(old.status, &would_be, &now) {
            patch.status = Some(TaskStatus::Todo);
        }
    }
    let task = tasks::update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(task)
}

/// The meeting a follow-up is made from, or the reason it can't be.
fn original(conn: &Connection, id: Uuid) -> Result<Task> {
    let task = tasks::get(conn, id)?;
    if !task.is_meeting() {
        return Err(StoreError::Invalid(format!(
            "“{}” is not a meeting, so it can't have a follow-up.",
            task.title
        )));
    }
    if task.archived_at.is_some() {
        return Err(StoreError::Invalid(
            "an archived meeting can't have a follow-up".into(),
        ));
    }
    Ok(task)
}

/// A new meeting that follows up on `source`, in one transaction (one undo step): it takes the
/// source's project, priority, assignee, objectives and reference links, a title that starts
/// "Follow-up:", and the length of the source unless one is given.
pub fn create_follow_up(
    conn: &mut Connection,
    source: Uuid,
    due: Date,
    start_minute: u16,
    length_minutes: Option<u32>,
) -> Result<Task> {
    let tx = conn.transaction()?;
    let from = original(&tx, source)?;
    let links = edges::list_for_node(&tx, source, false)?;
    let assignee = links
        .iter()
        .find(|e| e.edge_type == EdgeType::AssignedTo && e.from_id == source)
        .map_or(AssigneeChoice::Nobody, |e| AssigneeChoice::Person(e.to_id));
    let created = tasks::create_in_tx(
        &tx,
        CreateTask {
            links: from.links.clone(),
            task_type: Some(MEETING_TYPE.to_owned()),
            focus: None,
            start_minute: Some(start_minute),
            length_minutes: Some(length_minutes.unwrap_or_else(|| from.meeting_minutes())),
            title: meeting::follow_up_title(&from.title),
            assignee,
            description: String::new(),
            project_id: from.project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(due),
            priority: Some(from.priority),
            recurrence: None,
        },
    )?;
    for link in links.iter().filter(|e| {
        e.edge_type == EdgeType::ContributesTo
            && e.from_id == source
            && nodes::archived_at(&tx, NodeRef::new(e.to_type, e.to_id))
                .map(|a| a.is_none())
                .unwrap_or(false)
    }) {
        edges::add_in_tx(
            &tx,
            NewEdge {
                edge_type: link.edge_type,
                from: NodeRef::new(NodeType::Task, created.id),
                to: NodeRef::new(link.to_type, link.to_id),
                attrs: link.attrs.clone(),
            },
        )?;
    }
    edges::add_in_tx(
        &tx,
        NewEdge {
            edge_type: EdgeType::FollowsUp,
            from: NodeRef::new(NodeType::Task, created.id),
            to: NodeRef::new(NodeType::Task, source),
            attrs: serde_json::json!({}),
        },
    )?;
    tx.commit()?;
    Ok(created)
}

/// May `follow_up` follow up on `original`? Both are meetings, a meeting follows up on one
/// meeting, and the chain doesn't loop; the refusal names the meetings. Reads the live links, so
/// inside a transaction it also sees the ones before it.
pub fn check_follow_up(conn: &Connection, follow_up: Uuid, original: Uuid) -> Result<()> {
    let (a, b) = (tasks::get(conn, follow_up)?, tasks::get(conn, original)?);
    for t in [&a, &b] {
        if !t.is_meeting() {
            return Err(StoreError::Invalid(format!(
                "“{}” is not a meeting. A follow-up links two meetings.",
                t.title
            )));
        }
    }
    let pairs: Vec<(Uuid, Uuid)> = edges::list_active_of_type(conn, EdgeType::FollowsUp)?
        .into_iter()
        .map(|e| (e.from_id, e.to_id))
        .collect();
    let Err(why) = meeting::check_follow_up(&pairs, follow_up, original) else {
        return Ok(());
    };
    let other = match why {
        meeting::FollowUpError::AlreadyFollows(o) => {
            nodes::summary(conn, NodeRef::new(NodeType::Task, o))
                .map(|s| s.label)
                .unwrap_or_default()
        }
        meeting::FollowUpError::Circle => String::new(),
    };
    Err(StoreError::Invalid(why.message(&a.title, &b.title, &other)))
}
