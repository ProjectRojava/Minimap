use minimap_types::{
    ActivityAction, AssigneeChoice, CreateTask, EdgeType, LinkRelation, NewEdge, NodeRef, NodeType,
    Recurrence, Task, TaskStatus, UpdateTask, DECISION_TYPE, DEFAULT_PRIORITY,
};
use rusqlite::{params, Connection, Row, Transaction};
use time::OffsetDateTime;
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
const COLS: &str = "id, title, description, project_id, status, estimate_days, start_date, due_date, completed_at, priority, created_at, updated_at, archived_at, recurrence, links, task_type, focus, start_minute, length_minutes, confirmed_on, backburner_since, order_step";

fn from_row(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        links: col_links(r, 14)?,
        task_type: r.get(15)?,
        focus: col_focus(r, 16)?,
        start_minute: r.get(17)?,
        length_minutes: r.get(18)?,
        confirmed_on: col_date_opt(r, 19)?,
        backburner_since: col_date_opt(r, 20)?,
        order_step: r.get(21)?,
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
    // A meeting has a day and cannot be set aside (spec 59).
    if t.backburner_since.is_some() && t.is_meeting() {
        return Err(StoreError::Invalid(
            "A meeting has a day. Move it or cancel it instead of putting it on the backburner."
                .into(),
        ));
    }
    // A meeting needs a day and a time (spec 38).
    minimap_core::meeting::validate(t.is_meeting(), t.due_date, t.start_minute, t.length_minutes)
        .map_err(StoreError::Invalid)?;
    Ok(())
}

