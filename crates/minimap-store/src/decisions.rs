use minimap_types::{
    ActivityAction, CreateDecision, Decision, DecisionStatus, Edge, EdgeType, NewEdge, NodeRef,
    NodeType, Task, UpdateDecision, DECISION_ORIGIN_TASK, DECISION_TYPE,
};
use rusqlite::{params, Connection, Row, Transaction};
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

const TABLE: &str = "decisions";
const COLS: &str =
    "id, title, context, decision, rationale, decided_on, status, created_at, updated_at, archived_at, auto_status, auto_decided_on";

fn from_row(r: &Row) -> rusqlite::Result<Decision> {
    Ok(Decision {
        id: col_uuid(r, 0)?,
        title: r.get(1)?,
        context: r.get(2)?,
        decision: r.get(3)?,
        rationale: r.get(4)?,
        decided_on: col_date_opt(r, 5)?,
        status: col_enum(r, 6)?,
        created_at: col_ts(r, 7)?,
        updated_at: col_ts(r, 8)?,
        archived_at: col_ts_opt(r, 9)?,
        auto_status: r.get::<_, i64>(10)? != 0,
        auto_decided_on: r.get::<_, i64>(11)? != 0,
    })
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Decision> {
    fetch(conn, TABLE, COLS, NodeType::Decision, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Decision>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

pub fn create(conn: &mut Connection, input: CreateDecision) -> Result<Decision> {
    let tx = conn.transaction()?;
    let created = create_in_tx(&tx, input)?;
    tx.commit()?;
    Ok(created)
}

/// [`create`] inside a caller's transaction.
pub(crate) fn create_in_tx(tx: &Transaction, input: CreateDecision) -> Result<Decision> {
    let at = now();
    let d = Decision {
        id: Uuid::now_v7(),
        title: input.title,
        context: input.context,
        decision: input.decision,
        rationale: input.rationale,
        status: input.status.unwrap_or(DecisionStatus::Proposed),
        // Deciding something puts a date on it.
        decided_on: input
            .decided_on
            .or_else(|| (input.status == Some(DecisionStatus::Decided)).then(today)),
        auto_status: false,
        auto_decided_on: input.decided_on.is_none()
            && input.status == Some(DecisionStatus::Decided),
        created_at: at,
        updated_at: at,
        archived_at: None,
    };
    ensure_not_blank("title", &d.title)?;
    tx.execute(
        &format!("INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"),
        params![
            id_s(d.id),
            d.title,
            d.context,
            d.decision,
            d.rationale,
            date_s(d.decided_on),
            d.status.as_str(),
            ts_s(d.created_at),
            ts_s(d.updated_at),
            ts_opt_s(d.archived_at),
            i64::from(d.auto_status),
            i64::from(d.auto_decided_on),
        ],
    )?;
    activity::record_created(tx, at, NodeType::Decision, d.id, &d)?;
    Ok(d)
}

pub fn update(conn: &mut Connection, id: Uuid, patch: UpdateDecision) -> Result<Decision> {
    let tx = conn.transaction()?;
    let new = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(new)
}

/// Moving a decision to `decided` without a date stamps today (and says so: `auto_decided_on`).
/// A status or date changed here without the matching `auto_*` flag in the patch is the user's, so
/// its flag is cleared; Minimap's own fills (and undo) pass the flags.
pub(crate) fn update_in_tx(tx: &Transaction, id: Uuid, patch: UpdateDecision) -> Result<Decision> {
    let old = get(tx, id)?;
    let (flag_status, flag_date) = (patch.auto_status, patch.auto_decided_on);
    let mut new = old.clone();
    patch.apply(&mut new);
    if flag_status.is_none() && new.status != old.status {
        new.auto_status = false;
    }
    if flag_date.is_none() && new.decided_on != old.decided_on {
        new.auto_decided_on = false;
    }
    if new.status == DecisionStatus::Decided
        && old.status != DecisionStatus::Decided
        && new.decided_on.is_none()
    {
        new.decided_on = Some(today());
        new.auto_decided_on = true;
    }
    ensure_not_blank("title", &new.title)?;
    let diff = activity::diff(&old, &new)?;
    if diff.is_empty() {
        return Ok(old);
    }
    new.updated_at = now();
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, context=?3, decision=?4, rationale=?5, decided_on=?6, status=?7, updated_at=?8, auto_status=?9, auto_decided_on=?10 WHERE id=?1"
        ),
        params![
            id_s(id),
            new.title,
            new.context,
            new.decision,
            new.rationale,
            date_s(new.decided_on),
            new.status.as_str(),
            ts_s(new.updated_at),
            i64::from(new.auto_status),
            i64::from(new.auto_decided_on),
        ],
    )?;
    activity::record(
        tx,
        new.updated_at,
        NodeType::Decision,
        id,
        ActivityAction::Updated,
        &diff.into(),
    )?;
    Ok(new)
}

/// `new` replaces `old`: adds the `supersedes` link and marks `old` as superseded, in one
/// transaction (the matrix and loop checks are the caller's job).
pub fn supersede(conn: &mut Connection, new: Uuid, old: Uuid) -> Result<Edge> {
    let tx = conn.transaction()?;
    let edge = supersede_in_tx(&tx, new, old)?;
    tx.commit()?;
    Ok(edge)
}

/// [`supersede`] inside a caller's transaction.
pub(crate) fn supersede_in_tx(tx: &Transaction, new: Uuid, old: Uuid) -> Result<Edge> {
    let edge = edges::add_in_tx(
        tx,
        NewEdge {
            edge_type: EdgeType::Supersedes,
            from: NodeRef::new(NodeType::Decision, new),
            to: NodeRef::new(NodeType::Decision, old),
            attrs: serde_json::json!({}),
        },
    )?;
    let older = get(tx, old)?;
    if older.status != DecisionStatus::Superseded {
        update_in_tx(
            tx,
            old,
            UpdateDecision {
                status: Some(DecisionStatus::Superseded),
                ..Default::default()
            },
        )?;
    }
    Ok(edge)
}

/// The decision record of a task (spec 41): the active decision whose `affects` link to the task is
/// marked `origin: task`. It stays in the log when the task is finished; archiving the task
/// archives the link, so the record is then simply a decision with no task.
pub fn for_task(conn: &Connection, task: Uuid) -> Result<Option<Decision>> {
    for e in edges::list_for_node(conn, task, false)? {
        let marked = e.attrs.get("origin").and_then(|v| v.as_str()) == Some(DECISION_ORIGIN_TASK);
        if e.edge_type == EdgeType::Affects && e.to_id == task && marked {
            let d = get(conn, e.from_id)?;
            if d.archived_at.is_none() {
                return Ok(Some(d));
            }
        }
    }
    Ok(None)
}

/// A task of type Decision has a decision record: made now if it has none, and the existing one is
/// returned otherwise (so changing the type back and forth never makes a second). The record starts
/// `proposed`, titled like the task, with the task's description as its context, and affects the
/// task and its project.
pub(crate) fn ensure_for_task_in_tx(tx: &Transaction, task: &Task) -> Result<Decision> {
    if let Some(existing) = for_task(tx, task.id)? {
        return Ok(existing);
    }
    let d = create_in_tx(
        tx,
        CreateDecision {
            title: task.title.clone(),
            context: task.description.clone(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: None,
            status: None,
        },
    )?;
    let affects = |to: NodeRef, attrs: serde_json::Value| NewEdge {
        edge_type: EdgeType::Affects,
        from: NodeRef::new(NodeType::Decision, d.id),
        to,
        attrs,
    };
    edges::add_in_tx(
        tx,
        affects(
            NodeRef::new(NodeType::Task, task.id),
            serde_json::json!({ "origin": DECISION_ORIGIN_TASK }),
        ),
    )?;
    if let Some(project) = task.project_id {
        let project = NodeRef::new(NodeType::Project, project);
        if nodes::archived_at(tx, project)?.is_none() {
            edges::add_in_tx(tx, affects(project, serde_json::json!({})))?;
        }
    }
    Ok(d)
}

/// The "Create decision record" button for a task that was a Decision before records existed (or
/// whose record was archived). Refused for other tasks.
pub fn ensure_for_task(conn: &mut Connection, task: Uuid) -> Result<Decision> {
    let tx = conn.transaction()?;
    let t = tasks::get(&tx, task)?;
    if t.task_type.as_deref() != Some(DECISION_TYPE) || t.archived_at.is_some() {
        return Err(StoreError::Invalid(
            "Only an open task of type Decision has a decision record.".into(),
        ));
    }
    let d = ensure_for_task_in_tx(&tx, &t)?;
    tx.commit()?;
    Ok(d)
}

/// Both the decision and the reason are written (not blank).
fn is_written(d: &Decision) -> bool {
    !d.decision.trim().is_empty() && !d.rationale.trim().is_empty()
}

/// A task of type Decision can be finished only when its record says what was decided and why
/// (spec 42). A task with no record (it was typed before records existed, or the record was
/// archived) is not finished either: the *Finish decision* dialog makes the record.
pub(crate) fn check_can_finish(conn: &Connection, task: &Task) -> Result<()> {
    match for_task(conn, task.id)? {
        Some(d) if is_written(&d) => Ok(()),
        _ => Err(StoreError::DecisionIncomplete(format!(
            "Write what was decided and why before finishing \"{}\".",
            task.title
        ))),
    }
}

/// Finishing the task decides its record, in the same transaction: a *proposed* record becomes
/// `decided` today, marked automatic, and gets today's date unless it already has one (a date the
/// user gave stays theirs). A record the user already decided or replaced is left as it is.
pub(crate) fn decide_with_task_in_tx(tx: &Transaction, task: &Task) -> Result<()> {
    let Some(d) = for_task(tx, task.id)? else {
        return Ok(());
    };
    if d.status != DecisionStatus::Proposed {
        return Ok(());
    }
    let stamp = d.decided_on.is_none();
    update_in_tx(
        tx,
        d.id,
        UpdateDecision {
            status: Some(DecisionStatus::Decided),
            decided_on: if stamp {
                minimap_types::Patch::Set(today())
            } else {
                minimap_types::Patch::Keep
            },
            auto_status: Some(true),
            auto_decided_on: stamp.then_some(true),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// The *Finish decision* dialog: writes the decision and the reason on the task's record (made if
/// there is none) and finishes the task, which decides the record, all in one transaction (one
/// undo step).
pub fn finish_task(
    conn: &mut Connection,
    task: Uuid,
    decision: String,
    rationale: String,
) -> Result<Task> {
    if decision.trim().is_empty() || rationale.trim().is_empty() {
        return Err(StoreError::Invalid(
            "Write both what was decided and why.".into(),
        ));
    }
    let tx = conn.transaction()?;
    let t = tasks::get(&tx, task)?;
    if t.task_type.as_deref() != Some(DECISION_TYPE) || t.archived_at.is_some() {
        return Err(StoreError::Invalid(
            "Only an open task of type Decision can be finished this way.".into(),
        ));
    }
    let record = ensure_for_task_in_tx(&tx, &t)?;
    update_in_tx(
        &tx,
        record.id,
        UpdateDecision {
            decision: Some(decision),
            rationale: Some(rationale),
            ..Default::default()
        },
    )?;
    let done = tasks::update_in_tx(
        &tx,
        task,
        minimap_types::UpdateTask {
            status: Some(minimap_types::TaskStatus::Done),
            ..Default::default()
        },
    )?;
    tx.commit()?;
    Ok(done)
}
