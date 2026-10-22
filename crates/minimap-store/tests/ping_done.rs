//! A task that is moved to Done loses its ping: it is cleared (kept in the history and the export,
//! like any cleared ping), and undoing the finish brings it back.

use minimap_core::undo::{plan, Plan};
use minimap_store::*;
use minimap_types::*;
use time::Duration;

fn task(conn: &mut Connection, title: &str) -> Task {
    let project = fixtures::project(conn);
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: Some("build".into()),
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: Some(project),
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
}

fn ping(conn: &mut Connection, task: &Task) -> Ping {
    pings::create(
        conn,
        today(),
        CreatePing {
            target: NodeRef::new(NodeType::Task, task.id),
            note: "chase".into(),
            on_date: today() + Duration::days(2),
        },
    )
    .unwrap()
}

fn status(conn: &mut Connection, id: uuid::Uuid, s: TaskStatus) {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(s),
            // In progress needs a start date.
            start_date: Patch::Set(today()),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn moving_a_task_to_done_clears_its_ping() {
    let mut conn = open_in_memory().unwrap();
    let t = task(&mut conn, "Chase the vendor");
    let p = ping(&mut conn, &t);
    let other = task(&mut conn, "Another");
    let kept = ping(&mut conn, &other);

    status(&mut conn, t.id, TaskStatus::Done);

    assert!(pings::live_for(&conn, t.id).unwrap().is_none());
    let cleared = pings::get(&conn, p.id).unwrap();
    assert_eq!(cleared.cleared_on, Some(today()));
    assert!(
        cleared.archived_at.is_none(),
        "cleared, not erased: it stays in the history"
    );
    assert!(pings::list_live(&conn)
        .unwrap()
        .iter()
        .all(|r| r.ping.id != p.id));
    // Another task's ping is left alone.
    assert_eq!(
        pings::live_for(&conn, other.id).unwrap().unwrap().id,
        kept.id
    );
    // Both are still in the full export.
    assert_eq!(pings::list_all(&conn).unwrap().len(), 2);
}

#[test]
fn other_changes_of_status_keep_the_ping() {
    let mut conn = open_in_memory().unwrap();
    let t = task(&mut conn, "Chase the vendor");
    let p = ping(&mut conn, &t);
    for s in [
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::Cancelled,
    ] {
        status(&mut conn, t.id, s);
        assert_eq!(
            pings::live_for(&conn, t.id).unwrap().map(|x| x.id),
            Some(p.id),
            "{s:?}"
        );
    }
}

#[test]
fn a_task_with_no_ping_finishes_as_before_and_a_cleared_ping_keeps_its_first_day() {
    let mut conn = open_in_memory().unwrap();
    let plain = task(&mut conn, "Plain");
    status(&mut conn, plain.id, TaskStatus::Done);
    assert_eq!(
        tasks::get(&conn, plain.id).unwrap().status,
        TaskStatus::Done
    );

    let t = task(&mut conn, "Chase");
    let p = ping(&mut conn, &t);
    let first_day = today();
    pings::clear(&mut conn, p.id, first_day).unwrap();
    status(&mut conn, t.id, TaskStatus::Done);
    assert_eq!(pings::get(&conn, p.id).unwrap().cleared_on, Some(first_day));
}

#[test]
fn undoing_the_finish_brings_the_ping_back_and_redo_clears_it_again() {
    let mut conn = open_in_memory().unwrap();
    let t = task(&mut conn, "Chase the vendor");
    let p = ping(&mut conn, &t);

    let marker = activity::latest_rowid(&conn).unwrap();
    status(&mut conn, t.id, TaskStatus::Done);
    let rows = activity::since(&conn, marker).unwrap();
    let Plan::Steps(steps) = plan(&rows) else {
        panic!("expected steps");
    };

    let undone = undo::apply(&mut conn, &steps).unwrap();
    assert_eq!(tasks::get(&conn, t.id).unwrap().status, TaskStatus::Todo);
    assert_eq!(
        pings::live_for(&conn, t.id).unwrap().map(|x| x.id),
        Some(p.id)
    );

    let Plan::Steps(redo) = plan(&undone) else {
        panic!("expected steps");
    };
    undo::apply(&mut conn, &redo).unwrap();
    assert_eq!(tasks::get(&conn, t.id).unwrap().status, TaskStatus::Done);
    assert!(pings::live_for(&conn, t.id).unwrap().is_none());
}
