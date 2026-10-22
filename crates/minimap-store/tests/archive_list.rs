//! The Archive: everything that has been archived, in one list; restoring; and the finished tasks
//! that were archived still counting for the week they were done in.

use minimap_store::*;
use minimap_types::*;
use time::{macros::date, Duration};

fn task(conn: &mut Connection, title: &str, assignee: AssigneeChoice) -> Task {
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
            assignee,
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

fn person(conn: &mut Connection, name: &str) -> Person {
    people::create(
        conn,
        CreatePerson {
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: false,
            notes: String::new(),
        },
    )
    .unwrap()
}

#[test]
fn the_archive_lists_every_kind_of_item_newest_first() {
    let mut conn = open_in_memory().unwrap();
    let t = task(&mut conn, "A task", AssigneeChoice::Nobody);
    let p = person(&mut conn, "Priya");
    let o = objectives::create(
        &mut conn,
        CreateObjective {
            ongoing: false,
            review_every_days: None,
            title: "Launch".into(),
            description: String::new(),
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    assert!(archive::list_all(&conn).unwrap().is_empty());

    for node in [
        NodeRef::new(NodeType::Task, t.id),
        NodeRef::new(NodeType::Person, p.id),
        NodeRef::new(NodeType::Objective, o.id),
    ] {
        nodes::archive(&mut conn, node).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let list = archive::list_all(&conn).unwrap();
    let labels: Vec<&str> = list.iter().map(|i| i.node.label.as_str()).collect();
    assert_eq!(labels, ["Launch", "Priya", "A task"]);
    assert!(list.iter().all(|i| i.node.archived));
    assert_eq!(list[2].detail, "todo · Test project");
    assert_eq!(list[0].detail, "");
    assert_eq!(list[0].archived_on, today());
}

#[test]
fn restoring_brings_an_item_back_with_its_links() {
    let mut conn = open_in_memory().unwrap();
    let me = person(&mut conn, "Me");
    let t = task(&mut conn, "Mine", AssigneeChoice::Person(me.id));
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, t.id)).unwrap();
    assert_eq!(archive::list_all(&conn).unwrap().len(), 1);

    archive::restore(&mut conn, NodeRef::new(NodeType::Task, t.id)).unwrap();
    assert!(archive::list_all(&conn).unwrap().is_empty());
    assert!(tasks::get(&conn, t.id).unwrap().archived_at.is_none());
    let assigned = edges::list_active_of_type(&conn, EdgeType::AssignedTo).unwrap();
    assert_eq!(assigned.len(), 1);
}

#[test]
fn archived_done_tasks_still_count_for_the_week_they_were_finished_in() {
    let mut conn = open_in_memory().unwrap();
    let me = person(&mut conn, "Me");
    let done = task(&mut conn, "Shipped", AssigneeChoice::Person(me.id));
    let other = task(&mut conn, "Last month", AssigneeChoice::Nobody);
    let open = task(&mut conn, "Still open", AssigneeChoice::Nobody);
    for (id, day) in [
        (done.id, date!(2027 - 03 - 02)),
        (other.id, date!(2027 - 02 - 02)),
    ] {
        tasks::update(
            &mut conn,
            id,
            UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
            rusqlite::params![id.to_string(), format!("{day}T12:00:00.000Z")],
        )
        .unwrap();
        nodes::archive(&mut conn, NodeRef::new(NodeType::Task, id)).unwrap();
    }
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, open.id)).unwrap();

    let week =
        archive::done_rows_between(&conn, date!(2027 - 03 - 01), date!(2027 - 03 - 07)).unwrap();
    assert_eq!(week.len(), 1, "only the finished one, only in its week");
    assert_eq!(week[0].task.title, "Shipped");
    assert_eq!(week[0].assignee.as_ref().unwrap().label, "Me");
    assert_eq!(week[0].project.as_ref().unwrap().label, "Test project");
    let wider = archive::done_rows_between(
        &conn,
        date!(2027 - 03 - 01) - Duration::days(30),
        date!(2027 - 03 - 07),
    )
    .unwrap();
    assert_eq!(wider.len(), 2);
}