pub fn get(conn: &Connection, id: Uuid) -> Result<Task> {
    fetch(conn, TABLE, COLS, NodeType::Task, id, from_row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Task>> {
    fetch_all(conn, TABLE, COLS, include_archived, from_row)
}

/// The tasks the plan is made of: everything but meetings (spec 38). A meeting happens at a time
/// and takes no days of work, so it is left out of the schedule, the critical path, impact
/// analysis, capacity, health and the dependency graph.
pub fn list_planned(conn: &Connection) -> Result<Vec<Task>> {
    Ok(list(conn, false)?
        .into_iter()
        .filter(|t| !t.is_meeting())
        .collect())
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

/// What a task without a project is told.
pub const NEEDS_PROJECT: &str = "A task needs a project. Choose one before creating it.";

/// A task belongs to a project. A meeting (it has a start time) is the exception: a 1:1 or a
/// stand-up is not part of any project's work.
fn ensure_project(project_id: Option<Uuid>, start_minute: Option<u16>) -> Result<()> {
    if project_id.is_none() && start_minute.is_none() {
        return Err(StoreError::Invalid(NEEDS_PROJECT.into()));
    }
    Ok(())
}

/// What a task without a type is told.
pub const NEEDS_TYPE: &str = "A task needs a type. Choose one before creating it.";

/// A new task has a type, chosen by whoever makes it. A task from before types were required keeps
/// none until it is given one.
pub(crate) fn ensure_type(task_type: Option<&str>) -> Result<()> {
    if task_type.is_none_or(str::is_empty) {
        return Err(StoreError::Invalid(NEEDS_TYPE.into()));
    }
    Ok(())
}

/// The task, its assignee link (`Me` = the self person, if there is one) and their activity rows.
/// Refused without a project, unless it is a meeting, and without a type.
pub(crate) fn create_in_tx(tx: &Transaction, mut input: CreateTask) -> Result<Task> {
    ensure_project(input.project_id, input.start_minute)?;
    ensure_type(input.task_type.as_deref())?;
    // A task made in progress starts today unless it says otherwise: a task in progress always
    // has a start date.
    if input.status == Some(TaskStatus::InProgress)
        && input.start_date.is_none()
        && input.start_minute.is_none()
    {
        input.start_date = Some(today());
    }
    // The start and the estimate give the due date, the start and the due date the estimate
    // (as in `update`).
    if input.start_minute.is_none()
        && input.task_type.as_deref() != Some(minimap_types::MEETING_TYPE)
    {
        let week = crate::settings::get(tx)?.work_week;
        match minimap_core::tasks::third(
            input.start_date,
            input.due_date,
            input.estimate_days,
            week,
        ) {
            Some(minimap_core::tasks::Third::Due(d)) => input.due_date = Some(d),
            Some(minimap_core::tasks::Third::Estimate(e)) => input.estimate_days = Some(e),
            None => {}
        }
    }
    create_unchecked_in_tx(tx, input)
}

/// [`create_in_tx`] for a task made *from* another, which takes whatever project that one had: the
/// next of a repeating task, one linked to a task. A task from before projects were required, or
/// whose project has been archived since, must still be able to carry on.
pub(crate) fn create_unchecked_in_tx(tx: &Transaction, input: CreateTask) -> Result<Task> {
    let assignee = match input.assignee {
        AssigneeChoice::Me => people::get_self(tx)?.map(|p| p.id),
        AssigneeChoice::Nobody => None,
        AssigneeChoice::Person(id) => Some(id),
    };
    let at = now();
    let status = input.status.unwrap_or(TaskStatus::Todo);
    let t = Task {
        links: minimap_core::links::clean(input.links).map_err(StoreError::Invalid)?,
        task_type: input.task_type,
        focus: input.focus,
        start_minute: input.start_minute,
        length_minutes: input.length_minutes,
        confirmed_on: None,
        order_step: None,
        backburner_since: input.backburner_since,
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
    check_type(tx, None, t.task_type.as_deref())?;
    tx.execute(
        &format!(
            "INSERT INTO {TABLE} ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)"
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
            links_s(&t.links),
            t.task_type,
            focus_s(t.focus.as_ref()),
            t.start_minute,
            t.length_minutes,
            date_s(t.confirmed_on),
            date_s(t.backburner_since),
            t.order_step,
        ],
    )?;
    activity::record_created(tx, at, NodeType::Task, t.id, &t)?;
    if let Some(person) = assignee {
        edges::add_in_tx(tx, assigned_to(t.id, person))?;
    }
    // A decision task has its record in the Decisions log from the start (spec 41).
    if t.task_type.as_deref() == Some(DECISION_TYPE) {
        crate::decisions::ensure_for_task_in_tx(tx, &t)?;
    }
    Ok(t)
}

/// A type chosen for a task must be in Settings' list and not retired. A task may keep a retired
/// type it already has (`before`).
fn check_type(conn: &Connection, before: Option<&str>, chosen: Option<&str>) -> Result<()> {
    let Some(id) = chosen.filter(|id| Some(*id) != before) else {
        return Ok(());
    };
    let types = crate::settings::task_types(conn)?;
    match minimap_types::TaskType::find(&types, id) {
        Some(t) if !t.archived => Ok(()),
        Some(t) => Err(StoreError::Invalid(format!(
            "The task type \"{}\" is archived. Pick another type.",
            t.name
        ))),
        None => Err(StoreError::Invalid(
            "That task type is not in the list. Add it in Settings first.".into(),
        )),
    }
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
    set_assignee_in_tx(&tx, task, person)?;
    tx.commit()?;
    Ok(())
}

/// [`set_assignee`] inside the caller's transaction (spec 61 gives a task and orders it in one).
pub(crate) fn set_assignee_in_tx(tx: &Transaction, task: Uuid, person: Option<Uuid>) -> Result<()> {
    get(tx, task)?; // NotFound for unknown tasks
    let current: Vec<_> = edges::list_for_node(tx, task, false)?
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
        edges::archive_in_tx(tx, edge, at)?;
    }
    if let Some(p) = person {
        edges::add_in_tx(tx, assigned_to(task, p))?;
    }
    // Its place in the old assignee's order means nothing to the new one (spec 60).
    if get(tx, task)?.order_step.is_some() {
        update_in_tx(
            tx,
            task,
            UpdateTask {
                order_step: minimap_types::Patch::Clear,
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

/// May `child` become a sub-task of `parent`? One parent, one level (spec 33): the refusal names
/// the tasks. Reads the live links, so inside a transaction it also sees the ones before it.
pub fn check_subtask(conn: &Connection, child: Uuid, parent: Uuid) -> Result<()> {
    let pairs: Vec<(Uuid, Uuid)> = edges::list_active_of_type(conn, EdgeType::SubtaskOf)?
        .into_iter()
        .map(|e| (e.from_id, e.to_id))
        .collect();
    let Err(why) = minimap_core::subtasks::check_new(&pairs, child, parent) else {
        return Ok(());
    };
    let name = |id: Uuid| {
        nodes::summary(conn, NodeRef::new(NodeType::Task, id))
            .map(|s| s.label)
            .unwrap_or_else(|_| id.to_string())
    };
    let other = match why {
        minimap_core::subtasks::SubtaskError::AlreadyHasParent(p)
        | minimap_core::subtasks::SubtaskError::ParentIsSubtask(p) => name(p),
        minimap_core::subtasks::SubtaskError::ChildHasSubtasks => String::new(),
    };
    Err(StoreError::Invalid(why.message(
        &name(child),
        &name(parent),
        &other,
    )))
}

/// A new task linked to `source`, in one transaction (one undo step): it joins the source's
/// project and takes its priority, is assigned to the user like any new task, and is joined to the
/// source by `relation`. A new task can't close a loop, so no loop check is needed; a new parent or
/// sub-task still has to fit the one-parent, one-level rule.
pub fn create_linked(
    conn: &mut Connection,
    source: Uuid,
    relation: LinkRelation,
    title: String,
    task_type: Option<String>,
) -> Result<Task> {
    ensure_type(task_type.as_deref())?;
    let tx = conn.transaction()?;
    let from = get(&tx, source)?;
    if from.archived_at.is_some() {
        return Err(StoreError::Invalid(
            "an archived task can't be linked to a new one".into(),
        ));
    }
    let created = create_unchecked_in_tx(
        &tx,
        CreateTask {
            links: Vec::new(),
            task_type,
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            title,
            assignee: AssigneeChoice::Me,
            description: String::new(),
            project_id: from.project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: Some(from.priority),
            recurrence: None,
        },
    )?;
    let (edge_type, a, b) = match relation {
        LinkRelation::Blocks => (EdgeType::Blocks, created.id, source),
        LinkRelation::BlockedBy => (EdgeType::Blocks, source, created.id),
        LinkRelation::RelatesTo => (EdgeType::RelatesTo, source, created.id),
        LinkRelation::Parent => (EdgeType::SubtaskOf, source, created.id),
        LinkRelation::Subtask => (EdgeType::SubtaskOf, created.id, source),
    };
    if edge_type == EdgeType::SubtaskOf {
        check_subtask(&tx, a, b)?;
    }
    edges::add_in_tx(
        &tx,
        NewEdge {
            edge_type,
            from: NodeRef::new(NodeType::Task, a),
            to: NodeRef::new(NodeType::Task, b),
            attrs: serde_json::json!({}),
        },
    )?;
    tx.commit()?;
    Ok(created)
}

pub fn update(conn: &mut Connection, id: Uuid, mut patch: UpdateTask) -> Result<Task> {
    let tx = conn.transaction()?;
    fill_third_in_patch(&tx, id, &mut patch)?;
    let task = update_in_tx(&tx, id, patch)?;
    tx.commit()?;
    Ok(task)
}

/// A person's change to the start date, the due date or the estimate fills in the due date (from
/// the start and the estimate) or the estimate (from the start and the due date) when that one is
/// empty (`core::tasks::third`; the start is never filled in), and moving either date of a task
/// that has both re-counts the estimate (unless the change sets the estimate), as part of the same change (one
/// activity row, one undo step). Not for a meeting, and not for a field the change
/// itself clears. Writes Minimap makes itself (the order's dates, undo, merges) go through
/// [`update_in_tx`] and are left as they are.
fn fill_third_in_patch(tx: &Transaction, id: Uuid, patch: &mut UpdateTask) -> Result<()> {
    use minimap_core::tasks::Third;
    use minimap_types::Patch::{Clear, Keep, Set};
    let sets = matches!(patch.start_date, Set(_))
        || matches!(patch.due_date, Set(_))
        || matches!(patch.estimate_days, Set(_));
    if !sets {
        return Ok(());
    }
    let mut new = get(tx, id)?;
    patch.clone().apply(&mut new);
    if new.is_meeting() {
        return Ok(());
    }
    let week = crate::settings::get(tx)?.work_week;
    // Moving either date of a task that has both re-counts the estimate, so the three always
    // agree. (An estimate in the same change is the person's own word and stays.)
    let moved_a_date = matches!(patch.start_date, Set(_)) || matches!(patch.due_date, Set(_));
    if moved_a_date
        && patch.estimate_days == Keep
        && new.start_date.is_some()
        && new.due_date.is_some()
    {
        if let Some(Third::Estimate(e)) =
            minimap_core::tasks::third(new.start_date, new.due_date, None, week)
        {
            if new.estimate_days != Some(e) {
                patch.estimate_days = Set(e);
            }
        }
        return Ok(());
    }
    match minimap_core::tasks::third(new.start_date, new.due_date, new.estimate_days, week) {
        Some(Third::Due(d)) if patch.due_date != Clear => patch.due_date = Set(d),
        Some(Third::Estimate(e)) if patch.estimate_days != Clear => patch.estimate_days = Set(e),
        _ => {}
    }
    Ok(())
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
    update_in_tx_at(tx, id, patch, None)
}

/// [`update_in_tx`] for undo and redo: puts the fields back as they were without making a
/// decision record for a task that becomes a Decision (undo and redo bring the records back
/// themselves, so one made here would be a second).
pub(crate) fn restore_in_tx(tx: &Transaction, id: Uuid, patch: UpdateTask) -> Result<Task> {
    update_core(tx, id, patch, None, false)
}

/// [`update_in_tx`] where a task that is finished by this update is finished at `finished_at`
/// instead of now (a meeting that ended while the app was closed, spec 38).
pub(crate) fn update_in_tx_at(
    tx: &Transaction,
    id: Uuid,
    patch: UpdateTask,
    finished_at: Option<OffsetDateTime>,
) -> Result<Task> {
    update_core(tx, id, patch, finished_at, true)
}

fn update_core(
    tx: &Transaction,
    id: Uuid,
    patch: UpdateTask,
    finished_at: Option<OffsetDateTime>,
    make_record: bool,
) -> Result<Task> {
    let old = get(tx, id)?;
    let mut new = old.clone();
    let start_untouched = patch.start_date == minimap_types::Patch::Keep;
    patch.apply(&mut new);
    // Starting work starts the task: moving it from To do to In progress makes today its start
    // date, whatever date was planned. A start date set in the same change is the person's own
    // word and stays, and it can be changed by hand at any time after. A task in progress always
    // has a start date, so a move to In progress that would leave it without one is refused, and
    // the person is asked for the date (`StartDateNeeded`). (Undo and redo put things back as
    // they were; a meeting starts at its time, not when it is opened.)
    if make_record
        && start_untouched
        && old.status == TaskStatus::Todo
        && old.start_date.is_some()
        && new.status == TaskStatus::InProgress
        && !new.is_meeting()
    {
        new.start_date = Some(today());
    }
    // A task already in progress keeps its date: clearing it is refused too. (One that has had
    // none since before this rule can still be edited otherwise.)
    if make_record
        && new.status == TaskStatus::InProgress
        && !new.is_meeting()
        && new.start_date.is_none()
        && (old.status != TaskStatus::InProgress || old.start_date.is_some())
    {
        return Err(StoreError::StartDateNeeded(format!(
            "“{}” needs a start date before it can be in progress",
            new.title
        )));
    }
    new.links = minimap_core::links::clean(new.links).map_err(StoreError::Invalid)?;
    // The time of a meeting goes when the task stops being one.
    if !new.is_meeting() {
        new.start_minute = None;
        new.length_minutes = None;
    }
    // A project can be changed but not taken away. (Undo and redo put things back as they were.)
    if make_record && old.project_id.is_some() {
        ensure_project(new.project_id, new.start_minute)?;
    }
    // completed_at follows status: set on entering `done`, cleared on leaving it.
    let at = now();
    let finishing = old.status != TaskStatus::Done && new.status == TaskStatus::Done;
    if finishing {
        new.completed_at = Some(finished_at.unwrap_or(at));
    } else if old.status == TaskStatus::Done && new.status != TaskStatus::Done {
        new.completed_at = None;
    }
    // Finishing or cancelling a task takes it off the backburner: it is not set aside, it is
    // over. (Undo and redo put things back as they were.)
    if make_record && minimap_core::tasks::is_closed(new.status) {
        new.backburner_since = None;
        // Nor is it in anyone's order of work any more (spec 60).
        new.order_step = None;
    }
    validate(&new)?;
    check_type(tx, old.task_type.as_deref(), new.task_type.as_deref())?;
    // A decision is not finished until it says what was decided and why (spec 42). Undo and redo
    // put things back as they were and are not held up by it.
    let decides = make_record && finishing && new.task_type.as_deref() == Some(DECISION_TYPE);
    if decides {
        crate::decisions::check_can_finish(tx, &new)?;
    }
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
    if !activity::only_confirmation(&diff) {
        new.updated_at = at;
    }
    tx.execute(
        &format!(
            "UPDATE {TABLE} SET title=?2, description=?3, project_id=?4, status=?5, estimate_days=?6, start_date=?7, due_date=?8, completed_at=?9, priority=?10, recurrence=?11, links=?12, task_type=?14, focus=?15, start_minute=?16, length_minutes=?17, confirmed_on=?18, backburner_since=?19, order_step=?20, updated_at=?13 WHERE id=?1"
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
            links_s(&new.links),
            ts_s(new.updated_at),
            new.task_type,
            focus_s(new.focus.as_ref()),
            new.start_minute,
            new.length_minutes,
            date_s(new.confirmed_on),
            date_s(new.backburner_since),
            new.order_step,
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
    if decides {
        crate::decisions::decide_with_task_in_tx(tx, &new)?;
    }
    // A finished task needs no reminder: its ping is cleared (it stays in the history and the
    // export, like any cleared ping). Undo and redo of the finish bring it back or clear it again
    // through the ping's own activity row, so they don't do it here.
    if make_record && finishing {
        if let Some(ping) = crate::pings::live_for(tx, id)? {
            let day = today();
            crate::pings::update_in_tx(
                tx,
                ping.id,
                day,
                minimap_types::UpdatePing {
                    cleared_on: minimap_types::Patch::Set(day),
                    ..Default::default()
                },
            )?;
        }
    }
    // Becoming a decision task makes its record (spec 41); ceasing to be one removes it, for good
    // (the pane asks first). Changing back later makes a new, empty one. Undo and redo put the type
    // back without touching records, so a task typed Decision again by undo has none, and its pane
    // offers to create one.
    if make_record && new.task_type != old.task_type {
        if new.task_type.as_deref() == Some(DECISION_TYPE) && new.archived_at.is_none() {
            crate::decisions::ensure_for_task_in_tx(tx, &new)?;
        } else if old.task_type.as_deref() == Some(DECISION_TYPE) {
            crate::decisions::remove_for_task_in_tx(tx, id)?;
        }
    }
    if let Some(rule) = handed_on {
        make_next(tx, &new, rule)?;
    }
    Ok(new)
}

/// The task that follows a finished repeating one: the same title, description, project,
/// estimate, priority, links, pin, assignee and objectives, due on the rule's next date (with the same gap
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
    let next = create_unchecked_in_tx(
        tx,
        CreateTask {
            links: done.links.clone(),
            task_type: done.task_type.clone(),
            // A pin carries on to the next one; a focus with an end belonged to this one.
            focus: done.focus.filter(|f| f.until.is_none()),
            // The next one starts fresh: a task that is finished is off the backburner anyway.
            backburner_since: None,
            // A repeating meeting is at the same time of day, for as long.
            start_minute: done.start_minute,
            length_minutes: done.length_minutes,
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
    // Each meeting of a series follows up on the one before (spec 38).
    if done.is_meeting() {
        edges::add_in_tx(
            tx,
            NewEdge {
                edge_type: EdgeType::FollowsUp,
                from: NodeRef::new(NodeType::Task, next.id),
                to: NodeRef::new(NodeType::Task, done.id),
                attrs: serde_json::json!({}),
            },
        )?;
    }
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
