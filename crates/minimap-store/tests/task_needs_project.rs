//! A task belongs to a project: one can't be made without, and the project can be changed but not
//! taken away. Meetings are the exception, and a task made from another (the next of a repeating
//! one, a linked one) carries on with whatever project that one had.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn input(title: &str, project: Option<uuid::Uuid>) -> CreateTask {
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
        project_id: project,
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: None,
        priority: None,
        recurrence: None,
    }
}

fn project(conn: &mut Connection, title: &str) -> uuid::Uuid {
    projects::create(
        conn,
        CreateProject {
            title: title.into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap()
    .id
}

fn refused(r: Result<Task>) -> bool {
    matches!(r, Err(StoreError::Invalid(m)) if m == tasks::NEEDS_PROJECT)
}

#[test]
fn a_task_cannot_be_made_without_a_project() {
    let mut conn = open_in_memory().unwrap();
    assert!(refused(tasks::create(&mut conn, input("Loose", None))));
    assert!(tasks::list(&conn, true).unwrap().is_empty());

    let p = project(&mut conn, "Launch");
    let t = tasks::create(&mut conn, input("Filed", Some(p))).unwrap();
    assert_eq!(t.project_id, Some(p));
}

#[test]
fn a_batch_with_one_loose_task_makes_none() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "Launch");
    let made = tasks::create_many(
        &mut conn,
        vec![input("a", Some(p)), input("b", None), input("c", Some(p))],
    );
    assert!(matches!(made, Err(StoreError::Invalid(_))));
    assert!(tasks::list(&conn, true).unwrap().is_empty());
}

#[test]
fn a_meeting_is_the_exception() {
    let mut conn = open_in_memory().unwrap();
    let mut meeting = input("1:1", None);
    meeting.start_minute = Some(10 * 60);
    meeting.due_date = Some(date!(2027 - 03 - 03));
    meeting.task_type = Some(MEETING_TYPE.to_owned());
    let m = tasks::create(&mut conn, meeting).unwrap();
    assert_eq!(m.project_id, None);
    // And it can stay without one when it is edited.
    let renamed = tasks::update(
        &mut conn,
        m.id,
        UpdateTask {
            title: Some("1:1 with Raj".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(renamed.project_id, None);
}

#[test]
fn the_project_can_be_changed_but_not_taken_away() {
    let mut conn = open_in_memory().unwrap();
    let (a, b) = (project(&mut conn, "A"), project(&mut conn, "B"));
    let t = tasks::create(&mut conn, input("Move me", Some(a))).unwrap();

    let moved = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            project_id: Patch::Set(b),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(moved.project_id, Some(b));

    let cleared = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            project_id: Patch::Clear,
            ..Default::default()
        },
    );
    assert!(matches!(cleared, Err(StoreError::Invalid(m)) if m == tasks::NEEDS_PROJECT));
    assert_eq!(tasks::get(&conn, t.id).unwrap().project_id, Some(b));
}

#[test]
fn a_task_from_before_the_rule_can_still_be_worked_on() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "Launch");
    let old = tasks::create(&mut conn, input("Old", Some(p))).unwrap();
    // As if it had been made before projects were required.
    conn.execute(
        "UPDATE tasks SET project_id = NULL WHERE id = ?1",
        [old.id.to_string()],
    )
    .unwrap();

    let renamed = tasks::update(
        &mut conn,
        old.id,
        UpdateTask {
            title: Some("Old, renamed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(renamed.project_id, None);
    // A task linked to it takes what it has, none.
    let linked = tasks::create_linked(
        &mut conn,
        old.id,
        LinkRelation::RelatesTo,
        "Next".into(),
        Some("build".into()),
    )
    .unwrap();
    assert_eq!(linked.project_id, None);
}

#[test]
fn the_next_of_a_repeating_task_carries_on_even_if_its_project_was_archived() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "Launch");
    let mut weekly = input("Board update", Some(p));
    weekly.due_date = Some(date!(2027 - 03 - 01));
    weekly.recurrence = Some(
        Cadence::Weekly {
            every: 1,
            weekday: 0,
        }
        .into(),
    );
    let t = tasks::create(&mut conn, weekly).unwrap();
    projects::archive(&mut conn, p, TaskDisposition::Inbox).unwrap();

    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let next = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|x| x.id != t.id && x.title == "Board update")
        .expect("the next one was made");
    assert_eq!(next.project_id, None);
}
