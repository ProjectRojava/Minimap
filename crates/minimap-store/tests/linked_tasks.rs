//! A new task made from another one and linked to it in the same step.

use minimap_store::*;
use minimap_types::*;

fn task(conn: &mut Connection, title: &str, project: Option<uuid::Uuid>) -> Task {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: project,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: Some(1),
            recurrence: None,
        },
    )
    .unwrap()
}

fn edge_between(conn: &Connection, kind: EdgeType, from: uuid::Uuid, to: uuid::Uuid) -> bool {
    edges::list_active_of_type(conn, kind)
        .unwrap()
        .iter()
        .any(|e| e.from_id == from && e.to_id == to)
}

#[test]
fn the_new_task_blocks_follows_or_relates_to_the_one_it_came_from() {
    let mut conn = open_in_memory().unwrap();
    let source = task(&mut conn, "Source", None);
    let before = tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::Blocks,
        "First".into(),
        None,
    )
    .unwrap();
    let after = tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::BlockedBy,
        "Later".into(),
        None,
    )
    .unwrap();
    let near = tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::RelatesTo,
        "Near".into(),
        None,
    )
    .unwrap();
    assert!(edge_between(&conn, EdgeType::Blocks, before.id, source.id));
    assert!(edge_between(&conn, EdgeType::Blocks, source.id, after.id));
    assert!(edge_between(&conn, EdgeType::RelatesTo, source.id, near.id));
}

#[test]
fn it_joins_the_project_and_priority_and_an_archived_task_is_refused() {
    let mut conn = open_in_memory().unwrap();
    let project = projects::create(
        &mut conn,
        CreateProject {
            title: "Launch".into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let source = task(&mut conn, "Source", Some(project.id));
    let made = tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::Blocks,
        "New".into(),
        None,
    )
    .unwrap();
    assert_eq!((made.project_id, made.priority), (Some(project.id), 1));
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, source.id)).unwrap();
    assert!(
        tasks::create_linked(&mut conn, source.id, LinkRelation::Blocks, "x".into(), None).is_err()
    );
}
