//! A task in progress always has a start date (spec 06). Moving a task from To do with a planned
//! date makes today its start; moving one with no date at all is refused until a date is given,
//! so the person is asked for it.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn make(conn: &mut Connection, start: Option<time::Date>) -> Task {
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

fn try_status(conn: &mut Connection, id: uuid::Uuid, s: TaskStatus) -> Result<Task> {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(s),
            ..Default::default()
        },
    )
}

fn start_on(conn: &mut Connection, id: uuid::Uuid, day: time::Date) -> Task {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(TaskStatus::InProgress),
            start_date: Patch::Set(day),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn moving_a_task_with_no_start_date_to_in_progress_is_refused_and_asks_for_one() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, None);
    let err = try_status(&mut conn, t.id, TaskStatus::InProgress).unwrap_err();
    assert!(
        matches!(&err, StoreError::StartDateNeeded(m) if m.contains("Build it")),
        "{err}"
    );
    // Nothing was written.
    let still = tasks::get(&conn, t.id).unwrap();
    assert_eq!((still.status, still.start_date), (TaskStatus::Todo, None));
    // Every way in is covered: blocked or finished tasks too.
    for other in [TaskStatus::Blocked, TaskStatus::Done] {
        status(&mut conn, t.id, other);
        assert!(matches!(
            try_status(&mut conn, t.id, TaskStatus::InProgress),
            Err(StoreError::StartDateNeeded(_))
        ));
    }
}

#[test]
fn moving_a_task_with_a_planned_start_date_makes_today_the_start_date() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(date!(2027 - 03 - 01)));
    let started = status(&mut conn, t.id, TaskStatus::InProgress);
    assert_eq!(started.start_date, Some(today()));
}

#[test]
fn a_start_date_given_with_the_move_is_kept_and_it_can_be_changed_but_not_cleared_after() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, None);
    let by_hand = start_on(&mut conn, t.id, date!(2027 - 02 - 01));
    assert_eq!(by_hand.start_date, Some(date!(2027 - 02 - 01)));
    let changed = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            start_date: Patch::Set(date!(2027 - 03 - 05)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(changed.start_date, Some(date!(2027 - 03 - 05)));
    // A task in progress keeps a start date.
    let err = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            start_date: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(err, StoreError::StartDateNeeded(_)));
    assert_eq!(
        tasks::get(&conn, t.id).unwrap().start_date,
        Some(date!(2027 - 03 - 05))
    );
    // Moved out of In progress, the date can go.
    status(&mut conn, t.id, TaskStatus::Todo);
    let cleared = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            start_date: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cleared.start_date, None);
}

#[test]
fn only_the_move_from_to_do_replaces_a_date_and_other_edits_leave_it_alone() {
    let mut conn = open_in_memory().unwrap();
    // Reopening a finished task keeps the date it had.
    let planned = make(&mut conn, Some(date!(2027 - 03 - 01)));
    status(&mut conn, planned.id, TaskStatus::Done);
    let reopened = status(&mut conn, planned.id, TaskStatus::InProgress);
    assert_eq!(reopened.start_date, Some(date!(2027 - 03 - 01)));
    // Other edits to a task that is in progress leave the date alone.
    let started = make(&mut conn, None);
    let in_progress = start_on(&mut conn, started.id, date!(2027 - 03 - 02));
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
fn a_task_made_in_progress_starts_today_unless_it_says_otherwise() {
    let mut conn = open_in_memory().unwrap();
    let project = fixtures::project(&conn);
    let input = |start| CreateTask {
        links: Vec::new(),
        task_type: Some("build".into()),
        focus: None,
        backburner_since: None,
        start_minute: None,
        length_minutes: None,
        title: "Already going".into(),
        assignee: AssigneeChoice::Nobody,
        description: String::new(),
        project_id: Some(project),
        status: Some(TaskStatus::InProgress),
        estimate_days: None,
        start_date: start,
        due_date: None,
        priority: None,
        recurrence: None,
    };
    let now = tasks::create(&mut conn, input(None)).unwrap();
    assert_eq!(now.start_date, Some(today()));
    let planned = tasks::create(&mut conn, input(Some(date!(2027 - 03 - 01)))).unwrap();
    assert_eq!(planned.start_date, Some(date!(2027 - 03 - 01)));
}

#[test]
fn the_new_start_date_is_in_the_same_activity_row_as_the_status_change() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(date!(2027 - 03 - 01)));
    status(&mut conn, t.id, TaskStatus::InProgress);
    let log = activity::list_for_node(&conn, t.id).unwrap();
    let diff = log[0].diff.to_string();
    assert!(
        diff.contains("start_date") && diff.contains("status"),
        "{diff}"
    );
}
