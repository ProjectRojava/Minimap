use minimap_types::{
    ActivityAction, AssigneeChoice, CreateTask, EdgeType, NewEdge, NodeRef, NodeType, Recurrence,
    Task, TaskStatus, UpdateTask, DEFAULT_PRIORITY,
};
use rusqlite::{params, Connection, Row, Transaction};
use uuid::Uuid;

use crate::{
    activity,
    convert::*,
    edges,
    error::{Result, StoreError},
    nodes, people,
    repo::{fetch, fetch_all},
};

const TABLE: &str = "tasks";
const COLS: &str = "id, title, description, project_id, status, estimate_days, start_date, due_date, completed_at, priority, created_at, updated_at, archived_at, recurrence";

fn from_row(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        project_id: col_uuid_opt(r, 3)?,
        status: col_enum(r, 4)?,
        estimate_days: r.get(5)?,
        start_date: col_date_opt(r, 6)?,
        due_date: col_date_opt(r, 7)?,
        completed_at: col_ts_opt(r, 8)?,
        priority: r.get(9)?,
        created_at: col_ts(r, 10)?,
        updated_at: col_ts(r, 11)?,
        archived_at: col_ts_opt(r, 12)?,
        recurrence: col_recurrence(r, 13)?,
    })
}

fn validate(t: &Task) -> Result<()> {
    ensure_not_blank("title", &t.title)?;
    ensure_priority(t.priority)?;
    match t.estimate_days {
        Some(e) if !e.is_finite() || e < 0.0 => Err(StoreError::Invalid(
            "estimate_days must be a non-negative number".into(),
        )),
        _ => Ok(()),
    }?;
    if let Some(rule) = &t.recurrence {
        minimap_core::recurrence::validate(rule).map_err(StoreError::Invalid)?;
    }
    Ok(())
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Task> {
    fetch(conn, TABLE, COLS, NodeType::Task, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Task>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateTask) -> Result<Task> {
    let tx = conn.transaction()?;
    let task = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(task)
}

/// Creates all the tasks or none (one transaction). For pasted lists.
pub fn create_many(conn: &mut Connection, inputs: Vec<CreateTask>) -> Result<Vec<Task>> {
    let tx = conn.transaction()?;
    let tasks = inputs
        .into_iter()
        .map(|input| create_in_tx(&tx, input))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;
    Ok(tasks)
}

/// The task, its assignee link (`Me` = the self person, if there is one) and their activity rows.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateTask) -> Result<Task> {
    let assignee = match input.assignee {
        AssigneeChoice::Me => people::get_self(tx)?.map(|p| p.id),
        AssigneeChoice::Nobody => None,
        AssigneeChoice::Person(id) => Some(id),
    };
    let at = now();
    let status = input.status.unwrap_or(TaskStatus::Todo);
    let t = Task {
        id: Uuid::now_v7(),
        title: input.title,
        description: input.description,
        project_id: input.project_id,
        status,
        estimate_days: input.estimate_days,
        start_date: input.start_date,
        due_date: input.due_date,
        completed_at: (status == TaskStatus::Done).then_some(at),
        priority: input.priority.unwrap_or(DEFAULT_PRIORITY),
        recurrence: input.recurrence,
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    validate(&t)?;
    tx.execute(
        &format!(
            "INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"
        ),
        params![
            id_s(t.id),
            t.title,
            t.description,
            id_opt_s(t.project_id),
            t.status.as_str(),
            t.estimate_days,
            date_s(t.start_date),
            date_s(t.due_date),
            ts_opt_s(t.completed_at),
            t.priority,
            ts_s(t.created_at),
            ts_s(t.updated_at),
            ts_opt_s(t.archived_at),
            recurrence_s(t.recurrence.as_ref()),
        ],
    )?;
    activity::record_created(tx, at, NodeType::Task, t.id, &t)?;
    if let Some(person) = assignee {
        edges::add_in_tx(tx, assigned_to(t.id, person))?;
    }
    Ok(t)
}

fn assigned_to(task: Uuid, person: Uuid) -> NewEdge {
    NewEdge {
        edge_type: EdgeType::AssignedTo,
        from: NodeRef::new(NodeType::Task, task),
        to: NodeRef::new(NodeType::Person, person),
        attrs: serde_json::json!({}),
    }
}

/// Makes `person` the task's only assignee (`None` unassigns). Replaces any current
/// assignment in one transaction; assigning the current assignee again changes nothing.
pub fn set_assignee(conn: &mut Connection, task: Uuid, person: Option<Uuid>) -> Result<()> {
    let tx = conn.transaction()?;
    get(&tx, task)?; // NotFound for unknown tasks
    let current: Vec<_> = edges::list_for_node(&tx, task, false)?
        .into_iter()
        .filter(|e| e.edge_type == EdgeType::AssignedTo && e.from_id == task)
        .collect();
    if let Some(p) = person {
        if current.len() == 1 && current[0].to_id == p {
            return Ok(());
        }
    }
    let at = now();
    for edge in &current {
        edges::archive_in_tx(&tx, edge, at)?;
    }
    if let Some(p) = person {
        edges::add_in_tx(&tx, assigned_to(task, p))?;
    }
    tx.commit()?;
    Ok(())
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateTask) -> Result<Task> {
    let tx = conn.transaction()?;
    let task = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(task)
}

/// Applies several updates or none (one transaction); one activity row per changed task.
pub fn update_many(conn: &mut Connection, updates: Vec<(Uuid, UpdateTask)>) -> Result<Vec<Task>> {
    let tx = conn.transaction()?;
    let tasks = updates
        .into_iter()
        .map(|(id, patch)| update_in_tx(&tx, id, patch))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;
    Ok(tasks)
}

/// [`update`] inside a caller's transaction.
pub(crate) fn update_in_tx(tx: &Transaction, id: Uuid, patch: UpdateTask) -> Result<Task> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    patch.apply(&mut new);
    // completed_at follows status: set on entering `done`, cleared on leaving it.
    let at = now();
    let finishing = old.status != TaskStatus::Done && new.status == TaskStatus::Done;
    if finishing {
        new.completed_at = Some(at);
    } else if old.status == TaskStatus::Done && new.status != TaskStatus::Done {
        new.completed_at = None;
    }
    validate(&new)?;
    // Finishing a repeating task hands its rule on to the next one (spec 27): this one no
    // longer repeats.
    let handed_on = if finishing {
        new.recurrence.take()
    } else {
        None
    };
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = at;
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, description=?3, project_id=?4, status=?5, estimate_days=?6, start_date=?7, due_date=?8, completed_at=?9, priority=?10, recurrence=?11, updated_at=?12 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.description,
            id_opt_s(new.project_id),
            new.status.as_str(),
            new.estimate_days,
            date_s(new.start_date),
            date_s(new.due_date),
            ts_opt_s(new.completed_at),
            new.priority,
            recurrence_s(new.recurrence.as_ref()),
            ts_s(new.updated_at),
        ],
    )?;
    activity::record(
        tx,
        at,
        NodeType::Task,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    if let Some(rule) = handed_on {
        make_next(tx, &new, rule)?;
    }
    Ok(new)
}

