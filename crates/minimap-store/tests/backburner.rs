//! The backburner (spec 59): a flag that keeps the task as it was, goes when the task is finished
//! or cancelled, is refused on a meeting, and is undone like any other change.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;
use uuid::Uuid;

const DAY: time::Date = date!(2027 - 03 - 03);

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn task(conn: &mut Connection, f: impl FnOnce(&mut CreateTask)) -> Task {
    let project = fixtures::project(conn);
    let mut input = CreateTask {
        links: Vec::new(),
        task_type: Some("build".into()),
        focus: None,
        backburner_since: None,
        start_minute: None,
        length_minutes: None,
        title: "Tidy the wiki".into(),
        assignee: AssigneeChoice::Nobody,
        description: String::new(),
        project_id: Some(project),
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: Some(date!(2027 - 03 - 10)),
        priority: None,
        recurrence: None,
    };
    f(&mut input);
    tasks::create(conn, input).unwrap()
}

fn patch(conn: &mut Connection, id: Uuid, p: UpdateTask) -> Result<Task> {
    tasks::update(conn, id, p)
}

fn park(conn: &mut Connection, id: Uuid) -> Task {
    patch(
        conn,
        id,
        UpdateTask {
            backburner_since: Patch::Set(DAY),
            ..Default::default()
        },
    )
    .unwrap()
}

fn set_status(conn: &mut Connection, id: Uuid, status: TaskStatus) -> Task {
    patch(
        conn,
        id,
        UpdateTask {
            status: Some(status),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn a_task_is_parked_and_brought_back_with_everything_else_as_it_was() {
    let mut conn = db();
    let t = task(&mut conn, |t| t.status = Some(TaskStatus::Blocked));
    assert!(!t.is_parked());
    let parked = park(&mut conn, t.id);
    assert_eq!(parked.backburner_since, Some(DAY));
    assert!(parked.is_parked());
    let read = tasks::get(&conn, t.id).unwrap();
    assert_eq!(read.backburner_since, Some(DAY));
    // Status, dates and priority are untouched.
    assert_eq!(read.status, TaskStatus::Blocked);
    assert_eq!(read.due_date, t.due_date);
    assert_eq!(read.priority, t.priority);

    let back = patch(
        &mut conn,
        t.id,
        UpdateTask {
            backburner_since: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(back.backburner_since, None);
    assert_eq!(back.status, TaskStatus::Blocked);
}

#[test]
fn a_task_can_be_born_on_the_backburner() {
    let mut conn = db();
    let t = task(&mut conn, |t| t.backburner_since = Some(DAY));
    assert_eq!(tasks::get(&conn, t.id).unwrap().backburner_since, Some(DAY));
}

#[test]
fn finishing_or_cancelling_a_parked_task_takes_it_off_the_backburner() {
    let mut conn = db();
    for status in [TaskStatus::Done, TaskStatus::Cancelled] {
        let t = task(&mut conn, |_| {});
        park(&mut conn, t.id);
        let closed = set_status(&mut conn, t.id, status);
        assert_eq!(closed.backburner_since, None, "{status:?}");
        // Reopening it does not put it back.
        assert_eq!(
            set_status(&mut conn, t.id, TaskStatus::Todo).backburner_since,
            None
        );
    }
}

#[test]
fn a_meeting_cannot_be_put_on_the_backburner() {
    let mut conn = db();
    let m = meetings::create(
        &mut conn,
        "Weekly sync".into(),
        DAY,
        600,
        None,
        None,
        AssigneeChoice::Nobody,
    )
    .unwrap();
    let err = patch(
        &mut conn,
        m.id,
        UpdateTask {
            backburner_since: Patch::Set(DAY),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("backburner"), "{err}");
    assert_eq!(tasks::get(&conn, m.id).unwrap().backburner_since, None);
}

#[test]
fn parking_and_bringing_back_are_in_the_history_and_undone() {
    let mut conn = db();
    let t = task(&mut conn, |_| {});
    let marker = activity::latest_rowid(&conn).unwrap();
    park(&mut conn, t.id);
    let rows = activity::since(&conn, marker).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].diff.get("backburner_since").is_some());
    let steps = match minimap_core::undo::plan(&rows) {
        minimap_core::undo::Plan::Steps(s) => s,
        other => panic!("expected steps, got {other:?}"),
    };
    let redo = undo::apply(&mut conn, &steps).unwrap();
    assert_eq!(tasks::get(&conn, t.id).unwrap().backburner_since, None);
    let steps = match minimap_core::undo::plan(&redo) {
        minimap_core::undo::Plan::Steps(s) => s,
        other => panic!("expected steps, got {other:?}"),
    };
    undo::apply(&mut conn, &steps).unwrap();
    assert_eq!(tasks::get(&conn, t.id).unwrap().backburner_since, Some(DAY));
}

#[test]
fn the_next_of_a_repeating_task_starts_off_the_backburner() {
    let mut conn = db();
    let t = task(&mut conn, |t| {
        t.recurrence = Some(Cadence::Daily.into());
        t.backburner_since = Some(DAY);
    });
    set_status(&mut conn, t.id, TaskStatus::Done);
    let all = tasks::list(&conn, false).unwrap();
    assert!(all.len() >= 2);
    assert!(all.iter().all(|t| !t.is_parked()));
}
