//! Finished tasks are archived on their own after a while (Settings), except the ones that
//! something live still leans on.

use minimap_store::*;
use minimap_types::*;
use time::{macros::date, Duration};

const TODAY: time::Date = date!(2027-03-31);

fn task(conn: &mut Connection, title: &str) -> Task {
    let project = fixtures::project(conn);
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            focus: None,
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

/// A task finished `days` days before TODAY.
fn finished(conn: &mut Connection, title: &str, days: i64) -> Task {
    let t = task(conn, title);
    tasks::update(
        conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let at = format!("{}T12:00:00.000Z", TODAY - Duration::days(days));
    conn.execute(
        "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
        rusqlite::params![t.id.to_string(), at],
    )
    .unwrap();
    t
}

fn archived(conn: &Connection, id: uuid::Uuid) -> bool {
    tasks::get(conn, id).unwrap().archived_at.is_some()
}

fn link(conn: &mut Connection, kind: EdgeType, from: uuid::Uuid, to: uuid::Uuid) {
    edges::add(
        conn,
        NewEdge {
            edge_type: kind,
            from: NodeRef::new(NodeType::Task, from),
            to: NodeRef::new(NodeType::Task, to),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
}

#[test]
fn a_task_finished_a_month_ago_is_archived_and_a_newer_one_is_not() {
    let mut conn = open_in_memory().unwrap();
    let old = finished(&mut conn, "Old", 30);
    let nearly = finished(&mut conn, "Nearly", 29);
    let open = task(&mut conn, "Open");

    let gone = done_sweep::archive_old_done(&mut conn, TODAY).unwrap();
    assert_eq!(gone, vec![old.id]);
    assert!(archived(&conn, old.id));
    assert!(!archived(&conn, nearly.id) && !archived(&conn, open.id));
    // Nothing more to do the second time.
    assert!(done_sweep::archive_old_done(&mut conn, TODAY).unwrap().is_empty());
    // Tomorrow the next one is old enough.
    let next = done_sweep::archive_old_done(&mut conn, TODAY + Duration::days(1)).unwrap();
    assert_eq!(next, vec![nearly.id]);
}

#[test]
fn meetings_go_sooner_and_zero_means_never() {
    let mut conn = open_in_memory().unwrap();
    let mut meeting = CreateTask {
        links: Vec::new(),
        task_type: Some(MEETING_TYPE.into()),
        focus: None,
        start_minute: Some(600),
        length_minutes: None,
        title: "Sync".into(),
        assignee: AssigneeChoice::Nobody,
        description: String::new(),
        project_id: None,
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: Some(TODAY - Duration::days(3)),
        priority: None,
        recurrence: None,
    };
    meeting.status = Some(TaskStatus::Done);
    let m = tasks::create(&mut conn, meeting).unwrap();
    let at = format!("{}T12:00:00.000Z", TODAY - Duration::days(2));
    conn.execute(
        "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
        rusqlite::params![m.id.to_string(), at],
    )
    .unwrap();
    let plain = finished(&mut conn, "Plain", 2);

    let gone = done_sweep::archive_old_done(&mut conn, TODAY).unwrap();
    assert_eq!(gone, vec![m.id], "two days for a meeting, a month for the rest");
    assert!(!archived(&conn, plain.id));

    // Never: nothing is archived however old.
    settings::update(
        &mut conn,
        UpdateSettings {
            done_archive_days: Some(0),
            done_archive_meeting_days: Some(0),
            ..Default::default()
        },
    )
    .unwrap();
    finished(&mut conn, "Ancient", 400);
    assert!(done_sweep::archive_old_done(&mut conn, TODAY).unwrap().is_empty());
}

#[test]
fn the_days_are_a_setting_and_are_checked() {
    let mut conn = open_in_memory().unwrap();
    let s = settings::get(&conn).unwrap();
    assert_eq!((s.done_archive_days, s.done_archive_meeting_days), (30, 2));
    settings::update(
        &mut conn,
        UpdateSettings {
            done_archive_days: Some(7),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(settings::get(&conn).unwrap().done_archive_days, 7);
    let t = finished(&mut conn, "A week", 7);
    assert_eq!(
        done_sweep::archive_old_done(&mut conn, TODAY).unwrap(),
        vec![t.id]
    );
    let too_many = settings::update(
        &mut conn,
        UpdateSettings {
            done_archive_days: Some(366),
            ..Default::default()
        },
    );
    assert!(matches!(too_many, Err(StoreError::Invalid(_))));
}

#[test]
fn a_sub_task_of_an_open_parent_and_a_parent_of_an_open_sub_task_stay() {
    let mut conn = open_in_memory().unwrap();
    let parent = task(&mut conn, "Parent");
    let done_child = finished(&mut conn, "Done child", 60);
    link(&mut conn, EdgeType::SubtaskOf, done_child.id, parent.id);
    assert!(done_sweep::archive_old_done(&mut conn, TODAY).unwrap().is_empty());

    // Once the parent is finished too, both can go.
    tasks::update(
        &mut conn,
        parent.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    conn.execute(
        "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
        rusqlite::params![
            parent.id.to_string(),
            format!("{}T12:00:00.000Z", TODAY - Duration::days(60))
        ],
    )
    .unwrap();
    let gone = done_sweep::archive_old_done(&mut conn, TODAY).unwrap();
    assert_eq!(gone.len(), 2);

    // A done parent with an open sub-task stays.
    let p2 = finished(&mut conn, "Done parent", 60);
    let open_child = task(&mut conn, "Open child");
    link(&mut conn, EdgeType::SubtaskOf, open_child.id, p2.id);
    assert!(done_sweep::archive_old_done(&mut conn, TODAY).unwrap().is_empty());
}

#[test]
fn a_task_that_blocks_open_work_or_has_a_live_ping_stays() {
    let mut conn = open_in_memory().unwrap();
    let blocker = finished(&mut conn, "Blocker", 60);
    let waiting = task(&mut conn, "Waiting");
    link(&mut conn, EdgeType::Blocks, blocker.id, waiting.id);
    let pinged = finished(&mut conn, "Pinged", 60);
    pings::create(
        &mut conn,
        today(),
        CreatePing {
            target: NodeRef::new(NodeType::Task, pinged.id),
            note: "check".into(),
            on_date: today() + Duration::days(1),
        },
    )
    .unwrap();
    let plain = finished(&mut conn, "Plain", 60);

    let gone = done_sweep::archive_old_done(&mut conn, TODAY).unwrap();
    let names: Vec<String> = gone
        .iter()
        .map(|id| tasks::get(&conn, *id).unwrap().title)
        .collect();
    assert_eq!(names, ["Plain"], "the blocker and the pinged task stay");
    assert_eq!(gone, vec![plain.id]);

    // The blocked task is finished too: its blocker has nothing left to explain, and goes.
    tasks::update(
        &mut conn,
        waiting.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    conn.execute(
        "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
        rusqlite::params![
            waiting.id.to_string(),
            format!("{}T12:00:00.000Z", TODAY - Duration::days(1))
        ],
    )
    .unwrap();
    let gone = done_sweep::archive_old_done(&mut conn, TODAY).unwrap();
    assert_eq!(gone, vec![blocker.id]);
}

#[test]
fn it_is_one_undoable_step_that_the_archive_can_reverse() {
    let mut conn = open_in_memory().unwrap();
    let a = finished(&mut conn, "A", 40);
    let b = finished(&mut conn, "B", 45);
    done_sweep::archive_old_done(&mut conn, TODAY).unwrap();
    assert!(archived(&conn, a.id) && archived(&conn, b.id));
    nodes::unarchive(&mut conn, NodeRef::new(NodeType::Task, a.id)).unwrap();
    assert!(!archived(&conn, a.id));
}