/// The task that follows a finished repeating one: the same title, description, project,
/// estimate, priority, assignee and objectives, due on the rule's next date (with the same gap
/// between start and due), and carrying the rule on. Blockers are not copied: they belong to the
/// instance they blocked.
fn make_next(tx: &Transaction, done: &Task, rule: Recurrence) -> Result<Task> {
    let due = minimap_core::recurrence::next_for_task(rule.cadence, done.due_date, today());
    let start = match (done.start_date, done.due_date) {
        (Some(start), Some(was_due)) => Some(due - (was_due - start)),
        _ => None,
    };
    // A project that has been archived since is not somewhere to put new work.
    let project_id = match done.project_id {
        Some(p) if nodes::archived_at(tx, NodeRef::new(NodeType::Project, p))?.is_none() => Some(p),
        _ => None,
    };
    let next = create_in_tx(
        tx,
        CreateTask {
            title: done.title.clone(),
            assignee: AssigneeChoice::Nobody,
            description: done.description.clone(),
            project_id,
            status: None,
            estimate_days: done.estimate_days,
            start_date: start,
            due_date: Some(due),
            priority: Some(done.priority),
            recurrence: Some(rule),
        },
    )?;
    for edge in edges::list_for_node(tx, done.id, false)? {
        let copy = edge.from_id == done.id
            && matches!(
                edge.edge_type,
                EdgeType::AssignedTo | EdgeType::ContributesTo
            );
        let target = NodeRef::new(edge.to_type, edge.to_id);
        if copy && nodes::archived_at(tx, target)?.is_none() {
            edges::add_in_tx(
                tx,
                NewEdge {
                    edge_type: edge.edge_type,
                    from: NodeRef::new(NodeType::Task, next.id),
                    to: target,
                    attrs: edge.attrs,
                },
            )?;
        }
    }
    Ok(next)
}
