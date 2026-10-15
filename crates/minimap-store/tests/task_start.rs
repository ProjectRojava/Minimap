//! A task's start date is the day it was moved from To do to In progress, and can be changed by
//! hand as well.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn make(conn: &mut Connection, start: Option<time::Date>) -> Task {
    let project = fixtures::project(conn);
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            title: "Build it".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: Some(project),
            status: None,
            estimate_days: None,
            start_date: start,
            due_date: Some(date!(2030 - 01 - 01)),
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
}

fn status(conn: &mut Connection, id: uuid::Uuid, s: TaskStatus) -> Task {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(s),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn moving_to_in_progress_makes_today_the_start_date() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, None);
    assert_eq!(t.start_date, None);
    let started = status(&mut conn, t.id, TaskStatus::InProgress);
    assert_eq!(started.start_date, Some(today()));
}

#[test]
fn it_replaces_the_planned_start_date() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(date!(2027 - 03 - 01)));
    let started = status(&mut conn, t.id, TaskStatus::InProgress);
    assert_eq!(started.start_date, Some(today()));
}

#[test]
fn a_start_date_given_with_the_move_is_kept_and_it_can_be_changed_by_hand_later() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, None);
    let by_hand = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::InProgress),
            start_date: Patch::Set(date!(2027 - 02 - 01)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(by_hand.start_date, Some(date!(2027 - 02 - 01)));

    let t2 = make(&mut conn, None);
    status(&mut conn, t2.id, TaskStatus::InProgress);
    let changed = tasks::update(
        &mut conn,
        t2.id,
        UpdateTask {
            start_date: Patch::Set(date!(2027 - 03 - 05)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(changed.start_date, Some(date!(2027 - 03 - 05)));
    let cleared = tasks::update(
        &mut conn,
        t2.id,
        UpdateTask {
            start_date: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cleared.start_date, None);
}

#[test]
fn only_the_move_from_to_do_starts_it() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, None);
    // Blocked then in progress is not a start.
    status(&mut conn, t.id, TaskStatus::Blocked);
    assert_eq!(status(&mut conn, t.id, TaskStatus::InProgress).start_date, None);
    // Nor is reopening a finished task.
    let planned = make(&mut conn, Some(date!(2027 - 03 - 01)));
    status(&mut conn, planned.id, TaskStatus::Done);
    let reopened = status(&mut conn, planned.id, TaskStatus::InProgress);
    assert_eq!(reopened.start_date, Some(date!(2027 - 03 - 01)));
    // Other edits to a task that is in progress leave the date alone.
    let started = make(&mut conn, None);
    let in_progress = status(&mut conn, started.id, TaskStatus::InProgress);
    let renamed = tasks::update(
        &mut conn,
        started.id,
        UpdateTask {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(renamed.start_date, in_progress.start_date);
}

#[test]
fn the_new_start_date_is_in_the_same_activity_row_as_the_status_change() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(date!(2027 - 03 - 01)));
    status(&mut conn, t.id, TaskStatus::InProgress);
    let log = activity::list_for_node(&conn, t.id).unwrap();
    let diff = log[0].diff.to_string();
    assert!(diff.contains("start_date") && diff.contains("status"), "{diff}");
}
